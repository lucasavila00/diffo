use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{Receiver, Sender, channel},
    },
    thread,
};

use diffo_core::{ChangeKind, ExplorerFile, ExplorerFileContent, Repository};
use diffo_diff::{DiffBlock, DiffDocument, DiffLine, Hunk, parse_unified_patch};
use diffo_highlight::{
    HIGHLIGHT_LOOKBEHIND_LINES, HighlightWindowRequest, LineRange, MAX_HIGHLIGHT_BYTES_PER_SIDE,
    MAX_HIGHLIGHT_FILE_LINES, SyntaxHighlighter,
};
use diffo_ui::terminal_safe_text;
use ratatui::text::Line;

use super::model::{GutterMarker, Viewer};

#[derive(Clone, Debug)]
pub enum ExplorerRequest {
    Paths {
        id: u64,
    },
    File {
        id: u64,
        path: PathBuf,
        title: Line<'static>,
        status: Option<ChangeKind>,
        replace: bool,
        first_line: usize,
        viewport_rows: usize,
        window_viewports: usize,
    },
}

impl ExplorerRequest {
    fn id(&self) -> u64 {
        match self {
            Self::Paths { id } | Self::File { id, .. } => *id,
        }
    }
}

pub enum ExplorerOutcome {
    Paths {
        id: u64,
        result: Result<Vec<PathBuf>, String>,
    },
    File {
        id: u64,
        replace: bool,
        result: Result<Viewer, String>,
    },
}

pub struct ExplorerWorker {
    requests: Sender<ExplorerRequest>,
    outcomes: Receiver<ExplorerOutcome>,
    latest_file: Arc<AtomicU64>,
}

#[derive(Clone, Copy)]
struct SyntaxWindow {
    first_line: usize,
    viewport_rows: usize,
    window_viewports: usize,
}

impl ExplorerWorker {
    pub fn start(repository: Arc<dyn Repository>) -> Self {
        let (request_tx, request_rx) = channel::<ExplorerRequest>();
        let (outcome_tx, outcome_rx) = channel();
        let latest_file = Arc::new(AtomicU64::new(0));
        let worker_latest = Arc::clone(&latest_file);
        thread::spawn(move || {
            let highlighter = SyntaxHighlighter::new();
            while let Ok(request) = request_rx.recv() {
                if matches!(request, ExplorerRequest::File { .. })
                    && request.id() != worker_latest.load(Ordering::Acquire)
                {
                    continue;
                }
                let outcome = match request {
                    ExplorerRequest::Paths { id } => ExplorerOutcome::Paths {
                        id,
                        result: repository
                            .explorer_paths()
                            .map_err(|error| error.to_string()),
                    },
                    ExplorerRequest::File {
                        id,
                        path,
                        title,
                        status,
                        replace,
                        first_line,
                        viewport_rows,
                        window_viewports,
                    } => ExplorerOutcome::File {
                        id,
                        replace,
                        result: repository
                            .explorer_file(&path)
                            .map(|file| {
                                prepare_viewer(
                                    path,
                                    title,
                                    status,
                                    file,
                                    SyntaxWindow {
                                        first_line,
                                        viewport_rows,
                                        window_viewports,
                                    },
                                    &highlighter,
                                )
                            })
                            .map_err(|error| error.to_string()),
                    },
                };
                let stale = matches!(outcome, ExplorerOutcome::File { id, .. } if id != worker_latest.load(Ordering::Acquire));
                if !stale && outcome_tx.send(outcome).is_err() {
                    break;
                }
            }
        });
        Self {
            requests: request_tx,
            outcomes: outcome_rx,
            latest_file,
        }
    }

    pub fn submit(&self, request: ExplorerRequest) {
        if let ExplorerRequest::File { id, .. } = &request {
            self.latest_file.store(*id, Ordering::Release);
        }
        let _ = self.requests.send(request);
    }

    #[must_use]
    pub fn try_recv(&self) -> Option<ExplorerOutcome> {
        self.outcomes.try_recv().ok()
    }
}

