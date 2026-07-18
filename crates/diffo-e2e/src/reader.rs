use std::{io::Read, sync::mpsc::SyncSender};

pub(super) fn read_output(mut reader: Box<dyn Read + Send>, output: &SyncSender<Vec<u8>>) {
    let mut buffer = [0_u8; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(length) if output.send(buffer[..length].to_vec()).is_err() => break,
            Ok(_) => {}
        }
    }
}
