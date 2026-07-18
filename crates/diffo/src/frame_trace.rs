use std::{
    env,
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
    sync::mpsc::{SyncSender, TrySendError, sync_channel},
    thread::{self, JoinHandle},
    time::Instant,
};

use diffo_app::Model;
use diffo_tui::FramePreparation;
use serde::Serialize;

pub struct FrameTracer {
    started: Instant,
    next_frame: u64,
    sender: Option<SyncSender<FrameRecord>>,
    writer: Option<JoinHandle<()>>,
}

#[derive(Debug, Serialize)]
pub struct FrameRecord {
    frame: u64,
    input_events: Vec<String>,
    refresh_generation: u64,
    selected_file: Option<String>,
    requested_diff: Option<String>,
    displayed_diff: Option<String>,
    content_revision: u64,
    preparing: bool,
    viewport_transition: Option<(usize, usize)>,
    scroll_before: (usize, usize),
    scroll_after: (usize, usize),
    first_rendered_row: usize,
    event_read_us: Option<u64>,
    update_start_us: u64,
    draw_start_us: u64,
    draw_end_us: u64,
}

impl FrameRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        input_events: Vec<String>,
        refresh_generation: u64,
        model: &Model,
        preparation: &FramePreparation,
        scroll_before: (usize, usize),
        update_start_us: u64,
        event_read_us: Option<u64>,
        draw_start_us: u64,
        draw_end_us: u64,
    ) -> Self {
        Self {
            frame: 0,
            input_events,
            refresh_generation,
            selected_file: model
                .selected
                .as_ref()
                .map(|selected| format!("{:?}:{}", selected.area, selected.path.display())),
            requested_diff: preparation
                .requested_file
                .as_ref()
                .map(|file| format!("{:?}:{}", file.area, file.path.display())),
            displayed_diff: preparation
                .displayed_file
                .as_ref()
                .map(|file| format!("{:?}:{}", file.area, file.path.display())),
            content_revision: preparation.content_revision,
            preparing: preparation.preparing,
            viewport_transition: preparation
                .viewport_transition
                .map(|viewport| (viewport.vertical, viewport.horizontal)),
            scroll_before,
            scroll_after: (model.diff_scroll, model.diff_horizontal_scroll),
            first_rendered_row: model.diff_scroll,
            event_read_us,
            update_start_us,
            draw_start_us,
            draw_end_us,
        }
    }
}

impl FrameTracer {
    pub fn from_environment() -> Self {
        let started = Instant::now();
        let Some(path) = env::var_os("DIFFO_TRACE_FRAMES").map(PathBuf::from) else {
            return Self {
                started,
                next_frame: 0,
                sender: None,
                writer: None,
            };
        };
        let (sender, records) = sync_channel::<FrameRecord>(256);
        let writer = thread::Builder::new()
            .name("diffo-frame-trace".to_owned())
            .spawn(move || write_records(path, &records))
            .ok();
        Self {
            started,
            next_frame: 0,
            sender: writer.as_ref().map(|_| sender),
            writer,
        }
    }

    pub fn elapsed_us(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_micros()).unwrap_or(u64::MAX)
    }

    pub fn record(&mut self, mut record: FrameRecord) {
        record.frame = self.next_frame;
        self.next_frame = self.next_frame.saturating_add(1);
        if let Some(sender) = self.sender.as_ref() {
            match sender.try_send(record) {
                Ok(()) | Err(TrySendError::Full(_)) => {}
                Err(TrySendError::Disconnected(_)) => self.sender = None,
            }
        }
    }
}

impl Drop for FrameTracer {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
    }
}

fn write_records(path: PathBuf, records: &std::sync::mpsc::Receiver<FrameRecord>) {
    let Ok(file) = File::create(path) else {
        return;
    };
    let mut writer = BufWriter::new(file);
    while let Ok(record) = records.recv() {
        let Ok(serialized) = ron::to_string(&record) else {
            continue;
        };
        if writeln!(writer, "{serialized}").is_err() {
            return;
        }
    }
}
