use std::{
    fs,
    fs::File,
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};

use super::support::{TestRepository, git};

const HOST_ALIAS: &str = "diffo-e2e";

#[derive(Clone, Copy)]
pub(super) enum Authentication<'a> {
    PublicKey { passphrase: &'a str },
    Password,
}

pub(super) struct LocalSshServer {
    child: Child,
    client_config: PathBuf,
    known_hosts: PathBuf,
}

impl LocalSshServer {
    pub(super) fn start(
        repository: &TestRepository,
        authentication: Authentication<'_>,
    ) -> Result<Self> {
        let root = repository.root.path().join("ssh");
        fs::create_dir(&root).context("create SSH test directory")?;
        let host_key = root.join("host-key");
        generate_key(&host_key, "")?;
        let known_hosts = root.join("known-hosts");
        fs::write(&known_hosts, []).context("create isolated known_hosts")?;

        let (client_identity, public_key_authentication, password_authentication) =
            match authentication {
                Authentication::PublicKey { passphrase } => {
                    let client_key = root.join("client-key");
                    generate_key(&client_key, passphrase)?;
                    let authorized_keys = root.join("authorized-keys");
                    fs::copy(client_key.with_extension("pub"), &authorized_keys)
                        .context("install SSH test authorized key")?;
                    (Some(client_key), true, false)
                }
                Authentication::Password => (None, false, true),
            };

        let port = unused_loopback_port()?;
        let user = current_user()?;
        let server_config = root.join("sshd-config");
        fs::write(
            &server_config,
            server_configuration(
                port,
                &host_key,
                &root,
                public_key_authentication,
                password_authentication,
            ),
        )
        .context("write SSH server configuration")?;
        let client_config = root.join("ssh-config");
        fs::write(
            &client_config,
            client_configuration(port, &user, &known_hosts, client_identity.as_deref()),
        )
        .context("write SSH client configuration")?;

        let log = root.join("sshd.log");
        let log_file = File::create(&log).context("create SSH server log")?;
        let mut child = Command::new(sshd_path()?)
            .args(["-D", "-e", "-f"])
            .arg(&server_config)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(log_file))
            .spawn()
            .context("start local OpenSSH server")?;
        wait_until_ready(&mut child, port, &log)?;

        let remote_url = format!(
            "{HOST_ALIAS}:{}",
            repository.root.path().join("remote.git").display()
        );
        git(
            &repository.worktree,
            &["remote", "set-url", "origin", &remote_url],
        )?;

        Ok(Self {
            child,
            client_config,
            known_hosts,
        })
    }

    pub(super) fn command(&self) -> String {
        format!("ssh -F {}", self.client_config.display())
    }

    pub(super) fn known_hosts(&self) -> &Path {
        &self.known_hosts
    }

    pub(super) fn trust_host(&self) -> Result<()> {
        let public_key = fs::read_to_string(
            self.client_config
                .parent()
                .context("SSH client config has no parent")?
                .join("host-key.pub"),
        )
        .context("read SSH host public key")?;
        let mut fields = public_key.split_whitespace();
        let kind = fields.next().context("host public key has no kind")?;
        let key = fields.next().context("host public key has no key")?;
        fs::write(&self.known_hosts, format!("{HOST_ALIAS} {kind} {key}\n"))
            .context("trust SSH test host")
    }
}

impl Drop for LocalSshServer {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn generate_key(path: &Path, passphrase: &str) -> Result<()> {
    let status = Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", passphrase, "-f"])
        .arg(path)
        .status()
        .context("run ssh-keygen")?;
    if status.success() {
        Ok(())
    } else {
        bail!("ssh-keygen failed with {status}")
    }
}

fn unused_loopback_port() -> Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).context("reserve SSH test port")?;
    Ok(listener
        .local_addr()
        .context("read SSH test address")?
        .port())
}

fn current_user() -> Result<String> {
    let output = Command::new("id")
        .arg("-un")
        .output()
        .context("find current user for SSH test")?;
    if !output.status.success() {
        bail!("id -un failed with {}", output.status);
    }
    String::from_utf8(output.stdout)
        .context("current user is not UTF-8")
        .map(|user| user.trim().to_owned())
}

fn sshd_path() -> Result<&'static Path> {
    [
        Path::new("/usr/sbin/sshd"),
        Path::new("/usr/local/sbin/sshd"),
    ]
    .into_iter()
    .find(|path| path.is_file())
    .context("OpenSSH server is required at /usr/sbin/sshd or /usr/local/sbin/sshd")
}

fn server_configuration(
    port: u16,
    host_key: &Path,
    root: &Path,
    public_key_authentication: bool,
    password_authentication: bool,
) -> String {
    format!(
        "Port {port}\n\
         ListenAddress 127.0.0.1\n\
         HostKey {}\n\
         PidFile {}\n\
         AuthorizedKeysFile {}\n\
         PubkeyAuthentication {}\n\
         PasswordAuthentication {}\n\
         KbdInteractiveAuthentication no\n\
         UsePAM no\n\
         StrictModes no\n\
         PermitRootLogin prohibit-password\n\
         AllowTcpForwarding no\n\
         X11Forwarding no\n\
         PrintMotd no\n\
         LogLevel ERROR\n",
        host_key.display(),
        root.join("sshd.pid").display(),
        root.join("authorized-keys").display(),
        yes_no(public_key_authentication),
        yes_no(password_authentication),
    )
}

fn client_configuration(
    port: u16,
    user: &str,
    known_hosts: &Path,
    identity: Option<&Path>,
) -> String {
    let identity = identity.map_or_else(
        || "IdentityFile none\n  PubkeyAuthentication no\n  PreferredAuthentications password\n  NumberOfPasswordPrompts 1\n".to_owned(),
        |path| {
            format!(
                "IdentityFile {}\n  IdentitiesOnly yes\n  IdentityAgent none\n  PreferredAuthentications publickey\n",
                path.display()
            )
        },
    );
    format!(
        "Host {HOST_ALIAS}\n\
           HostName 127.0.0.1\n\
           Port {port}\n\
           User {user}\n\
           HostKeyAlias {HOST_ALIAS}\n\
           UserKnownHostsFile {}\n\
           GlobalKnownHostsFile /dev/null\n\
           StrictHostKeyChecking ask\n\
           LogLevel ERROR\n  {identity}",
        known_hosts.display(),
    )
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn wait_until_ready(child: &mut Child, port: u16, log: &Path) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return Ok(());
        }
        if let Some(status) = child.try_wait().context("poll local OpenSSH server")? {
            bail!(
                "local OpenSSH server exited with {status}\n{}",
                fs::read_to_string(log).unwrap_or_default()
            );
        }
        thread::sleep(Duration::from_millis(10));
    }
    bail!(
        "local OpenSSH server did not listen in time\n{}",
        fs::read_to_string(log).unwrap_or_default()
    )
}
