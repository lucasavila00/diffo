use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{Receiver, Sender, channel},
    },
    thread,
};

use diffo_core::{ChangeKind, ExplorerFile, ExplorerFileContent, Repository};
use diffo_diff::{DiffBlock, parse_unified_patch};
use diffo_highlight::{
    HIGHLIGHT_LOOKBEHIND_LINES, HighlightedTextWindow, LineRange, MAX_HIGHLIGHT_BYTES_PER_SIDE,
    MAX_HIGHLIGHT_FILE_LINES, SyntaxHighlighter,
};
use diffo_ui::terminal_safe_text;
use diffo_ui::text_view::SyntaxCoverage;
use ratatui::text::Line;

use super::model::{ExplorerDocumentId, GutterMarker, Viewer};

#[derive(Clone, Debug)]
pub enum ExplorerRequest {
    Paths {
        id: u64,
    },
    QuickOpenPaths {
        id: u64,
    },
    LoadFile {
        id: u64,
        path: PathBuf,
        title: Line<'static>,
        status: Option<ChangeKind>,
        first_line: usize,
        viewport_rows: usize,
        window_viewports: usize,
    },
    HighlightWindow {
        id: u64,
        document_id: ExplorerDocumentId,
        path: PathBuf,
        lines: Arc<[String]>,
        first_line: usize,
        viewport_rows: usize,
        window_viewports: usize,
    },
}

pub enum ExplorerOutcome {
    Paths {
        id: u64,
        result: Result<Vec<PathBuf>, String>,
    },
    QuickOpenPaths {
        id: u64,
        result: Result<Vec<PathBuf>, String>,
    },
    FileLoaded {
        id: u64,
        result: Result<Viewer, String>,
    },
    WindowHighlighted {
        id: u64,
        document_id: ExplorerDocumentId,
        result: HighlightedTextWindow,
    },
}

