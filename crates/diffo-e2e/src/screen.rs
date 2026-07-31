use std::{
    ffi::OsStr,
    io::Write,
    path::Path,
    sync::mpsc::{Receiver, RecvTimeoutError, sync_channel},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use portable_pty::{Child, CommandBuilder, PtySize, native_pty_system};

use crate::{
    Key, ScrollDirection, Selector,
    input::key_bytes,
    reader::read_output,
    selectors::{
        find_dialog_action, find_file_action, find_in_row, find_panel_action, find_text,
        find_toast_action, positions,
    },
};

const ROWS: u16 = 30;
const COLUMNS: u16 = 100;
const ACTIVITY_BAR_WIDTH: u16 = 5;
const TIMEOUT: Duration = Duration::from_secs(10);
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const SELECTION_BACKGROUND: vt100::Color = vt100::Color::Idx(8);

pub struct DiffoScreen {
    parser: vt100::Parser,
    output: Receiver<Vec<u8>>,
    raw_output: Vec<u8>,
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
        command.env_remove("NO_COLOR");
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
            raw_output: Vec::new(),
            writer: Some(writer),
            child,
        };
        screen.wait_for_text_until("[ Commands (1 / F1) ]", STARTUP_TIMEOUT)?;
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
        let (column, row) = self.wait_for_position(selector)?;
        self.click_at(column, row)
    }

    /// Sends one real terminal mouse click at a zero-based cell position.
    ///
    /// # Errors
    ///
    /// Returns an error when the position is outside the test terminal or input fails.
    pub fn click_at(&mut self, column: u16, row: u16) -> Result<&mut Self> {
        if column >= COLUMNS || row >= ROWS {
            bail!("click position ({column}, {row}) is outside the test terminal");
        }
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
        self.write_wheel(direction, count, 74, 9)?;
        Ok(self)
    }

    /// Sends wheel events over one visible control.
    ///
    /// # Errors
    ///
    /// Returns an error when the selector is missing, ambiguous, or input fails.
    pub fn scroll_many_at(
        &mut self,
        selector: &Selector,
        direction: ScrollDirection,
        count: usize,
    ) -> Result<&mut Self> {
        let (column, row) = self.wait_for_position(selector)?;
        self.write_wheel(direction, count, column, row)?;
        Ok(self)
    }

    fn write_wheel(
        &mut self,
        direction: ScrollDirection,
        count: usize,
        column: u16,
        row: u16,
    ) -> Result<()> {
        let button = match direction {
            ScrollDirection::Up => 64,
            ScrollDirection::Down => 65,
            ScrollDirection::Left => 66,
            ScrollDirection::Right => 67,
        };
        let x = column.saturating_add(1);
        let y = row.saturating_add(1);
        let event = format!("\x1b[<{button};{x};{y}M");
        self.write(event.repeat(count).as_bytes())
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
        let track_start = 3_u16;
        let track_length = ROWS.saturating_sub(6);
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
        let content_width = COLUMNS.saturating_sub(ACTIVITY_BAR_WIDTH);
        let track_start = ACTIVITY_BAR_WIDTH + content_width / 4 + 3;
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
        self.wait_for_text_until(text, TIMEOUT)
    }

    fn wait_for_text_until(&mut self, text: &str, timeout: Duration) -> Result<&mut Self> {
        let deadline = Instant::now() + timeout;
        loop {
            self.pump_available();
            if !find_text(&self.cells(), text).is_empty() {
                return Ok(self);
            }
            if Instant::now() >= deadline {
                bail!(
                    "text {text:?} was not visible within {} seconds\n{}",
                    timeout.as_secs(),
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
                    "selector {selector:?} was not visible within ten seconds\n{}",
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
                    "text {text:?} remained visible for ten seconds\n{}",
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
                    "terminal did not change within ten seconds\n{}",
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
                bail!("Diffo did not exit within ten seconds\n{}", self.contents());
            }
            self.pump_available();
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[must_use]
    pub fn contents(&self) -> String {
        self.parser.screen().contents()
    }

    /// Returns all bytes emitted by Diffo since launch.
    #[must_use]
    pub fn raw_output(&mut self) -> &[u8] {
        self.pump_available();
        &self.raw_output
    }

    /// Waits until Diffo has emitted no terminal bytes for the requested interval.
    ///
    /// # Errors
    ///
    /// Returns an error when the process exits or output never becomes quiet.
    pub fn wait_for_quiet(&mut self, interval: Duration) -> Result<&mut Self> {
        let deadline = Instant::now() + TIMEOUT;
        let mut quiet_until = Instant::now() + interval;
        loop {
            let now = Instant::now();
            if now >= quiet_until {
                return Ok(self);
            }
            if now >= deadline {
                bail!("Diffo terminal output did not become quiet within ten seconds");
            }
            let wait = quiet_until
                .saturating_duration_since(now)
                .min(deadline.saturating_duration_since(now));
            match self.output.recv_timeout(wait) {
                Ok(bytes) => {
                    self.process_output(&bytes);
                    quiet_until = Instant::now() + interval;
                }
                Err(RecvTimeoutError::Timeout) => return Ok(self),
                Err(RecvTimeoutError::Disconnected) => {
                    bail!("Diffo PTY output stopped before becoming quiet")
                }
            }
        }
    }

    /// Proves that Diffo emits no terminal bytes for the requested interval.
    ///
    /// # Errors
    ///
    /// Returns an error when output is received or the process exits.
    pub fn expect_quiet(&mut self, interval: Duration) -> Result<&mut Self> {
        self.pump_available();
        match self.output.recv_timeout(interval) {
            Err(RecvTimeoutError::Timeout) => Ok(self),
            Ok(bytes) => {
                self.process_output(&bytes);
                bail!(
                    "Diffo emitted {} bytes during a quiet interval",
                    bytes.len()
                )
            }
            Err(RecvTimeoutError::Disconnected) => bail!("Diffo PTY output stopped"),
        }
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
                .flat_map(|(row, cells)| positions(row, find_in_row(cells, text), text))
                .filter(|(column, row)| {
                    self.parser
                        .screen()
                        .cell(*row, *column)
                        .is_some_and(|cell| cell.bgcolor() == SELECTION_BACKGROUND)
                })
                .collect(),
            Selector::DialogAction { dialog, action } => find_dialog_action(&cells, dialog, action),
            Selector::ToastAction { toast, action } => find_toast_action(&cells, toast, action),
            Selector::VerticalScrollbarEnd => {
                let row = ROWS.saturating_sub(4);
                let column = COLUMNS.saturating_sub(2);
                cells
                    .get(usize::from(row))
                    .and_then(|cells| cells.get(usize::from(column)))
                    .is_some_and(|cell| cell == "█")
                    .then_some((column, row))
                    .into_iter()
                    .collect()
            }
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

    fn wait_for_position(&mut self, selector: &Selector) -> Result<(u16, u16)> {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            self.pump_available();
            match self.locate(selector)? {
                Some(position) => return Ok(position),
                None if Instant::now() < deadline => self.pump_until(deadline)?,
                None => {
                    bail!(
                        "selector {selector:?} was not visible within ten seconds\n{}",
                        self.contents()
                    )
                }
            }
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
            self.process_output(&bytes);
        }
    }

    fn pump_until(&mut self, deadline: Instant) -> Result<()> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match self
            .output
            .recv_timeout(remaining.min(Duration::from_millis(50)))
        {
            Ok(bytes) => self.process_output(&bytes),
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

    fn process_output(&mut self, bytes: &[u8]) {
        self.raw_output.extend_from_slice(bytes);
        self.parser.process(bytes);
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
