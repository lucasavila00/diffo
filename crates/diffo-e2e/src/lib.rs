use std::{
    io::{Read, Write},
    path::Path,
    sync::mpsc::{Receiver, RecvTimeoutError, sync_channel},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use portable_pty::{Child, CommandBuilder, PtySize, native_pty_system};

const ROWS: u16 = 30;
const COLUMNS: u16 = 100;
const TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Key {
    Char(char),
    Enter,
    Escape,
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Ctrl(char),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Selector {
    Text(String),
    PanelAction {
        panel: String,
        action: String,
    },
    FileAction {
        panel: String,
        path: String,
        action: String,
    },
}

impl Selector {
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text(text.into())
    }

    #[must_use]
    pub fn panel_action(panel: impl Into<String>, action: impl Into<String>) -> Self {
        Self::PanelAction {
            panel: panel.into(),
            action: action.into(),
        }
    }

    #[must_use]
    pub fn file_action(
        panel: impl Into<String>,
        path: impl Into<String>,
        action: impl Into<String>,
    ) -> Self {
        Self::FileAction {
            panel: panel.into(),
            path: path.into(),
            action: action.into(),
        }
    }
}

pub struct DiffoPage {
    parser: vt100::Parser,
    output: Receiver<Vec<u8>>,
    writer: Option<Box<dyn Write + Send>>,
    child: Box<dyn Child + Send + Sync>,
    reader: Option<JoinHandle<()>>,
}

impl DiffoPage {
    pub fn launch(binary: impl AsRef<Path>, worktree: impl AsRef<Path>) -> Result<Self> {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: ROWS,
                cols: COLUMNS,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("open Diffo test PTY")?;
        let reader = pair
            .master
            .try_clone_reader()
            .context("clone Diffo PTY reader")?;
        let writer = pair.master.take_writer().context("open Diffo PTY writer")?;
        let mut command = CommandBuilder::new(binary.as_ref().as_os_str());
        command.cwd(worktree.as_ref().as_os_str());
        command.env("TERM", "xterm-256color");
        command.env("GIT_TERMINAL_PROMPT", "0");
        let child = pair
            .slave
            .spawn_command(command)
            .context("launch compiled Diffo CLI")?;
        drop(pair.slave);

        let (output_tx, output) = sync_channel(64);
        let reader = thread::spawn(move || read_output(reader, &output_tx));
        let mut page = Self {
            parser: vt100::Parser::new(ROWS, COLUMNS, 0),
            output,
            writer: Some(writer),
            child,
            reader: Some(reader),
        };
        page.wait_for_text("1/f1: commands")?;
        Ok(page)
    }

    pub fn press(&mut self, key: Key) -> Result<&mut Self> {
        let bytes = key_bytes(key)?;
        self.write(&bytes)?;
        Ok(self)
    }

    pub fn type_text(&mut self, text: &str) -> Result<&mut Self> {
        self.write(text.as_bytes())?;
        Ok(self)
    }

    pub fn click(&mut self, selector: Selector) -> Result<&mut Self> {
        let deadline = Instant::now() + TIMEOUT;
        let (column, row) = loop {
            self.pump_available();
            match self.locate(&selector)? {
                Some(position) => break position,
                None if Instant::now() < deadline => self.pump_until(deadline)?,
                None => {
                    bail!(
                        "selector {selector:?} was not visible within five seconds\n{}",
                        self.screen()
                    )
                }
            }
        };
        let x = column.saturating_add(1);
        let y = row.saturating_add(1);
        self.write(format!("\x1b[<0;{x};{y}M\x1b[<0;{x};{y}m").as_bytes())?;
        Ok(self)
    }

    pub fn wait_for_text(&mut self, text: &str) -> Result<&mut Self> {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            self.pump_available();
            if !find_text(&self.cells(), text).is_empty() {
                return Ok(self);
            }
            if Instant::now() >= deadline {
                bail!(
                    "text {text:?} was not visible within five seconds\n{}",
                    self.screen()
                );
            }
            self.pump_until(deadline)?;
        }
    }

    #[must_use]
    pub fn screen(&self) -> String {
        self.parser.screen().contents()
    }

    fn locate(&self, selector: &Selector) -> Result<Option<(u16, u16)>> {
        let cells = self.cells();
        let matches = match selector {
            Selector::Text(text) => find_text(&cells, text),
            Selector::PanelAction { panel, action } => find_panel_action(&cells, panel, action),
            Selector::FileAction {
                panel,
                path,
                action,
            } => find_file_action(&cells, panel, path, action),
        };
        match matches.as_slice() {
            [] => Ok(None),
            [position] => Ok(Some(*position)),
            _ => bail!(
                "selector {selector:?} matched {} visible controls\n{}",
                matches.len(),
                self.screen()
            ),
        }
    }

    fn cells(&self) -> Vec<Vec<String>> {
        (0..ROWS)
            .map(|row| {
                (0..COLUMNS)
                    .map(|column| {
                        self.parser
                            .screen()
                            .cell(row, column)
                            .map_or_else(|| " ".to_owned(), vt100::Cell::contents)
                    })
                    .collect()
            })
            .collect()
    }

    fn write(&mut self, bytes: &[u8]) -> Result<()> {
        let writer = self.writer.as_mut().context("Diffo PTY is closed")?;
        writer.write_all(bytes).context("write Diffo PTY input")?;
        writer.flush().context("flush Diffo PTY input")
    }

    fn pump_available(&mut self) {
        while let Ok(bytes) = self.output.try_recv() {
            self.parser.process(&bytes);
        }
    }

    fn pump_until(&mut self, deadline: Instant) -> Result<()> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match self
            .output
            .recv_timeout(remaining.min(Duration::from_millis(50)))
        {
            Ok(bytes) => self.parser.process(&bytes),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                if let Some(status) = self.child.try_wait().context("poll Diffo process")? {
                    bail!("Diffo exited before the expected UI appeared: {status:?}")
                }
                bail!("Diffo PTY output stopped")
            }
        }
        Ok(())
    }
}

