use std::{
    io::{Read as _, Write as _},
    os::unix::net::UnixListener,
    process::Command,
    thread,
};

use anyhow::Result;

#[test]
fn private_askpass_startup_returns_the_broker_answer() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let socket = directory.path().join("askpass.sock");
    let listener = UnixListener::bind(&socket)?;
    let server = thread::spawn(move || -> Result<()> {
        let (mut stream, _) = listener.accept()?;
        let mut kind = [0];
        stream.read_exact(&mut kind)?;
        assert_eq!(kind, [1]);
        assert_eq!(read_field(&mut stream)?, "example.com");
        stream.write_all(&[0])?;
        write_field(&mut stream, "alice")?;
        Ok(())
    });

    let output = Command::new(env!("CARGO_BIN_EXE_diffo"))
        .arg("Username for 'https://person@example.com': ")
        .env("DIFFO_INTERNAL_ASKPASS", "1")
        .env("DIFFO_INTERNAL_ASKPASS_SOCKET", &socket)
        .output()?;

    assert!(output.status.success());
    assert_eq!(output.stdout, b"alice\n");
    assert!(output.stderr.is_empty());
    server.join().expect("askpass server stopped")?;
    Ok(())
}

#[test]
fn private_askpass_startup_rejects_unknown_prompts_without_output() -> Result<()> {
    let output = Command::new(env!("CARGO_BIN_EXE_diffo"))
        .arg("Password: ")
        .env("DIFFO_INTERNAL_ASKPASS", "1")
        .env("DIFFO_INTERNAL_ASKPASS_SOCKET", "/does/not/exist")
        .output()?;

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    Ok(())
}

fn read_field(stream: &mut impl std::io::Read) -> Result<String> {
    let mut length = [0; 4];
    stream.read_exact(&mut length)?;
    let mut field = vec![0; u32::from_be_bytes(length) as usize];
    stream.read_exact(&mut field)?;
    Ok(String::from_utf8(field)?)
}

fn write_field(stream: &mut impl std::io::Write, field: &str) -> Result<()> {
    stream.write_all(&u32::try_from(field.len())?.to_be_bytes())?;
    stream.write_all(field.as_bytes())?;
    Ok(())
}
