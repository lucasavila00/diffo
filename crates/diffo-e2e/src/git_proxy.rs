use std::{
    env,
    ffi::OsString,
    fs,
    io::Write as _,
    os::unix::fs::PermissionsExt as _,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitGatePhase {
    Before,
    After,
}

/// Test-only `git` proxy that blocks one matching invocation on an explicit FIFO gate.
///
/// Every unmatched invocation delegates directly to the real Git executable. A matching
/// invocation delegates before or after the gate according to [`GitGatePhase`].
pub struct GitProxy {
    _directory: tempfile::TempDir,
    bin_directory: PathBuf,
    started: PathBuf,
    release: PathBuf,
    released: bool,
}

impl GitProxy {
    /// Create a one-shot gate for a Git subcommand such as `checkout` or `for-each-ref`.
    ///
    /// # Errors
    ///
    /// Returns an error when Git cannot be resolved or the proxy and FIFO cannot be created.
    pub fn new(subcommand: &str, phase: GitGatePhase) -> Result<Self> {
        if subcommand.is_empty()
            || !subcommand
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            bail!("invalid Git proxy subcommand")
        }
        let real_git = resolve_executable("git").context("find real Git executable")?;
        let directory = tempfile::tempdir().context("create Git proxy directory")?;
        let bin_directory = directory.path().join("bin");
        fs::create_dir(&bin_directory).context("create Git proxy bin directory")?;
        let started = directory.path().join("started");
        let release = directory.path().join("release");
        let claim = directory.path().join("claim");
        let status = Command::new("mkfifo")
            .arg(&release)
            .status()
            .context("create Git proxy release FIFO")?;
        if !status.success() {
            bail!("mkfifo failed for Git proxy")
        }
        let gate = match phase {
            GitGatePhase::Before => format!(
                "printf started > {started}\nIFS= read -r release < {release}\nexec {git} \"$@\"",
                started = shell_quote(&started),
                release = shell_quote(&release),
                git = shell_quote(&real_git),
            ),
            GitGatePhase::After => format!(
                "{git} \"$@\"\nstatus=$?\nprintf started > {started}\nIFS= read -r release < {release}\nexit $status",
                started = shell_quote(&started),
                release = shell_quote(&release),
                git = shell_quote(&real_git),
            ),
        };
        let script = format!(
            "#!/bin/sh\nif [ \"$1\" = {subcommand} ] && mkdir {claim} 2>/dev/null; then\n  {gate}\nfi\nexec {git} \"$@\"\n",
            subcommand = shell_quote(Path::new(subcommand)),
            claim = shell_quote(&claim),
            gate = gate.replace('\n', "\n  "),
            git = shell_quote(&real_git),
        );
        let proxy = bin_directory.join("git");
        fs::write(&proxy, script).context("write Git proxy")?;
        fs::set_permissions(&proxy, fs::Permissions::from_mode(0o755))
            .context("make Git proxy executable")?;
        Ok(Self {
            _directory: directory,
            bin_directory,
            started,
            release,
            released: false,
        })
    }

    /// Build a `PATH` that resolves this proxy before the caller's real Git.
    ///
    /// # Errors
    ///
    /// Returns an error when the process `PATH` cannot be joined safely.
    pub fn path(&self) -> Result<OsString> {
        let mut paths = vec![self.bin_directory.clone()];
        paths.extend(
            env::var_os("PATH")
                .as_deref()
                .map(env::split_paths)
                .into_iter()
                .flatten(),
        );
        env::join_paths(paths).context("build Git proxy PATH")
    }

    /// Wait until the matching Git invocation reaches the gate.
    ///
    /// # Errors
    ///
    /// Returns an error if the invocation does not reach the gate within five seconds.
    pub fn wait_until_blocked(&self) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if self.started.exists() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(10));
        }
        bail!("Git invocation did not reach the proxy gate")
    }

    /// Release the blocked Git invocation.
    ///
    /// # Errors
    ///
    /// Returns an error when the FIFO cannot be opened or written.
    pub fn release(&mut self) -> Result<()> {
        if self.released {
            return Ok(());
        }
        let mut release = fs::OpenOptions::new()
            .write(true)
            .open(&self.release)
            .context("open Git proxy release FIFO")?;
        release
            .write_all(b"release\n")
            .context("release Git proxy")?;
        self.released = true;
        Ok(())
    }
}

fn resolve_executable(name: &str) -> Option<PathBuf> {
    env::var_os("PATH")
        .as_deref()
        .map(env::split_paths)?
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

fn shell_quote(path: &Path) -> String {
    let value = path.as_os_str().to_string_lossy();
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_matching_command_blocks_and_then_delegates_to_real_git() {
        let mut proxy = GitProxy::new("version", GitGatePhase::Before).unwrap();
        let path = proxy.path().unwrap();
        let command = thread::spawn(move || {
            Command::new("git")
                .arg("version")
                .env("PATH", path)
                .output()
        });

        proxy.wait_until_blocked().unwrap();
        proxy.release().unwrap();
        let output = command.join().unwrap().unwrap();

        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).starts_with("git version"));
    }
}
