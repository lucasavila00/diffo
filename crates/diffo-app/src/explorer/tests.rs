use super::model::Viewer;
use super::*;
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::text::Line;
use std::collections::{BTreeMap, HashMap};

#[test]
fn stale_file_results_do_not_commit() {
    let mut explorer = ExplorerActivity::new(&RepositorySnapshot::default());
    explorer.latest_load = 2;
    explorer.pending_path = Some(PathBuf::from("new.rs"));
    explorer.model.viewer = Some(Viewer {
        document_id: ExplorerDocumentId(2),
        path: PathBuf::from("new.rs"),
        title: Box::new(Line::raw("M new.rs")),
        lines: vec!["new".to_owned()].into(),
        markers: HashMap::new(),
        highlighted: BTreeMap::new(),
        coverage: Vec::new().into(),
        syntax_eligible: false,
        message: None,
    });
    explorer.accept(ExplorerOutcome::FileLoaded {
        id: 1,
        result: Ok(Viewer {
            document_id: ExplorerDocumentId(1),
            path: PathBuf::from("old.rs"),
            title: Box::new(Line::raw("M old.rs")),
            lines: vec!["old".to_owned()].into(),
            markers: HashMap::new(),
            highlighted: BTreeMap::new(),
            coverage: Vec::new().into(),
            syntax_eligible: false,
            message: None,
        }),
    });
    let viewer = explorer.model.viewer.as_ref().unwrap();
    assert_eq!(viewer.path, PathBuf::from("new.rs"));
    assert_eq!(*viewer.title, Line::raw("M new.rs"));
    assert_eq!(viewer.lines.as_ref(), ["new"]);
    assert_eq!(explorer.pending_path, Some(PathBuf::from("new.rs")));
}

#[test]
fn filesystem_change_refreshes_paths_and_selected_content_without_git_changes() {
    let mut explorer = ExplorerActivity::new(&RepositorySnapshot::default());
    explorer.accept(ExplorerOutcome::Paths {
        id: 1,
        result: Ok(vec![PathBuf::from("ignored.txt")]),
    });
    explorer.prepare_frame(Rect::new(0, 0, 100, 30), PaneSplit::default());
    explorer.queued.clear();

    explorer.filesystem_changed();

    assert_eq!(explorer.queued.len(), 2);
    assert!(matches!(
        explorer.queued.front(),
        Some(ExplorerRequest::Paths { .. })
    ));
    assert!(matches!(
        explorer.queued.back(),
        Some(ExplorerRequest::LoadFile { path, .. })
            if path == &PathBuf::from("ignored.txt")
    ));
}

#[test]
fn filesystem_replacement_does_not_reuse_old_syntax_coverage() {
    let mut explorer = ExplorerActivity::new(&RepositorySnapshot::default());
    explorer.latest_load = 2;
    explorer.pending_path = Some(PathBuf::from("ignored.rs"));
    explorer.model.viewer = Some(Viewer {
        document_id: ExplorerDocumentId(1),
        path: PathBuf::from("ignored.rs"),
        title: Box::new(Line::raw("ignored.rs")),
        lines: vec!["old".to_owned()].into(),
        markers: HashMap::new(),
        highlighted: BTreeMap::from([(1, diffo_highlight::HighlightedLine::default())]),
        coverage: vec![diffo_highlight::LineRange { start: 1, end: 1 }].into(),
        syntax_eligible: true,
        message: None,
    });

    explorer.accept(ExplorerOutcome::FileLoaded {
        id: 2,
        result: Ok(Viewer {
            document_id: ExplorerDocumentId(2),
            path: PathBuf::from("ignored.rs"),
            title: Box::new(Line::raw("ignored.rs")),
            lines: vec!["new".to_owned()].into(),
            markers: HashMap::new(),
            highlighted: BTreeMap::new(),
            coverage: Vec::new().into(),
            syntax_eligible: true,
            message: None,
        }),
    });

    let viewer = explorer.model.viewer.as_ref().unwrap();
    assert_eq!(viewer.lines.as_ref(), ["new"]);
    assert!(viewer.highlighted.is_empty());
    assert!(viewer.coverage.is_empty());
}

