use std::{
    fs::{self, File, Permissions},
    io::{Read, Write as _},
    os::unix::fs::PermissionsExt as _,
    path::{Path, PathBuf},
};

use sha2::{Digest as _, Sha256};
use tempfile::NamedTempFile;

use crate::{ErrorCategory, UpdateError, UpdatePlan};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstallOutcome {
    UpToDate { current: String, latest: String },
    Installed { previous: String, installed: String },
}

/// Resolves `/proc/self/exe` to the actual regular file being executed.
///
/// # Errors
///
/// Returns an error when procfs cannot be resolved or the target is not a regular file.
pub fn resolved_executable() -> Result<PathBuf, UpdateError> {
    let path = fs::canonicalize("/proc/self/exe")
        .map_err(|error| UpdateError::io("could not resolve the running executable", &error))?;
    let metadata = fs::metadata(&path)
        .map_err(|error| UpdateError::io("could not inspect the running executable", &error))?;
    if !metadata.is_file() {
        return Err(UpdateError::new(
            ErrorCategory::Other,
            format!(
                "running executable is not a regular file: {}",
                path.display()
            ),
        ));
    }
    Ok(path)
}

pub(crate) fn install_response(
    path: &Path,
    plan: &UpdatePlan,
    response: impl Read,
) -> Result<InstallOutcome, UpdateError> {
    install_reader(path, plan, response, &RealFileSystem)
}

trait FileSystem {
    fn create_temporary(&self, parent: &Path) -> std::io::Result<NamedTempFile>;
    fn write(&self, file: &mut NamedTempFile, bytes: &[u8]) -> std::io::Result<()>;
    fn set_executable(&self, path: &Path) -> std::io::Result<()>;
    fn sync_file(&self, file: &File) -> std::io::Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()>;
    fn sync_directory(&self, path: &Path) -> std::io::Result<()>;
}

struct RealFileSystem;

impl FileSystem for RealFileSystem {
    fn create_temporary(&self, parent: &Path) -> std::io::Result<NamedTempFile> {
        NamedTempFile::new_in(parent)
    }

    fn write(&self, file: &mut NamedTempFile, bytes: &[u8]) -> std::io::Result<()> {
        file.write_all(bytes)
    }

    fn set_executable(&self, path: &Path) -> std::io::Result<()> {
        fs::set_permissions(path, Permissions::from_mode(0o755))
    }

    fn sync_file(&self, file: &File) -> std::io::Result<()> {
        file.sync_all()
    }

    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        fs::rename(from, to)
    }

    fn sync_directory(&self, path: &Path) -> std::io::Result<()> {
        File::open(path)?.sync_all()
    }
}

fn install_reader(
    path: &Path,
    plan: &UpdatePlan,
    mut source: impl Read,
    file_system: &impl FileSystem,
) -> Result<InstallOutcome, UpdateError> {
    let parent = path.parent().ok_or_else(|| {
        UpdateError::new(
            ErrorCategory::Other,
            "running executable has no parent directory",
        )
    })?;
    let mut temporary = file_system.create_temporary(parent).map_err(|error| {
        UpdateError::io("could not create update beside the executable", &error)
    })?;
    let mut digest = Sha256::new();
    let mut length = 0_u64;
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = source
            .read(&mut buffer)
            .map_err(|error| UpdateError::new(ErrorCategory::Network, error.to_string()))?;
        if read == 0 {
            break;
        }
        length = length.saturating_add(read as u64);
        if length > plan.asset.length {
            return Err(verification(
                "downloaded asset is longer than its manifest length",
            ));
        }
        file_system
            .write(&mut temporary, &buffer[..read])
            .map_err(|error| UpdateError::io("could not write the update", &error))?;
        digest.update(&buffer[..read]);
    }
    if length != plan.asset.length {
        return Err(verification(format!(
            "downloaded asset length was {length}, expected {}",
            plan.asset.length
        )));
    }
    let actual = format!("{:x}", digest.finalize());
    if actual != plan.asset.sha256 {
        return Err(verification(
            "downloaded asset SHA-256 did not match the manifest",
        ));
    }
    file_system
        .set_executable(temporary.path())
        .map_err(|error| UpdateError::io("could not make the update executable", &error))?;
    file_system
        .sync_file(temporary.as_file())
        .map_err(|error| UpdateError::io("could not flush the update", &error))?;
    file_system
        .rename(temporary.path(), path)
        .map_err(|error| UpdateError::io("could not replace the running executable", &error))?;
    file_system
        .sync_directory(parent)
        .map_err(|error| UpdateError::io("could not flush the executable directory", &error))?;
    Ok(InstallOutcome::Installed {
        previous: plan.current.clone(),
        installed: plan.latest.clone(),
    })
}

