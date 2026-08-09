use std::{
    env,
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
    sync::mpsc::{SyncSender, TrySendError, sync_channel},
    thread::{self, JoinHandle},
    time::Instant,
};

use diffo_app::FramePreparation;
use diffo_app::Model;
use serde::Serialize;

pub fn input_events(events: &[crossterm::event::Event], redact: bool) -> Vec<String> {
    if redact {
        return events
            .iter()
            .map(|_| "GitPrompt([redacted])".to_owned())
            .collect();
    }
    events.iter().map(|event| format!("{event:?}")).collect()
}

pub struct FrameTracer {
    started: Instant,
    next_frame: u64,
    sender: Option<SyncSender<FrameRecord>>,
    writer: Option<JoinHandle<()>>,
}

#[derive(Debug, Serialize)]
pub struct FrameRecord {
    frame: u64,
    presentation: Presentation,
    input_events: Vec<String>,
    protected_push_prompt: bool,
    visible_modal: Option<&'static str>,
    refresh_generation: u64,
    head: String,
    repository_operation: diffo_core::RepositoryOperationState,
    repository_files: Vec<String>,
    selected_file: Option<String>,
    requested_diff: Option<String>,
    displayed_diff: Option<String>,
    requested_explorer_file: Option<String>,
    displayed_explorer_file: Option<String>,
    content_revision: u64,
    preparing: bool,
    syntax_ready: bool,
    viewport_transition: Option<(usize, usize)>,
    scroll_before: (usize, usize),
    scroll_after: (usize, usize),
    first_rendered_row: usize,
    event_read_us: Option<u64>,
    update_start_us: u64,
    draw_start_us: u64,
    draw_end_us: u64,
    text_surface: Option<TextSurfaceRecord>,
}

#[derive(Clone, Copy, Debug, Serialize)]
enum Presentation {
    Presented,
    Suppressed,
}

#[derive(Debug, Serialize)]
struct TextSurfaceRecord {
    surface: String,
    document_revision: u64,
    viewport: (usize, usize),
    requested_range: (usize, usize),
    render_mode: String,
    coverage_before: Option<(u32, u32)>,
    coverage_after: Option<(u32, u32)>,
    request_id: Option<u64>,
    queue_wait_us: u64,
    worker_us: u64,
    install_us: u64,
    parsed_lines: usize,
    parsed_bytes: usize,
    projected_lines: usize,
    projected_bytes: usize,
    highlighted_lines: usize,
    highlighted_bytes: usize,
    rendered_lines: usize,
    rendered_bytes: usize,
    cache_hit: bool,
    coalesced_request: bool,
    stale_discarded: bool,
}

impl FrameRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        input_events: Vec<String>,
        protected_push_prompt: bool,
        visible_modal: Option<&'static str>,
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
            presentation: Presentation::Presented,
            input_events,
            protected_push_prompt,
            visible_modal,
            refresh_generation,
            head: match &model.snapshot.head {
                diffo_core::HeadState::Named { name, commit } => {
                    format!("named:{name}:{commit}")
                }
                diffo_core::HeadState::Unborn { name } => format!("unborn:{name}"),
                diffo_core::HeadState::Detached { commit } => format!("detached:{commit}"),
            },
            repository_operation: model.snapshot.operation,
            repository_files: model
                .snapshot
                .files
                .iter()
                .map(|file| {
                    format!(
                        "{}:staged={}:unstaged={}",
                        file.path.display(),
                        file.staged.is_some(),
                        file.unstaged.is_some()
                    )
                })
                .collect(),
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
            requested_explorer_file: preparation
                .requested_explorer_file
                .as_ref()
                .map(|path| path.display().to_string()),
            displayed_explorer_file: preparation
                .displayed_explorer_file
                .as_ref()
                .map(|path| path.display().to_string()),
            content_revision: preparation.content_revision,
            preparing: preparation.preparing,
            syntax_ready: preparation.syntax_ready,
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
            text_surface: preparation
                .text_surface
                .as_ref()
                .map(|surface| TextSurfaceRecord {
                    surface: format!("{:?}", surface.surface),
                    document_revision: surface.document_revision,
                    viewport: surface.viewport,
                    requested_range: surface.requested_range,
                    render_mode: format!("{:?}", surface.mode),
                    coverage_before: surface.coverage_before,
                    coverage_after: surface.coverage_after,
                    request_id: surface.request_id,
                    queue_wait_us: 0,
                    worker_us: 0,
                    install_us: 0,
                    parsed_lines: 0,
                    parsed_bytes: 0,
                    projected_lines: 0,
                    projected_bytes: 0,
                    highlighted_lines: 0,
                    highlighted_bytes: 0,
                    rendered_lines: surface
                        .requested_range
                        .1
                        .saturating_sub(surface.requested_range.0),
                    rendered_bytes: 0,
                    cache_hit: surface.cache_hit,
                    coalesced_request: surface.coalesced_request,
                    stale_discarded: surface.stale_discarded,
                }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn suppressed(
        input_events: Vec<String>,
        protected_push_prompt: bool,
        visible_modal: Option<&'static str>,
        refresh_generation: u64,
        model: &Model,
        preparation: &FramePreparation,
        scroll_before: (usize, usize),
        update_start_us: u64,
        event_read_us: Option<u64>,
        timestamp_us: u64,
    ) -> Self {
        let mut record = Self::new(
            input_events,
            protected_push_prompt,
            visible_modal,
            refresh_generation,
            model,
            preparation,
            scroll_before,
            update_start_us,
            event_read_us,
            timestamp_us,
            timestamp_us,
        );
        record.presentation = Presentation::Suppressed;
        record
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
        if matches!(record.presentation, Presentation::Presented) {
            self.next_frame = self.next_frame.saturating_add(1);
        }
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
