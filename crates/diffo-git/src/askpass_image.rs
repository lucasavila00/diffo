use std::{
    env, fs,
    fs::{File, OpenOptions},
    io,
    os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

pub(crate) struct PreparedAskpass {
    _directory: tempfile::TempDir,
    executable: PathBuf,
}

impl PreparedAskpass {
    pub(crate) fn prepare() -> Result<Self> {
        let running_path =
            env::current_exe().context("failed to locate the running Diffo executable")?;
        let mut running =
            File::open(&running_path).context("failed to open the running Diffo executable")?;
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
        io::copy(&mut running, &mut prepared)
            .context("failed to copy the running Diffo executable for askpass")?;
        prepared
            .sync_all()
            .context("failed to publish the private askpass executable")?;
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

    pub(crate) fn executable(&self) -> &Path {
        &self.executable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepared_image_is_private_and_executable() {
        let image = PreparedAskpass::prepare().unwrap();

        assert_eq!(
            fs::metadata(image.executable())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(image.executable().parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(image.executable()).unwrap().len(),
            fs::metadata(env::current_exe().unwrap()).unwrap().len()
        );
    }
}