fn verification(message: impl Into<String>) -> UpdateError {
    UpdateError::new(ErrorCategory::Verification, message)
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, io, rc::Rc};

    use super::*;
    use crate::Asset;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Stage {
        Create,
        Write,
        Mode,
        FileSync,
        Rename,
        DirectorySync,
    }

    struct FaultingFileSystem {
        fault: Stage,
        reached: Rc<Cell<Option<Stage>>>,
    }

    impl FaultingFileSystem {
        fn fail(&self, stage: Stage) -> io::Result<()> {
            self.reached.set(Some(stage));
            if self.fault == stage {
                Err(io::Error::other("injected fault"))
            } else {
                Ok(())
            }
        }
    }

    impl FileSystem for FaultingFileSystem {
        fn create_temporary(&self, parent: &Path) -> io::Result<NamedTempFile> {
            self.fail(Stage::Create)?;
            NamedTempFile::new_in(parent)
        }

        fn write(&self, file: &mut NamedTempFile, bytes: &[u8]) -> io::Result<()> {
            self.fail(Stage::Write)?;
            RealFileSystem.write(file, bytes)
        }

        fn set_executable(&self, path: &Path) -> io::Result<()> {
            self.fail(Stage::Mode)?;
            RealFileSystem.set_executable(path)
        }

        fn sync_file(&self, file: &File) -> io::Result<()> {
            self.fail(Stage::FileSync)?;
            RealFileSystem.sync_file(file)
        }

        fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
            self.fail(Stage::Rename)?;
            RealFileSystem.rename(from, to)
        }

        fn sync_directory(&self, path: &Path) -> io::Result<()> {
            self.fail(Stage::DirectorySync)?;
            RealFileSystem.sync_directory(path)
        }
    }

    fn plan(bytes: &[u8]) -> UpdatePlan {
        UpdatePlan {
            current: "1111111111111111111111111111111111111111".to_owned(),
            latest: "2222222222222222222222222222222222222222".to_owned(),
            asset: Asset {
                name: "diffo-x86_64-unknown-linux-gnu".to_owned(),
                length: bytes.len() as u64,
                target: "x86_64-unknown-linux-gnu".to_owned(),
                sha256: format!("{:x}", Sha256::digest(bytes)),
            },
        }
    }

    #[test]
    fn only_complete_verified_bytes_replace_the_executable() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("diffo");
        fs::write(&path, b"old").unwrap();
        let update = b"complete-new-image";

        install_reader(&path, &plan(update), update.as_slice(), &RealFileSystem).unwrap();

        assert_eq!(fs::read(&path).unwrap(), update);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }

    #[test]
    fn length_and_digest_failures_leave_the_executable_unchanged() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("diffo");
        for (declared, downloaded) in [
            (b"right".as_slice(), b"short".as_slice()),
            (b"right", b"wrong"),
        ] {
            fs::write(&path, b"old").unwrap();
            assert!(install_reader(&path, &plan(declared), downloaded, &RealFileSystem).is_err());
            assert_eq!(fs::read(&path).unwrap(), b"old");
        }
    }

    #[test]
    fn every_filesystem_fault_before_rename_preserves_the_old_image() {
        let update = b"complete-new-image";
        for stage in [
            Stage::Create,
            Stage::Write,
            Stage::Mode,
            Stage::FileSync,
            Stage::Rename,
        ] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("diffo");
            fs::write(&path, b"old").unwrap();
            let reached = Rc::new(Cell::new(None));
            let file_system = FaultingFileSystem {
                fault: stage,
                reached: Rc::clone(&reached),
            };

            assert!(install_reader(&path, &plan(update), update.as_slice(), &file_system).is_err());
            assert_eq!(reached.get(), Some(stage));
            assert_eq!(fs::read(&path).unwrap(), b"old");
            assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
        }
    }

    #[test]
    fn a_directory_flush_fault_can_only_expose_the_complete_verified_image() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("diffo");
        fs::write(&path, b"old").unwrap();
        let update = b"complete-new-image";
        let file_system = FaultingFileSystem {
            fault: Stage::DirectorySync,
            reached: Rc::new(Cell::new(None)),
        };

        assert!(install_reader(&path, &plan(update), update.as_slice(), &file_system).is_err());
        assert_eq!(fs::read(&path).unwrap(), update);
    }

    #[test]
    fn permission_failure_is_classified_without_changing_the_executable() {
        struct PermissionFileSystem;
        impl FileSystem for PermissionFileSystem {
            fn create_temporary(&self, _parent: &Path) -> io::Result<NamedTempFile> {
                Err(io::Error::from(io::ErrorKind::PermissionDenied))
            }
            fn write(&self, _file: &mut NamedTempFile, _bytes: &[u8]) -> io::Result<()> {
                unreachable!()
            }
            fn set_executable(&self, _path: &Path) -> io::Result<()> {
                unreachable!()
            }
            fn sync_file(&self, _file: &File) -> io::Result<()> {
                unreachable!()
            }
            fn rename(&self, _from: &Path, _to: &Path) -> io::Result<()> {
                unreachable!()
            }
            fn sync_directory(&self, _path: &Path) -> io::Result<()> {
                unreachable!()
            }
        }

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("diffo with spaces");
        fs::write(&path, b"old").unwrap();
        let update = b"complete-new-image";
        let error = install_reader(
            &path,
            &plan(update),
            update.as_slice(),
            &PermissionFileSystem,
        )
        .unwrap_err();

        assert_eq!(error.category(), ErrorCategory::Permission);
        assert_eq!(fs::read(&path).unwrap(), b"old");
        assert_eq!(
            crate::sudo_command(&path),
            format!("sudo '{}' update", path.display())
        );
    }
}
