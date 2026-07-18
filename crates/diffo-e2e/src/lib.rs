use std::{
    ffi::OsStr,
    io::{Read, Write},
    path::Path,
    sync::mpsc::{Receiver, RecvTimeoutError, sync_channel},
    thread,
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
    Home,
    End,
    PageUp,
    PageDown,
    Function(u8),
    Ctrl(char),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScrollDirection {
    Up,
    Down,
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
    SelectedRow(String),
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

    #[must_use]
    pub fn selected_row(text: impl Into<String>) -> Self {
        Self::SelectedRow(text.into())
    }
}

pub struct DiffoScreen {
    parser: vt100::Parser,
    output: Receiver<Vec<u8>>,
    writer: Option<Box<dyn Write + Send>>,
    child: Box<dyn Child + Send + Sync>,
}

impl DiffoScreen {
    /// Launches the compiled Diffo binary in a fixed-size terminal.
    ///
    /// # Errors
    ///
    /// Returns an error when the PTY or process cannot start, or the initial UI times out.
    pub fn launch(binary: impl AsRef<Path>, worktree: impl AsRef<Path>) -> Result<Self> {
        Self::launch_with_env(binary, worktree, &[])
    }

    /// Launches Diffo with developer-only environment hooks.
    ///
    /// # Errors
    ///
    /// Returns an error when the PTY or process cannot start, or the initial UI times out.
    pub fn launch_with_env(
        binary: impl AsRef<Path>,
        worktree: impl AsRef<Path>,
        environment: &[(&str, &OsStr)],
    ) -> Result<Self> {
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
        for (key, value) in environment {
            command.env(key, value);
        }
        let child = pair
            .slave
            .spawn_command(command)
            .context("launch compiled Diffo CLI")?;
        drop(pair.slave);

        let (output_tx, output) = sync_channel(64);
        thread::spawn(move || read_output(reader, &output_tx));
        let mut screen = Self {
            parser: vt100::Parser::new(ROWS, COLUMNS, 0),
            output,
            writer: Some(writer),
            child,
        };
        screen.wait_for_text("1/f1: commands")?;
        Ok(screen)
    }

    /// Sends one terminal key.
    ///
    /// # Errors
    ///
    /// Returns an error when the key is unsupported or the PTY cannot accept input.
    pub fn press(&mut self, key: Key) -> Result<&mut Self> {
        self.press_many(key, 1)
    }

    /// Sends the same terminal key several times in one write.
    ///
    /// # Errors
    ///
    /// Returns an error when the key is unsupported or the PTY cannot accept input.
    pub fn press_many(&mut self, key: Key, count: usize) -> Result<&mut Self> {
        let bytes = key_bytes(key)?;
        self.write(&bytes.repeat(count))?;
        Ok(self)
    }

    /// Types text and waits until it is visible on the terminal.
    ///
    /// # Errors
    ///
    /// Returns an error when input fails or the text does not appear before the deadline.
    pub fn type_text(&mut self, text: &str) -> Result<&mut Self> {
        self.write(text.as_bytes())?;
        self.wait_for_text(text)
    }

    /// Finds one visible control and sends a real terminal mouse click.
    ///
    /// # Errors
    ///
    /// Returns an error when the selector is missing, ambiguous, or input fails.
    pub fn click(&mut self, selector: &Selector) -> Result<&mut Self> {
        let deadline = Instant::now() + TIMEOUT;
        let (column, row) = loop {
            self.pump_available();
            match self.locate(selector)? {
                Some(position) => break position,
                None if Instant::now() < deadline => self.pump_until(deadline)?,
                None => {
                    bail!(
                        "selector {selector:?} was not visible within five seconds\n{}",
                        self.contents()
                    )
                }
            }
        };
        let x = column.saturating_add(1);
        let y = row.saturating_add(1);
        self.write(format!("\x1b[<0;{x};{y}M\x1b[<0;{x};{y}m").as_bytes())?;
        Ok(self)
    }

    /// Sends one real terminal mouse-wheel event over the diff pane.
    ///
    /// # Errors
    ///
    /// Returns an error when the PTY cannot accept input.
    pub fn scroll(&mut self, direction: ScrollDirection) -> Result<&mut Self> {
        self.scroll_many(direction, 1)
    }

    /// Sends several wheel events in one terminal write.
    ///
    /// # Errors
    ///
    /// Returns an error when the PTY cannot accept input.
    pub fn scroll_many(&mut self, direction: ScrollDirection, count: usize) -> Result<&mut Self> {
        let button = match direction {
            ScrollDirection::Up => 64,
            ScrollDirection::Down => 65,
        };
        let event = format!("\x1b[<{button};75;10M");
        self.write(event.repeat(count).as_bytes())?;
        Ok(self)
    }

    /// Drags the visible vertical scrollbar between two percentages.
    ///
    /// # Errors
    ///
    /// Returns an error when a percentage is invalid or the PTY cannot accept input.
    pub fn drag_vertical_scrollbar(
        &mut self,
        from_percent: u16,
        to_percent: u16,
    ) -> Result<&mut Self> {
        if from_percent > 100 || to_percent > 100 {
            bail!("scrollbar percentages must be between 0 and 100");
        }
        let track_start = 2_u16;
        let track_length = ROWS.saturating_sub(4);
        let position =
            |percent: u16| track_start.saturating_add(track_length.saturating_mul(percent) / 100);
        let from = position(from_percent);
        let to = position(to_percent);
        let column = COLUMNS.saturating_sub(1);
        self.write(
            format!("\x1b[<0;{column};{from}M\x1b[<32;{column};{to}M\x1b[<0;{column};{to}m")
                .as_bytes(),
        )?;
        Ok(self)
    }

    /// Drags the visible horizontal scrollbar between two percentages.
    ///
    /// This targets Diffo's default 25% file-pane layout.
    ///
    /// # Errors
    ///
    /// Returns an error when a percentage is invalid or the PTY cannot accept input.
    pub fn drag_horizontal_scrollbar(
        &mut self,
        from_percent: u16,
        to_percent: u16,
    ) -> Result<&mut Self> {
        if from_percent > 100 || to_percent > 100 {
            bail!("scrollbar percentages must be between 0 and 100");
        }
        let track_start = COLUMNS / 4 + 2;
        let track_end = COLUMNS.saturating_sub(1);
        let track_length = track_end.saturating_sub(track_start);
        let position =
            |percent: u16| track_start.saturating_add(track_length.saturating_mul(percent) / 100);
        let from = position(from_percent);
        let to = position(to_percent);
        let row = ROWS.saturating_sub(2);
        self.write(
            format!("\x1b[<0;{from};{row}M\x1b[<32;{to};{row}M\x1b[<0;{to};{row}m").as_bytes(),
        )?;
        Ok(self)
    }

    /// Waits until text is visible on the terminal.
    ///
    /// # Errors
    ///
    /// Returns an error when the process exits or the deadline expires.
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
                    self.contents()
                );
            }
            self.pump_until(deadline)?;
        }
    }

    /// Waits until one semantic selector is visible on the terminal.
    ///
    /// # Errors
    ///
    /// Returns an error when the selector is ambiguous, the process exits, or time expires.
    pub fn wait_for(&mut self, selector: &Selector) -> Result<&mut Self> {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            self.pump_available();
            if self.locate(selector)?.is_some() {
                return Ok(self);
            }
            if Instant::now() >= deadline {
                bail!(
                    "selector {selector:?} was not visible within five seconds\n{}",
                    self.contents()
                );
            }
            self.pump_until(deadline)?;
        }
    }

    /// Returns the current terminal cell for one visible selector.
    ///
    /// # Errors
    ///
    /// Returns an error when the selector is ambiguous.
    pub fn position(&mut self, selector: &Selector) -> Result<Option<(u16, u16)>> {
        self.pump_available();
        self.locate(selector)
    }

    /// Waits until text is no longer visible on the terminal.
    ///
    /// # Errors
    ///
    /// Returns an error when the process exits or the deadline expires.
    pub fn wait_for_text_gone(&mut self, text: &str) -> Result<&mut Self> {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            self.pump_available();
            if find_text(&self.cells(), text).is_empty() {
                return Ok(self);
            }
            if Instant::now() >= deadline {
                bail!(
                    "text {text:?} remained visible for five seconds\n{}",
                    self.contents()
                );
            }
            self.pump_until(deadline)?;
        }
    }

    /// Waits until a later terminal frame differs from the provided contents.
    ///
    /// # Errors
    ///
    /// Returns an error when the process exits or no new frame appears before the deadline.
    pub fn wait_for_change(&mut self, previous: &str) -> Result<&mut Self> {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            self.pump_available();
            if self.contents() != previous {
                return Ok(self);
            }
            if Instant::now() >= deadline {
                bail!(
                    "terminal did not change within five seconds\n{}",
                    self.contents()
                );
            }
            self.pump_until(deadline)?;
        }
    }

    /// Waits for Diffo to exit successfully.
    ///
    /// # Errors
    ///
    /// Returns an error when the process fails or remains alive past the deadline.
    pub fn wait_for_exit(&mut self) -> Result<&mut Self> {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Some(status) = self.child.try_wait().context("poll Diffo process")? {
                if !status.success() {
                    bail!("Diffo exited unsuccessfully: {status:?}");
                }
                return Ok(self);
            }
            if Instant::now() >= deadline {
                bail!(
                    "Diffo did not exit within five seconds\n{}",
                    self.contents()
                );
            }
            self.pump_available();
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[must_use]
    pub fn contents(&self) -> String {
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
            Selector::SelectedRow(text) => cells
                .iter()
                .enumerate()
                .filter(|(_, row)| !find_in_row(row, "›").is_empty())
                .flat_map(|(row, cells)| positions(row, find_in_row(cells, text), text))
                .collect(),
        };
        match matches.as_slice() {
            [] => Ok(None),
            [position] => Ok(Some(*position)),
            _ => bail!(
                "selector {selector:?} matched {} visible controls\n{}",
                matches.len(),
                self.contents()
            ),
        }
    }

    fn cells(&self) -> Vec<Vec<String>> {
        (0..ROWS)
            .map(|row| {
                (0..COLUMNS)
                    .map(|column| {
                        self.parser.screen().cell(row, column).map_or_else(
                            || " ".to_owned(),
                            |cell| {
                                let contents = cell.contents();
                                if contents.is_empty() {
                                    " ".to_owned()
                                } else {
                                    contents
                                }
                            },
                        )
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

impl Drop for DiffoScreen {
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
                let deadline = Instant::now() + Duration::from_millis(250);
                while Instant::now() < deadline {
                    if self.child.try_wait().ok().flatten().is_some() {
                        break;
                    }
                    thread::sleep(Duration::from_millis(10));
                }
            }
        }
        self.writer.take();
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