#[test]
fn stale_syntax_window_cannot_contaminate_a_reloaded_document() {
    let mut explorer = ExplorerActivity::new(&RepositorySnapshot::default());
    explorer.model.viewer = Some(Viewer {
        document_id: ExplorerDocumentId(2),
        path: PathBuf::from("same.rs"),
        title: Box::new(Line::raw("same.rs")),
        lines: vec!["new".to_owned()].into(),
        markers: HashMap::new(),
        highlighted: BTreeMap::new(),
        coverage: Vec::new().into(),
        syntax_eligible: true,
        message: None,
    });

    explorer.accept(ExplorerOutcome::WindowHighlighted {
        id: 3,
        document_id: ExplorerDocumentId(1),
        result: diffo_highlight::HighlightedTextWindow {
            styles: std::collections::BTreeMap::from([(
                1,
                diffo_highlight::HighlightedLine::default(),
            )]),
            coverage: Some(diffo_highlight::LineRange::new(1, 1)),
            lines_processed: 1,
        },
    });

    let viewer = explorer.model.viewer.as_ref().unwrap();
    assert!(viewer.highlighted.is_empty());
    assert!(viewer.coverage.is_empty());
}

#[test]
fn uppercase_shortcuts_are_rejected() {
    let mut explorer = ExplorerActivity::new(&RepositorySnapshot::default());
    let event = Event::Key(crossterm::event::KeyEvent::new(
        KeyCode::Char('J'),
        KeyModifiers::SHIFT,
    ));
    assert!(
        explorer
            .handle_event(&event, Rect::default(), PaneSplit::default())
            .is_none()
    );
}

#[test]
fn quick_open_commits_tree_selection_with_the_viewer() {
    let mut explorer = ExplorerActivity::new(&RepositorySnapshot::default());
    explorer.accept(ExplorerOutcome::Paths {
        id: 1,
        result: Ok(vec![PathBuf::from("src/nested/main.rs")]),
    });
    explorer.prepare_frame(Rect::new(0, 0, 100, 30), PaneSplit::default());
    explorer.queued.clear();
    let prior = explorer.picker.selected().cloned();

    explorer.quick_open(PathBuf::from("src/nested/main.rs"));
    assert_eq!(explorer.picker.selected(), prior.as_ref());
    let request = explorer.take_request().expect("file request");
    let ExplorerRequest::LoadFile { id, .. } = request else {
        panic!("expected file request");
    };
    explorer.accept(ExplorerOutcome::FileLoaded {
        id,
        result: Ok(Viewer {
            document_id: ExplorerDocumentId(id),
            path: PathBuf::from("src/nested/main.rs"),
            title: Box::new(Line::raw("main.rs")),
            lines: vec!["ready".to_owned()].into(),
            markers: HashMap::new(),
            highlighted: BTreeMap::new(),
            coverage: Vec::new().into(),
            syntax_eligible: false,
            message: None,
        }),
    });

    assert_eq!(
        explorer.picker.selected(),
        Some(&EntryId::File(PathBuf::from("src/nested/main.rs")))
    );
    assert_eq!(explorer.picker.visible_rows(), 3);
    assert_eq!(explorer.document_paths().0, explorer.document_paths().1);
}

