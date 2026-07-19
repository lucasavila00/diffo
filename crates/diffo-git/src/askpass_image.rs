use std::{
    env, fs,
    fs::{File, OpenOptions},
    io::{self, Seek as _, SeekFrom},
    os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _},
    path::PathBuf,
    sync::Mutex,
};

use anyhow::{Context, Result};

pub(crate) struct OwnedAskpass {
    state: Mutex<AskpassState>,
}

enum AskpassState {
    Captured(File),
    Prepared(PreparedAskpass),
}

struct PreparedAskpass {
    _directory: tempfile::TempDir,
    executable: PathBuf,
}

impl OwnedAskpass {
    pub(crate) fn capture() -> Result<Self> {
        let running_path =
            env::current_exe().context("failed to locate the running Diffo executable")?;
        let running =
            File::open(&running_path).context("failed to open the running Diffo executable")?;
        Ok(Self {
            state: Mutex::new(AskpassState::Captured(running)),
        })
    }

    pub(crate) fn executable(&self) -> Result<PathBuf> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("private askpass image state is unavailable"))?;
        if let AskpassState::Captured(running) = &mut *state {
            let prepared = PreparedAskpass::prepare(running)?;
            *state = AskpassState::Prepared(prepared);
        }
        match &*state {
            AskpassState::Prepared(prepared) => Ok(prepared.executable.clone()),
            AskpassState::Captured(_) => {
                anyhow::bail!("private askpass image was not materialized")
            }
        }
    }

    #[cfg(test)]
    fn is_prepared(&self) -> bool {
        self.state
            .lock()
            .is_ok_and(|state| matches!(*state, AskpassState::Prepared(_)))
    }
}

impl PreparedAskpass {
    fn prepare(running: &mut File) -> Result<Self> {
        running
            .seek(SeekFrom::Start(0))
            .context("failed to rewind the running Diffo executable")?;
        let directory = tempfile::Builder::new()
            .prefix("diffo-askpass-image-")
            .tempdir()
            .context("failed to create the private askpass directory")?;
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700))
            .context("failed to protect the private askpass directory")?;

        let temporary = directory.path().join("askpass.tmp");
        let executable = directory.path().join("askpass");
        let mut prepared = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o700)
            .open(&temporary)
            .context("failed to create the private askpass executable")?;
        io::copy(running, &mut prepared)
            .context("failed to copy the running Diffo executable for askpass")?;
        drop(prepared);
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o700))
            .context("failed to protect the private askpass executable")?;
        fs::rename(&temporary, &executable)
            .context("failed to publish the private askpass executable")?;

        Ok(Self {
            _directory: directory,
            executable,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captured_image_is_materialized_lazily_and_privately() {
        let image = OwnedAskpass::capture().unwrap();
        assert!(!image.is_prepared());

        let executable = image.executable().unwrap();
        assert!(image.is_prepared());
        assert_eq!(image.executable().unwrap(), executable);

        assert_eq!(
            fs::metadata(&executable).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(executable.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(executable).unwrap().len(),
            fs::metadata(env::current_exe().unwrap()).unwrap().len()
        );
    }
}