pub struct ExplorerWorker {
    requests: Sender<ExplorerRequest>,
    outcomes: Receiver<ExplorerOutcome>,
    latest_load: Arc<AtomicU64>,
    latest_window: Arc<AtomicU64>,
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
        let latest_load = Arc::new(AtomicU64::new(0));
        let latest_window = Arc::new(AtomicU64::new(0));
        let worker_latest_load = Arc::clone(&latest_load);
        let worker_latest_window = Arc::clone(&latest_window);
        thread::spawn(move || {
            let highlighter = SyntaxHighlighter::new();
            while let Ok(request) = request_rx.recv() {
                match &request {
                    ExplorerRequest::LoadFile { id, .. }
                        if *id != worker_latest_load.load(Ordering::Acquire) =>
                    {
                        continue;
                    }
                    ExplorerRequest::HighlightWindow { id, .. }
                        if *id != worker_latest_window.load(Ordering::Acquire) =>
                    {
                        continue;
                    }
                    ExplorerRequest::Paths { .. }
                    | ExplorerRequest::QuickOpenPaths { .. }
                    | ExplorerRequest::LoadFile { .. }
                    | ExplorerRequest::HighlightWindow { .. } => {}
                }
                let outcome = match request {
                    ExplorerRequest::Paths { id } => ExplorerOutcome::Paths {
                        id,
                        result: repository
                            .explorer_paths()
                            .map_err(|error| error.to_string()),
                    },
                    ExplorerRequest::QuickOpenPaths { id } => ExplorerOutcome::QuickOpenPaths {
                        id,
                        result: repository
                            .quick_open_paths()
                            .map_err(|error| error.to_string()),
                    },
                    ExplorerRequest::LoadFile {
                        id,
                        path,
                        title,
                        status,
                        first_line,
                        viewport_rows,
                        window_viewports,
                    } => ExplorerOutcome::FileLoaded {
                        id,
                        result: repository
                            .explorer_file(&path)
                            .map(|file| {
                                prepare_viewer(
                                    ExplorerDocumentId(id),
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
                    ExplorerRequest::HighlightWindow {
                        id,
                        document_id,
                        path,
                        lines,
                        first_line,
                        viewport_rows,
                        window_viewports,
                    } => ExplorerOutcome::WindowHighlighted {
                        id,
                        document_id,
                        result: prepare_syntax_window(
                            &path,
                            &lines,
                            SyntaxWindow {
                                first_line,
                                viewport_rows,
                                window_viewports,
                            },
                            &highlighter,
                        ),
                    },
                };
                let stale = matches!(
                    outcome,
                    ExplorerOutcome::FileLoaded { id, .. }
                        if id != worker_latest_load.load(Ordering::Acquire)
                );
                if !stale && outcome_tx.send(outcome).is_err() {
                    break;
                }
            }
        });
        Self {
            requests: request_tx,
            outcomes: outcome_rx,
            latest_load,
            latest_window,
        }
    }

    pub fn submit(&self, request: ExplorerRequest) {
        match &request {
            ExplorerRequest::LoadFile { id, .. } => {
                self.latest_load.store(*id, Ordering::Release);
                self.latest_window.store(0, Ordering::Release);
            }
            ExplorerRequest::HighlightWindow { id, .. } => {
                self.latest_window.store(*id, Ordering::Release);
            }
            ExplorerRequest::Paths { .. } | ExplorerRequest::QuickOpenPaths { .. } => {}
        }
        let _ = self.requests.send(request);
    }

    #[must_use]
    pub fn try_recv(&self) -> Option<ExplorerOutcome> {
        self.outcomes.try_recv().ok()
    }
}

fn prepare_viewer(
    document_id: ExplorerDocumentId,
    path: PathBuf,
    title: Line<'static>,
    status: Option<ChangeKind>,
    file: ExplorerFile,
    window: SyntaxWindow,
    highlighter: &SyntaxHighlighter,
) -> Viewer {
    let ExplorerFileContent::Text(text) = file.content else {
        return Viewer {
            document_id,
            path,
            title: Box::new(title),
            lines: Arc::from([]),
            markers: HashMap::new(),
            highlighted: BTreeMap::new(),
            coverage: SyntaxCoverage::default(),
            syntax_eligible: false,
            message: Some("Binary or non-UTF-8 file.".to_owned()),
        };
    };
    let lines: Arc<[String]> = text.lines().map(terminal_safe_text).collect();
    let markers = change_markers(&file.patch, status, &lines);
    let syntax_eligible = lines.len() < MAX_HIGHLIGHT_FILE_LINES;
    let syntax_window = if syntax_eligible {
        prepare_syntax_window(&path, &lines, window, highlighter)
    } else {
        HighlightedTextWindow::default()
    };
    Viewer {
        document_id,
        path,
        title: Box::new(title),
        lines,
        markers,
        highlighted: syntax_window.styles,
        coverage: SyntaxCoverage::from_range(syntax_window.coverage),
        syntax_eligible,
        message: None,
    }
}

fn prepare_syntax_window(
    path: &Path,
    lines: &[String],
    window: SyntaxWindow,
    highlighter: &SyntaxHighlighter,
) -> HighlightedTextWindow {
    let Some(range) = visible_range(
        window.first_line,
        window.viewport_rows,
        window.window_viewports,
        lines.len(),
    ) else {
        return HighlightedTextWindow::default();
    };
    highlighter.highlight_text_window(
        path,
        lines,
        range,
        HIGHLIGHT_LOOKBEHIND_LINES,
        MAX_HIGHLIGHT_BYTES_PER_SIDE,
    )
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
    use diffo_core::{
        FailureKind, OperationFailure, RepositoryAction, RepositorySnapshot, RepositorySource,
    };
    use std::{sync::atomic::AtomicUsize, time::Duration};

    struct CountingRepository {
        file_reads: AtomicUsize,
    }

    impl RepositorySource for CountingRepository {
        fn snapshot(&self) -> anyhow::Result<RepositorySnapshot> {
            Ok(RepositorySnapshot::default())
        }
    }

    impl Repository for CountingRepository {
        fn explorer_file(&self, _path: &std::path::Path) -> anyhow::Result<ExplorerFile> {
            self.file_reads.fetch_add(1, Ordering::Relaxed);
            Ok(ExplorerFile {
                content: ExplorerFileContent::Text("line one\nline two\n".to_owned()),
                patch: String::new(),
            })
        }

        fn apply(
            &self,
            action: &RepositoryAction,
        ) -> std::result::Result<diffo_core::OperationResult, OperationFailure> {
            Err(OperationFailure {
                action: action.clone(),
                kind: FailureKind::Unknown,
                detail: "not used by Explorer worker tests".to_owned(),
            })
        }
    }

    #[test]
    fn syntax_windows_do_not_reread_repository_files() {
        let repository = Arc::new(CountingRepository {
            file_reads: AtomicUsize::new(0),
        });
        let worker = ExplorerWorker::start(repository.clone());
        worker.submit(ExplorerRequest::LoadFile {
            id: 1,
            path: PathBuf::from("file.txt"),
            title: Line::raw("file.txt"),
            status: None,
            first_line: 0,
            viewport_rows: 1,
            window_viewports: 3,
        });
        let loaded = worker
            .outcomes
            .recv_timeout(Duration::from_secs(1))
            .expect("file load outcome");
        let ExplorerOutcome::FileLoaded {
            result: Ok(viewer), ..
        } = loaded
        else {
            panic!("expected successful file load");
        };

        worker.submit(ExplorerRequest::HighlightWindow {
            id: 2,
            document_id: viewer.document_id,
            path: viewer.path.clone(),
            lines: viewer.lines.clone(),
            first_line: 1,
            viewport_rows: 1,
            window_viewports: 3,
        });
        assert!(matches!(
            worker
                .outcomes
                .recv_timeout(Duration::from_secs(1))
                .expect("syntax-window outcome"),
            ExplorerOutcome::WindowHighlighted { id: 2, .. }
        ));
        assert_eq!(repository.file_reads.load(Ordering::Relaxed), 1);

        worker.submit(ExplorerRequest::LoadFile {
            id: 3,
            path: PathBuf::from("file.txt"),
            title: Line::raw("file.txt"),
            status: None,
            first_line: 0,
            viewport_rows: 1,
            window_viewports: 3,
        });
        assert!(matches!(
            worker
                .outcomes
                .recv_timeout(Duration::from_secs(1))
                .expect("refresh outcome"),
            ExplorerOutcome::FileLoaded { id: 3, .. }
        ));
        assert_eq!(repository.file_reads.load(Ordering::Relaxed), 2);
    }

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
            ExplorerDocumentId(1),
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
            ExplorerDocumentId(2),
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
            ExplorerDocumentId(1),
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