#[test]
fn clicking_a_directory_toggles_expansion() {
    let mut explorer = ExplorerActivity::new(&RepositorySnapshot::default());
    explorer.accept(ExplorerOutcome::Paths {
        id: 1,
        result: Ok(vec![PathBuf::from("src/main.rs")]),
    });
    let area = Rect::new(0, 0, 100, 30);
    explorer.prepare_frame(area, PaneSplit::default());
    let click = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 1,
        row: 1,
        modifiers: KeyModifiers::NONE,
    });

    assert!(
        explorer
            .handle_event(&click, area, PaneSplit::default())
            .is_some()
    );
    assert_eq!(explorer.picker.visible_rows(), 2);
    assert!(
        explorer
            .handle_event(&click, area, PaneSplit::default())
            .is_some()
    );
    assert_eq!(explorer.picker.visible_rows(), 1);
}

#[test]
fn tree_header_buttons_expand_and_collapse_every_directory() {
    let mut explorer = ExplorerActivity::new(&RepositorySnapshot::default());
    explorer.accept(ExplorerOutcome::Paths {
        id: 1,
        result: Ok(vec![PathBuf::from("src/nested/main.rs")]),
    });
    let area = Rect::new(0, 0, 100, 30);
    let split = PaneSplit::default();
    explorer.prepare_frame(area, split);
    let tree = explorer_areas(area, split).tree;
    let click = |column| {
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row: tree.y,
            modifiers: KeyModifiers::NONE,
        })
    };

    assert!(
        explorer
            .handle_event(&click(tree.right() - 4), area, split)
            .is_some()
    );
    assert_eq!(explorer.picker.visible_rows(), 3);
    assert!(
        explorer
            .handle_event(&click(tree.right() - 8), area, split)
            .is_some()
    );
    assert_eq!(explorer.picker.visible_rows(), 1);
}

#[test]
fn explorer_uses_the_shared_path_menu() {
    let mut explorer = ExplorerActivity::new(&RepositorySnapshot::default());
    explorer.accept(ExplorerOutcome::Paths {
        id: 1,
        result: Ok(vec![PathBuf::from("file.txt")]),
    });
    let area = Rect::new(0, 0, 100, 30);
    let split = PaneSplit::default();
    explorer.prepare_frame(area, split);
    let tree = explorer_areas(area, split).tree;
    let right_click = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: tree.x + 2,
        row: tree.y + 1,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(
        explorer.handle_event(&right_click, area, split),
        Some(ExplorerEvent::Consumed)
    );
    assert!(explorer.picker.has_open_menu());

    let copy_absolute = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: tree.x + 3,
        row: tree.y + 2,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(
        explorer.handle_event(&copy_absolute, area, split),
        Some(ExplorerEvent::CopyPath {
            path: PathBuf::from("file.txt"),
            absolute: true,
        })
    );

    let shortcut = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
    assert_eq!(
        explorer.handle_event(&shortcut, area, split),
        Some(ExplorerEvent::Consumed)
    );
    assert!(explorer.picker.has_open_menu());
}

#[test]
fn explorer_commands_use_the_same_state_transitions_as_header_buttons() {
    let mut explorer = ExplorerActivity::new(&RepositorySnapshot::default());
    explorer.accept(ExplorerOutcome::Paths {
        id: 1,
        result: Ok(vec![PathBuf::from("src/nested/main.rs")]),
    });
    explorer.prepare_frame(Rect::new(0, 0, 100, 30), PaneSplit::default());

    assert!(explorer.execute_command(EXPAND_ALL_COMMAND));
    assert_eq!(explorer.picker.visible_rows(), 3);
    assert!(explorer.execute_command(COLLAPSE_ALL_COMMAND));
    assert_eq!(explorer.picker.visible_rows(), 1);
    assert!(!explorer.execute_command(CommandId::new("unknown")));
}

