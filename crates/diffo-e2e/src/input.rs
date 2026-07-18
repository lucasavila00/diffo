use anyhow::{Result, bail};

use crate::Key;

pub(super) fn key_bytes(key: Key) -> Result<Vec<u8>> {
    Ok(match key {
        Key::Char(character) => character.to_string().into_bytes(),
        Key::Enter => b"\r".to_vec(),
        Key::Escape => b"\x1b".to_vec(),
        Key::Up => b"\x1b[A".to_vec(),
        Key::Down => b"\x1b[B".to_vec(),
        Key::Right => b"\x1b[C".to_vec(),
        Key::Left => b"\x1b[D".to_vec(),
        Key::Home => b"\x1b[H".to_vec(),
        Key::End => b"\x1b[F".to_vec(),
        Key::PageUp => b"\x1b[5~".to_vec(),
        Key::PageDown => b"\x1b[6~".to_vec(),
        Key::Function(1) => b"\x1bOP".to_vec(),
        Key::Function(2) => b"\x1bOQ".to_vec(),
        Key::Function(number) => bail!("unsupported function key F{number}"),
        Key::Ctrl(character) if character.is_ascii_alphabetic() => {
            vec![(character.to_ascii_lowercase() as u8) & 0x1f]
        }
        Key::Ctrl(character) => bail!("unsupported control key {character:?}"),
    })
}