fn prepare_viewer(
    path: PathBuf,
    title: Line<'static>,
    status: Option<ChangeKind>,
    file: ExplorerFile,
    window: SyntaxWindow,
    highlighter: &SyntaxHighlighter,
) -> Viewer {
    let ExplorerFileContent::Text(text) = file.content else {
        return Viewer {
            path,
            title: Box::new(title),
            lines: Vec::new(),
            markers: HashMap::new(),
            highlighted: HashMap::new(),
            coverage: Vec::new(),
            syntax_eligible: false,
            message: Some("Binary or non-UTF-8 file.".to_owned()),
        };
    };
    let lines = text.lines().map(terminal_safe_text).collect::<Vec<_>>();
    let markers = change_markers(&file.patch, status, &lines);
    let syntax_eligible = lines.len() < MAX_HIGHLIGHT_FILE_LINES;
    let range = visible_range(
        window.first_line,
        window.viewport_rows,
        window.window_viewports,
        lines.len(),
    );
    let styles = if syntax_eligible {
        range.map_or_else(HashMap::new, |range| {
            let document = file_document(&lines, range);
            highlighter
                .highlight_window(
                    &path,
                    &document,
                    HighlightWindowRequest {
                        old: None,
                        new: Some(range),
                        lookbehind_lines: HIGHLIGHT_LOOKBEHIND_LINES,
                        maximum_bytes_per_side: MAX_HIGHLIGHT_BYTES_PER_SIDE,
                    },
                )
                .styles
                .new
                .into_iter()
                .collect()
        })
    } else {
        HashMap::new()
    };
    Viewer {
        path,
        title: Box::new(title),
        lines,
        markers,
        highlighted: styles,
        coverage: range.into_iter().collect(),
        syntax_eligible,
        message: None,
    }
}

fn visible_range(
    first_line: usize,
    viewport_rows: usize,
    window_viewports: usize,
    line_count: usize,
) -> Option<LineRange> {
    if line_count == 0 {
        return None;
    }
    let window = diffo_ui::text_view::centered_window(
        first_line,
        line_count,
        viewport_rows,
        window_viewports,
    );
    let start = window.start.saturating_add(1);
    let end = window.end.max(start);
    Some(LineRange::new(
        u32::try_from(start).unwrap_or(u32::MAX),
        u32::try_from(end).unwrap_or(u32::MAX),
    ))
}

fn file_document(lines: &[String], range: LineRange) -> DiffDocument {
    let first = usize::try_from(range.start)
        .unwrap_or(usize::MAX)
        .saturating_sub(HIGHLIGHT_LOOKBEHIND_LINES)
        .max(1);
    let end = usize::try_from(range.end)
        .unwrap_or(usize::MAX)
        .min(lines.len());
    DiffDocument {
        hunks: vec![Hunk {
            header: String::new(),
            old_start: 1,
            new_start: 1,
            blocks: vec![DiffBlock::Context(
                lines
                    .iter()
                    .enumerate()
                    .skip(first.saturating_sub(1))
                    .take(end.saturating_sub(first).saturating_add(1))
                    .map(|(index, text)| {
                        let number = u32::try_from(index.saturating_add(1)).unwrap_or(u32::MAX);
                        DiffLine {
                            old_number: Some(number),
                            new_number: Some(number),
                            text: text.clone(),
                        }
                    })
                    .collect(),
            )],
        }],
        binary: false,
    }
}

