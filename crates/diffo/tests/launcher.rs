use std::{
    fs,
    io::{Read as _, Write as _},
    net::TcpListener,
    os::unix::fs::{PermissionsExt as _, symlink},
    process::Command,
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::{Signer as _, SigningKey};
use serde_json::json;
use sha2::{Digest as _, Sha256};

#[test]
fn invalid_arguments_are_rejected_before_repository_discovery() -> Result<()> {
    let directory = tempfile::tempdir().context("create non-repository directory")?;
    for arguments in [&["--help"][..], &["update", "extra"][..]] {
        let output = Command::new(diffo_e2e::diffo_binary(env!("CARGO_BIN_EXE_diffo"))?)
            .args(arguments)
            .current_dir(directory.path())
            .output()
            .context("run Diffo launcher")?;
        ensure!(!output.status.success(), "invalid arguments were accepted");
        let stderr = String::from_utf8_lossy(&output.stderr);
        ensure!(stderr.contains("usage: diffo [update]"), "{stderr}");
        ensure!(
            !stderr.contains("repository"),
            "repository initialized: {stderr}"
        );
    }
    Ok(())
}

#[test]
fn update_entry_path_verifies_and_atomically_replaces_its_own_file() -> Result<()> {
    const SEED: [u8; 32] = [
        0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c,
        0xc4, 0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae,
        0x7f, 0x60,
    ];
    let source = diffo_e2e::diffo_binary(env!("CARGO_BIN_EXE_diffo"))?;
    let directory = tempfile::tempdir().context("create update test directory")?;
    let installed = directory.path().join("diffo");
    fs::copy(&source, &installed).context("copy executable under test")?;
    fs::set_permissions(&installed, fs::Permissions::from_mode(0o755))?;
    let launcher = directory.path().join("diffo-link");
    symlink(&installed, &launcher).context("create executable symlink")?;
    let asset = fs::read(&source).context("read replacement asset")?;
    let manifest = serde_json::to_vec(&json!({
        "schema": 1,
        "version": "999.0.0",
        "assets": [{
            "name": "diffo-x86_64-unknown-linux-gnu",
            "length": asset.len(),
            "target": "x86_64-unknown-linux-gnu",
            "sha256": format!("{:x}", Sha256::digest(&asset)),
        }]
    }))?;
    let signing_key = SigningKey::from_bytes(&SEED);
    let signature = BASE64
        .encode(signing_key.sign(&manifest).to_bytes())
        .into_bytes();
    let public_key = BASE64.encode(signing_key.verifying_key().to_bytes());
    let listener = TcpListener::bind("127.0.0.1:0").context("bind update test server")?;
    let address = listener.local_addr()?;
    listener.set_nonblocking(true)?;
    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut served = 0;
        while served < 3 && Instant::now() < deadline {
            let (mut stream, _) = match listener.accept() {
                Ok(connection) => connection,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(error) => panic!("accept update request: {error}"),
            };
            let mut request = [0_u8; 1024];
            let read = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            let body = if request.starts_with("GET /update-v1.json.sig ") {
                signature.as_slice()
            } else if request.starts_with("GET /update-v1.json ") {
                manifest.as_slice()
            } else if request.starts_with("GET /diffo-x86_64-unknown-linux-gnu ") {
                asset.as_slice()
            } else {
                panic!("unexpected request: {request}");
            };
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(body).unwrap();
            served += 1;
        }
        assert_eq!(served, 3, "updater did not fetch every release asset");
    });

    let output = Command::new(&launcher)
        .arg("update")
        .env("DIFFO_UPDATE_BASE_URL", format!("http://{address}"))
        .env("DIFFO_UPDATE_PUBLIC_KEY", public_key)
        .output()
        .context("run embedded updater")?;
    server.join().expect("update test server completes");

    ensure!(
        output.status.success(),
        "updater failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    ensure!(
        String::from_utf8_lossy(&output.stdout).contains("Quit and relaunch"),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    ensure!(fs::read(&installed)? == fs::read(&source)?);
    ensure!(launcher.is_symlink());
    ensure!(fs::read_dir(directory.path())?.count() == 2);
    Ok(())
}