impl Drop for DiffoPage {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.write(b"q");
            let deadline = Instant::now() + Duration::from_millis(250);
            while Instant::now() < deadline {
                if self.child.try_wait().ok().flatten().is_some() {
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
            if self.child.try_wait().ok().flatten().is_none() {
                let _ = self.child.kill();
                let _ = self.child.wait();
            }
        }
        self.writer.take();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

fn read_output(mut reader: Box<dyn Read + Send>, output: &std::sync::mpsc::SyncSender<Vec<u8>>) {
    let mut buffer = [0_u8; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(length) if output.send(buffer[..length].to_vec()).is_err() => break,
            Ok(_) => {}
        }
    }
}

fn key_bytes(key: Key) -> Result<Vec<u8>> {
    Ok(match key {
        Key::Char(character) => character.to_string().into_bytes(),
        Key::Enter => b"\r".to_vec(),
        Key::Escape => b"\x1b".to_vec(),
        Key::Up => b"\x1b[A".to_vec(),
        Key::Down => b"\x1b[B".to_vec(),
        Key::Right => b"\x1b[C".to_vec(),
        Key::Left => b"\x1b[D".to_vec(),
        Key::PageUp => b"\x1b[5~".to_vec(),
        Key::PageDown => b"\x1b[6~".to_vec(),
        Key::Ctrl(character) if character.is_ascii_alphabetic() => {
            vec![(character.to_ascii_lowercase() as u8) & 0x1f]
        }
        Key::Ctrl(character) => bail!("unsupported control key {character:?}"),
    })
}

fn find_panel_action(cells: &[Vec<String>], panel: &str, action: &str) -> Vec<(u16, u16)> {
    cells
        .iter()
        .enumerate()
        .filter(|(_, row)| !find_in_row(row, panel).is_empty())
        .flat_map(|(row, cells)| positions(row, find_in_row(cells, action), action))
        .collect()
}

fn find_file_action(
    cells: &[Vec<String>],
    panel: &str,
    path: &str,
    action: &str,
) -> Vec<(u16, u16)> {
    let Some(panel_row) = cells
        .iter()
        .position(|row| !find_in_row(row, panel).is_empty())
    else {
        return Vec::new();
    };
    let end = cells
        .iter()
        .enumerate()
        .skip(panel_row + 1)
        .find(|(_, row)| {
            !find_in_row(row, "Staged").is_empty() || !find_in_row(row, "Changes").is_empty()
        })
        .map_or(cells.len(), |(row, _)| row);
    cells
        .iter()
        .enumerate()
        .take(end)
        .skip(panel_row + 1)
        .filter(|(_, row)| !find_in_row(row, path).is_empty())
        .flat_map(|(row, cells)| positions(row, find_in_row(cells, action), action))
        .collect()
}

fn find_text(cells: &[Vec<String>], text: &str) -> Vec<(u16, u16)> {
    cells
        .iter()
        .enumerate()
        .flat_map(|(row, cells)| positions(row, find_in_row(cells, text), text))
        .collect()
}

fn positions(row: usize, starts: Vec<usize>, text: &str) -> Vec<(u16, u16)> {
    let center = text.chars().count() / 2;
    starts
        .into_iter()
        .filter_map(|column| {
            Some((
                u16::try_from(column.checked_add(center)?).ok()?,
                u16::try_from(row).ok()?,
            ))
        })
        .collect()
}

fn find_in_row(cells: &[String], text: &str) -> Vec<usize> {
    let needle = text
        .chars()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    if needle.is_empty() || needle.len() > cells.len() {
        return Vec::new();
    }
    cells
        .windows(needle.len())
        .enumerate()
        .filter_map(|(index, window)| (window == needle).then_some(index))
        .collect()
}