fn change_markers(
    patch: &str,
    status: Option<ChangeKind>,
    lines: &[String],
) -> HashMap<usize, GutterMarker> {
    let Ok(document) = parse_unified_patch(patch) else {
        return HashMap::new();
    };
    let mut markers = HashMap::new();
    for hunk in document.hunks {
        let mut new_cursor = hunk.new_start.max(1);
        for block in hunk.blocks {
            match block {
                DiffBlock::Context(context) => {
                    if let Some(number) = context.last().and_then(|line| line.new_number) {
                        new_cursor = number.saturating_add(1);
                    }
                }
                DiffBlock::Change {
                    removed,
                    added,
                    alignment,
                } => {
                    if added.is_empty() && !removed.is_empty() {
                        let point = usize::try_from(new_cursor)
                            .unwrap_or(usize::MAX)
                            .clamp(1, lines.len().max(1));
                        markers.insert(point, GutterMarker::Deleted);
                    } else if removed.is_empty() {
                        for line in added {
                            if let Some(number) = line.new_number {
                                markers.insert(number as usize, GutterMarker::Added);
                                new_cursor = number.saturating_add(1);
                            }
                        }
                    } else {
                        let mut deleted_at_point = false;
                        for pair in alignment {
                            match (pair.old, pair.new) {
                                (Some(_), Some(line)) => {
                                    if let Some(number) = line.new_number {
                                        markers.insert(number as usize, GutterMarker::Modified);
                                        new_cursor = number.saturating_add(1);
                                    }
                                }
                                (None, Some(line)) => {
                                    if let Some(number) = line.new_number {
                                        markers.insert(number as usize, GutterMarker::Added);
                                        new_cursor = number.saturating_add(1);
                                    }
                                }
                                (Some(_), None) => deleted_at_point = true,
                                (None, None) => {}
                            }
                        }
                        if deleted_at_point {
                            let point = usize::try_from(new_cursor)
                                .unwrap_or(usize::MAX)
                                .clamp(1, lines.len().max(1));
                            markers.insert(point, GutterMarker::Deleted);
                        }
                    }
                }
                DiffBlock::Meta(_) => {}
            }
        }
    }
    if status == Some(ChangeKind::Conflicted) {
        for (index, line) in lines.iter().enumerate() {
            if ["<<<<<<<", "|||||||", "=======", ">>>>>>>"]
                .iter()
                .any(|prefix| line.starts_with(prefix))
            {
                markers.insert(index.saturating_add(1), GutterMarker::Conflict);
            }
        }
    }
    markers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_added_modified_and_deleted_lines_to_file_numbers() {
        let patch = "@@ -1,4 +1,4 @@\n same\n-old\n+new\n-gone\n tail\n+added\n";
        let lines = ["same", "new", "tail", "added"].map(str::to_owned);
        let markers = change_markers(patch, Some(ChangeKind::Modified), &lines);
        let mut markers = markers.into_iter().collect::<Vec<_>>();
        markers.sort_by_key(|(line, _)| *line);
        insta::assert_debug_snapshot!(markers);
    }

    #[test]
    fn conflict_lines_use_the_conflict_marker() {
        let lines = [
            "<<<<<<< ours",
            "value",
            "=======",
            "other",
            ">>>>>>> theirs",
        ]
        .map(str::to_owned);
        let markers = change_markers("", Some(ChangeKind::Conflicted), &lines);
        let mut markers = markers.into_iter().collect::<Vec<_>>();
        markers.sort_by_key(|(line, _)| *line);
        insta::assert_debug_snapshot!(markers);
    }

    #[test]
    fn open_commits_visible_syntax_and_keeps_strict_line_limit() {
        let highlighter = SyntaxHighlighter::new();
        let text = (0..9_999)
            .map(|index| format!("let value_{index} = {index};"))
            .collect::<Vec<_>>()
            .join("\n");
        let viewer = prepare_viewer(
            PathBuf::from("source.rs"),
            Line::raw("  source.rs"),
            None,
            ExplorerFile {
                content: ExplorerFileContent::Text(text),
                patch: String::new(),
            },
            SyntaxWindow {
                first_line: 0,
                viewport_rows: 20,
                window_viewports: 3,
            },
            &highlighter,
        );
        assert!(viewer.syntax_eligible);
        assert!(viewer.coverage.iter().any(|range| range.end >= 20));
        assert!(viewer.highlighted.contains_key(&1));

        let at_limit = prepare_viewer(
            PathBuf::from("source.rs"),
            Line::raw("  source.rs"),
            None,
            ExplorerFile {
                content: ExplorerFileContent::Text("value\n".repeat(10_000)),
                patch: String::new(),
            },
            SyntaxWindow {
                first_line: 0,
                viewport_rows: 20,
                window_viewports: 3,
            },
            &highlighter,
        );
        assert!(!at_limit.syntax_eligible);
        assert!(at_limit.highlighted.is_empty());
    }

    #[test]
    fn syntax_window_is_centered_and_preserves_its_size_at_both_boundaries() {
        assert_eq!(visible_range(50, 10, 3, 100), Some(LineRange::new(41, 70)));
        assert_eq!(visible_range(0, 10, 3, 100), Some(LineRange::new(1, 30)));
        assert_eq!(visible_range(95, 10, 3, 100), Some(LineRange::new(71, 100)));
    }

    #[test]
    fn viewer_content_cannot_emit_terminal_control_sequences() {
        let viewer = prepare_viewer(
            PathBuf::from("control.txt"),
            Line::raw("  control.txt"),
            None,
            ExplorerFile {
                content: ExplorerFileContent::Text("before\t\x1b[2J\x08after\n".to_owned()),
                patch: String::new(),
            },
            SyntaxWindow {
                first_line: 0,
                viewport_rows: 20,
                window_viewports: 3,
            },
            &SyntaxHighlighter::new(),
        );

        assert!(!viewer.lines[0].chars().any(char::is_control));
        insta::assert_debug_snapshot!(viewer.lines);
    }
}