#[test]
fn deleted_snapshot_path_is_absent_from_the_tree() {
    let snapshot = RepositorySnapshot {
        files: vec![diffo_core::FileState {
            path: PathBuf::from("foo"),
            old_path: None,
            kind: diffo_core::ChangeKind::Deleted,
            staged: None,
            unstaged: Some(diffo_core::FileDiff {
                text: String::new(),
            }),
        }],
        ..RepositorySnapshot::default()
    };
    let mut explorer = ExplorerActivity::new(&snapshot);
    explorer.accept(ExplorerOutcome::Paths {
        id: 1,
        result: Ok(vec![PathBuf::from("foo/bar.rs")]),
    });
    explorer.prepare_frame(Rect::new(0, 0, 100, 30), PaneSplit::default());

    assert_eq!(explorer.picker.visible_rows(), 1);
    assert_eq!(
        explorer.picker.selected(),
        Some(&EntryId::Directory(PathBuf::from("foo")))
    );

    assert!(explorer.execute_command(EXPAND_ALL_COMMAND));
    assert_eq!(explorer.picker.visible_rows(), 2);
    explorer
        .picker
        .navigate(diffo_ui::file_picker::Navigation::Next);
    assert_eq!(
        explorer.picker.selected(),
        Some(&EntryId::File(PathBuf::from("foo/bar.rs")))
    );

    assert!(explorer.execute_command(COLLAPSE_ALL_COMMAND));
    assert_eq!(explorer.picker.visible_rows(), 1);
    assert_eq!(
        explorer.picker.selected(),
        Some(&EntryId::Directory(PathBuf::from("foo")))
    );
    assert!(
        explorer
            .model
            .file_entry(std::path::Path::new("foo"))
            .is_none()
    );
}

#[test]
fn horizontal_pan_clamps_to_the_visible_code_width_and_returns_to_zero() {
    let mut explorer = ExplorerActivity::new(&RepositorySnapshot::default());
    explorer.model.viewer = Some(Viewer {
        document_id: ExplorerDocumentId(1),
        path: PathBuf::from("wide.txt"),
        title: Box::new(Line::raw("  wide.txt")),
        lines: vec!["x".repeat(100)].into(),
        markers: HashMap::new(),
        highlighted: BTreeMap::new(),
        coverage: Vec::new().into(),
        syntax_eligible: false,
        message: None,
    });
    let area = Rect::new(0, 0, 100, 30);
    explorer.prepare_frame(area, PaneSplit::default());
    let right = Event::Key(crossterm::event::KeyEvent::new(
        KeyCode::Right,
        KeyModifiers::NONE,
    ));
    let left = Event::Key(crossterm::event::KeyEvent::new(
        KeyCode::Left,
        KeyModifiers::NONE,
    ));

    for _ in 0..100 {
        if explorer
            .handle_event(&right, area, PaneSplit::default())
            .is_none()
        {
            break;
        }
    }
    assert_eq!(
        explorer.model.viewer_horizontal_scroll,
        100_usize.saturating_sub(explorer.viewport_columns)
    );
    assert!(
        explorer
            .handle_event(&right, area, PaneSplit::default())
            .is_none()
    );
    for _ in 0..100 {
        if explorer
            .handle_event(&left, area, PaneSplit::default())
            .is_none()
        {
            break;
        }
    }
    assert_eq!(explorer.model.viewer_horizontal_scroll, 0);
    assert!(
        explorer
            .handle_event(&left, area, PaneSplit::default())
            .is_none()
    );
}

