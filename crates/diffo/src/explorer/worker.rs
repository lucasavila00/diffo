use std::{
    collections::HashMap,
    env,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{Receiver, Sender, channel},
    },
    thread,
    time::Duration,
};

use diffo_core::{ChangeKind, ExplorerFile, ExplorerFileContent, Repository};
use diffo_diff::{DiffBlock, DiffDocument, DiffLine, Hunk, parse_unified_patch};
use diffo_highlight::{HighlightWindowRequest, LineRange, SyntaxHighlighter};
use diffo_tui::{
    HIGHLIGHT_LOOKBEHIND_LINES, MAX_HIGHLIGHT_BYTES_PER_SIDE, MAX_HIGHLIGHT_FILE_LINES,
};

use super::model::{GutterMarker, Viewer};

#[derive(Clone, Debug)]
pub(crate) enum ExplorerRequest {
    Paths {
        id: u64,
    },
    File {
        id: u64,
        path: PathBuf,
        status: Option<ChangeKind>,
        first_line: usize,
        viewport_rows: usize,
    },
}

impl ExplorerRequest {
    fn id(&self) -> u64 {
        match self {
            Self::Paths { id } | Self::File { id, .. } => *id,
        }
    }
}

pub(crate) enum ExplorerOutcome {
    Paths {
        id: u64,
        result: Result<Vec<PathBuf>, String>,
    },
    File {
        id: u64,
        result: Result<Viewer, String>,
    },
}

pub(crate) struct ExplorerWorker {
    requests: Sender<ExplorerRequest>,
    outcomes: Receiver<ExplorerOutcome>,
    latest_file: Arc<AtomicU64>,
}

impl ExplorerWorker {
    pub(crate) fn start(repository: Arc<dyn Repository>) -> Self {
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
                        status,
                        first_line,
                        viewport_rows,
                    } => {
                        thread::sleep(preparation_delay_from_environment());
                        ExplorerOutcome::File {
                            id,
                            result: repository
                                .explorer_file(&path)
                                .map(|file| {
                                    prepare_viewer(
                                        path,
                                        status,
                                        file,
                                        first_line,
                                        viewport_rows,
                                        &highlighter,
                                    )
                                })
                                .map_err(|error| error.to_string()),
                        }
                    }
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

    pub(crate) fn submit(&self, request: ExplorerRequest) {
        if let ExplorerRequest::File { id, .. } = &request {
            self.latest_file.store(*id, Ordering::Release);
        }
        let _ = self.requests.send(request);
    }

    pub(crate) fn try_recv(&self) -> Option<ExplorerOutcome> {
        self.outcomes.try_recv().ok()
    }
}

fn preparation_delay_from_environment() -> Duration {
    env::var("DIFFO_E2E_EXPLORER_PREP_DELAY_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(|milliseconds| Duration::from_millis(milliseconds.min(5_000)))
        .unwrap_or_default()
}

fn prepare_viewer(
    path: PathBuf,
    status: Option<ChangeKind>,
    file: ExplorerFile,
    first_line: usize,
    viewport_rows: usize,
    highlighter: &SyntaxHighlighter,
) -> Viewer {
    let ExplorerFileContent::Text(text) = file.content else {
        return Viewer {
            path,
            lines: Vec::new(),
            markers: HashMap::new(),
            highlighted: HashMap::new(),
            coverage: None,
            syntax_eligible: false,
            message: Some("Binary or non-UTF-8 file.".to_owned()),
            maximum_width: 0,
        };
    };
    let lines = text.lines().map(str::to_owned).collect::<Vec<_>>();
    let maximum_width = lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);
    let markers = change_markers(&file.patch, file.deleted, status, &lines);
    let syntax_eligible = lines.len() < MAX_HIGHLIGHT_FILE_LINES;
    let range = visible_range(first_line, viewport_rows, lines.len());
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
        lines,
        markers,
        highlighted: styles,
        coverage: range,
        syntax_eligible,
        message: None,
        maximum_width,
    }
}

fn visible_range(first_line: usize, viewport_rows: usize, line_count: usize) -> Option<LineRange> {
    if line_count == 0 {
        return None;
    }
    let start = first_line.saturating_add(1).min(line_count);
    let end = first_line
        .saturating_add(viewport_rows.max(1).saturating_mul(3))
        .min(line_count)
        .max(start);
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
    deleted: bool,
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
                    removed, added: _, ..
                } if deleted => {
                    for line in removed {
                        if let Some(number) = line.old_number {
                            markers.insert(number as usize, GutterMarker::Deleted);
                        }
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
        let markers = change_markers(patch, false, Some(ChangeKind::Modified), &lines);
        assert_eq!(markers.get(&2), Some(&GutterMarker::Modified));
        assert_eq!(markers.get(&3), Some(&GutterMarker::Deleted));
        assert_eq!(markers.get(&4), Some(&GutterMarker::Added));
    }

    #[test]
    fn deleted_file_uses_old_line_numbers() {
        let markers = change_markers(
            "@@ -1,2 +0,0 @@\n-one\n-two\n",
            true,
            Some(ChangeKind::Deleted),
            &["one".to_owned(), "two".to_owned()],
        );
        assert_eq!(markers.get(&1), Some(&GutterMarker::Deleted));
        assert_eq!(markers.get(&2), Some(&GutterMarker::Deleted));
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
        let markers = change_markers("", false, Some(ChangeKind::Conflicted), &lines);
        assert_eq!(markers.get(&1), Some(&GutterMarker::Conflict));
        assert_eq!(markers.get(&3), Some(&GutterMarker::Conflict));
        assert_eq!(markers.get(&5), Some(&GutterMarker::Conflict));
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
            None,
            ExplorerFile {
                content: ExplorerFileContent::Text(text),
                patch: String::new(),
                deleted: false,
            },
            0,
            20,
            &highlighter,
        );
        assert!(viewer.syntax_eligible);
        assert!(viewer.coverage.is_some_and(|range| range.end >= 20));
        assert!(viewer.highlighted.contains_key(&1));

        let at_limit = prepare_viewer(
            PathBuf::from("source.rs"),
            None,
            ExplorerFile {
                content: ExplorerFileContent::Text("value\n".repeat(10_000)),
                patch: String::new(),
                deleted: false,
            },
            0,
            20,
            &highlighter,
        );
        assert!(!at_limit.syntax_eligible);
        assert!(at_limit.highlighted.is_empty());
    }
}