#[test]
fn uncached_scroll_keeps_the_committed_viewport_until_coverage_arrives() {
    let mut explorer = ExplorerActivity::new(&RepositorySnapshot::default());
    let path = PathBuf::from("large.rs");
    let lines = (1..=100)
        .map(|line| format!("let value_{line} = {line};"))
        .collect::<Vec<_>>();
    explorer.model.viewer = Some(Viewer {
        document_id: ExplorerDocumentId(7),
        path: path.clone(),
        title: Box::new(Line::raw("  large.rs")),
        lines: lines.into(),
        markers: HashMap::new(),
        highlighted: BTreeMap::new(),
        coverage: vec![diffo_highlight::LineRange { start: 1, end: 20 }].into(),
        syntax_eligible: true,
        message: None,
    });
    explorer.viewport_rows = 10;

    explorer.scroll_viewer(40);
    assert_eq!(explorer.model.viewer_scroll, 0);
    assert_eq!(explorer.vertical_scroll.requested(), Some(40));
    assert!(explorer.viewer_syntax_ready());
    let request_id = explorer.latest_window;

    explorer.accept(ExplorerOutcome::WindowHighlighted {
        id: request_id,
        document_id: ExplorerDocumentId(7),
        result: diffo_highlight::HighlightedTextWindow {
            styles: std::collections::BTreeMap::default(),
            coverage: Some(diffo_highlight::LineRange { start: 41, end: 60 }),
            lines_processed: 20,
        },
    });
    explorer.prepare_viewer_scroll();

    assert_eq!(explorer.model.viewer_scroll, 40);
    assert_eq!(explorer.vertical_scroll.requested(), None);
    assert!(explorer.viewer_syntax_ready());
    assert!(
        explorer
            .model
            .viewer
            .as_ref()
            .unwrap()
            .coverage
            .iter()
            .any(|range| range.contains(1))
    );

    explorer.scroll_viewer(-40);
    assert_eq!(explorer.model.viewer_scroll, 0);
    assert!(explorer.viewer_syntax_ready());
    assert!(explorer.pending_path.is_none());
    assert!(explorer.pending_window.is_none());
}

#[test]
fn cold_scroll_targets_accumulate_and_reverse_without_moving_the_committed_viewport() {
    let mut explorer = ExplorerActivity::new(&RepositorySnapshot::default());
    let path = PathBuf::from("large.rs");
    explorer.model.viewer = Some(Viewer {
        document_id: ExplorerDocumentId(1),
        path,
        title: Box::new(Line::raw("  large.rs")),
        lines: (1..=200).map(|line| format!("line {line}")).collect(),
        markers: HashMap::new(),
        highlighted: BTreeMap::new(),
        coverage: vec![diffo_highlight::LineRange {
            start: 91,
            end: 120,
        }]
        .into(),
        syntax_eligible: true,
        message: None,
    });
    explorer.model.viewer_scroll = 100;
    explorer.viewport_rows = 10;

    explorer.scroll_viewer(-40);
    assert_eq!(explorer.model.viewer_scroll, 100);
    assert_eq!(explorer.vertical_scroll.requested(), Some(60));

    explorer.scroll_viewer(-20);
    assert_eq!(explorer.model.viewer_scroll, 100);
    assert_eq!(explorer.vertical_scroll.requested(), Some(40));

    explorer.scroll_viewer(80);
    assert_eq!(explorer.model.viewer_scroll, 100);
    assert_eq!(explorer.vertical_scroll.requested(), Some(120));
}

#[test]
fn syntax_window_requests_coalesce_and_share_the_committed_document() {
    let mut explorer = ExplorerActivity::new(&RepositorySnapshot::default());
    explorer.queued.clear();
    let lines = std::sync::Arc::<[String]>::from(
        (1..=100)
            .map(|line| format!("line {line}"))
            .collect::<Vec<_>>(),
    );
    explorer.model.viewer = Some(Viewer {
        document_id: ExplorerDocumentId(1),
        path: PathBuf::from("large.rs"),
        title: Box::new(Line::raw("  large.rs")),
        lines: lines.clone(),
        markers: HashMap::new(),
        highlighted: BTreeMap::new(),
        coverage: Vec::new().into(),
        syntax_eligible: true,
        message: None,
    });

    explorer.request_syntax_window(20);
    explorer.request_syntax_window(80);

    let requests = explorer.queued.iter().collect::<Vec<_>>();
    assert_eq!(requests.len(), 1);
    let ExplorerRequest::HighlightWindow {
        first_line,
        lines: requested_lines,
        ..
    } = requests[0]
    else {
        panic!("expected syntax-window request");
    };
    assert_eq!(*first_line, 80);
    assert!(std::sync::Arc::ptr_eq(&lines, requested_lines));
}
