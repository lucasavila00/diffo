use std::fmt::Write;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;
use std::time::Instant;

use crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use diffo_app::{ChangeArea, DiffViewMode, FileListScroll, Message, Model};
use diffo_core::{
    ChangeKind, FileDiff, FileState, HeadState, OperationResult, RepositoryAction,
    RepositorySnapshot, UpstreamState,
};
use diffo_diff::RowKind;
use diffo_highlight::Rgb;
use ratatui::{
    Terminal,
    backend::TestBackend,
    layout::Rect,
    style::{Color, Modifier},
};

use super::{
    Renderer, contrast_ratio, contrasting_foreground, diff_background, diff_background_rgb,
    diff_file_lines, file_kind_style, overview_position, row_style, scrollbar_position_count,
    should_syntax_highlight, status_line,
};

#[test]
fn file_list_styles_show_git_change_kinds() {
    assert_eq!(
        file_kind_style(ChangeKind::Untracked, false).fg,
        Some(Color::LightGreen)
    );
    assert_eq!(
        file_kind_style(ChangeKind::Added, false).fg,
        Some(Color::LightGreen)
    );
    assert_eq!(
        file_kind_style(ChangeKind::Modified, false).fg,
        Some(Color::Yellow)
    );
    let deleted = file_kind_style(ChangeKind::Deleted, false);
    assert_eq!(deleted.fg, Some(Color::LightRed));
    assert!(deleted.add_modifier.contains(Modifier::CROSSED_OUT));
    let conflicted = file_kind_style(ChangeKind::Conflicted, false);
    assert_eq!(conflicted.fg, Some(Color::LightRed));
    assert!(conflicted.add_modifier.contains(Modifier::BOLD));
    assert!(
        file_kind_style(ChangeKind::Added, true)
            .add_modifier
            .contains(Modifier::BOLD)
    );
}

#[test]
fn scrollbar_length_is_the_number_of_legal_viewport_positions() {
    assert_eq!(scrollbar_position_count(120, 25), 96);
    assert_eq!(scrollbar_position_count(25, 25), 1);
}

#[test]
fn maps_change_rows_across_the_overview_track() {
    assert_eq!(overview_position(0, 101, 11), 0);
    assert_eq!(overview_position(50, 101, 11), 5);
    assert_eq!(overview_position(100, 101, 11), 10);
}

#[test]
fn conflict_markers_have_a_dedicated_high_contrast_style() {
    let marker = row_style(RowKind::Conflict);
    assert_eq!(marker.fg, Some(Color::LightYellow));
    assert_eq!(marker.bg, Some(Color::Indexed(58)));
    assert!(marker.add_modifier.contains(Modifier::BOLD));
    assert_eq!(diff_background(RowKind::Conflict).bg, marker.bg);
}

#[test]
fn conflict_rows_require_trusted_repository_state() {
    let mut model = model();
    model.snapshot.files[0].unstaged.as_mut().unwrap().text =
        "@@ -1 +1,3 @@\n-old\n+<<<<<<< HEAD\n+ours\n+>>>>>>> branch\n".to_owned();
    let mut renderer = Renderer::new();

    renderer.prepare_frame(&model, Rect::new(0, 0, 100, 30));
    assert!(
        renderer
            .highlighted
            .as_ref()
            .unwrap()
            .inline
            .iter()
            .all(|row| row.kind != RowKind::Conflict)
    );

    model.snapshot.files[0].kind = ChangeKind::Conflicted;
    renderer.prepare_frame(&model, Rect::new(0, 0, 100, 30));
    assert!(
        renderer
            .highlighted
            .as_ref()
            .unwrap()
            .inline
            .iter()
            .any(|row| row.kind == RowKind::Conflict)
    );
}

fn model() -> Model {
    Model::new(RepositorySnapshot {
        files: vec![FileState {
            path: PathBuf::from("src/main.rs"),
            old_path: None,
            kind: ChangeKind::Modified,
            staged: None,
            unstaged: Some(FileDiff {
                text: "@@ -1 +1 @@\n-let old = true;\n+let new = false;\n".to_owned(),
            }),
        }],
        ..RepositorySnapshot::default()
    })
}

fn file_list_model(file_count: usize) -> Model {
    Model::new(RepositorySnapshot {
        files: (0..file_count)
            .map(|index| FileState {
                path: PathBuf::from(format!("generated/file-{index:03}.rs")),
                old_path: None,
                kind: ChangeKind::Modified,
                staged: Some(FileDiff {
                    text: format!("@@ -0,0 +1 @@\n+staged {index}\n"),
                }),
                unstaged: Some(FileDiff {
                    text: format!("@@ -0,0 +1 @@\n+unstaged {index}\n"),
                }),
            })
            .collect(),
        ..RepositorySnapshot::default()
    })
}

fn mouse_at(kind: MouseEventKind, area: Rect) -> Event {
    Event::Mouse(MouseEvent {
        kind,
        column: area.x,
        row: area.y,
        modifiers: KeyModifiers::NONE,
    })
}

fn buffer_text(buffer: &ratatui::buffer::Buffer) -> String {
    buffer
        .content
        .iter()
        .fold(String::new(), |mut output, cell| {
            output.push_str(cell.symbol());
            output
        })
}

fn line_text(line: &ratatui::text::Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

#[test]
fn status_line_shows_named_head_state_and_divergence() {
    let mut model = Model::new(RepositorySnapshot {
        head: HeadState::Named {
            name: "main".to_owned(),
            commit: "123456789abcdef".to_owned(),
        },
        upstream: Some(UpstreamState {
            name: "origin/main".to_owned(),
            ahead: 2,
            behind: 1,
        }),
        ..RepositorySnapshot::default()
    });

    let line = status_line(&model, 0, 80);
    assert!(line_text(&line).contains(" branch main · clean · ↓1 ↑2"));
    assert_eq!(line.spans[0].style.fg, Some(Color::Cyan));
    assert!(line.spans[0].style.add_modifier.contains(Modifier::BOLD));

    model.snapshot.files.push(FileState {
        path: PathBuf::from("changed.rs"),
        old_path: None,
        kind: ChangeKind::Modified,
        staged: None,
        unstaged: Some(FileDiff {
            text: "@@ -1 +1 @@\n-old\n+new\n".to_owned(),
        }),
    });
    assert!(line_text(&status_line(&model, 0, 80)).contains(" · changes"));

    model.snapshot.files[0].staged = model.snapshot.files[0].unstaged.clone();
    assert!(line_text(&status_line(&model, 0, 80)).contains(" · staged"));

    model.snapshot.files[0].kind = ChangeKind::Conflicted;
    let line = status_line(&model, 0, 80);
    assert!(line_text(&line).contains(" · conflicts"));
    assert_eq!(line.spans[1].style.fg, Some(Color::LightRed));
}

#[test]
fn status_line_distinguishes_unborn_and_detached_head() {
    let mut model = Model::new(RepositorySnapshot {
        head: HeadState::Unborn {
            name: "main".to_owned(),
        },
        ..RepositorySnapshot::default()
    });
    assert!(line_text(&status_line(&model, 0, 80)).contains(" branch main (unborn) · clean"));

    model.snapshot.head = HeadState::Detached {
        commit: "123456789abcdef".to_owned(),
    };
    assert!(line_text(&status_line(&model, 0, 80)).contains(" detached 1234567 · clean"));
}

#[test]
fn status_line_preserves_head_and_respects_unicode_width() {
    let mut model = Model::new(RepositorySnapshot {
        head: HeadState::Named {
            name: "feature/日本語-very-long".to_owned(),
            commit: "123456789abcdef".to_owned(),
        },
        upstream: Some(UpstreamState {
            name: "origin/feature".to_owned(),
            ahead: 3,
            behind: 4,
        }),
        ..RepositorySnapshot::default()
    });
    model.error = Some("Checkout failed: local changes".to_owned());

    let line = status_line(&model, 0, 24);
    assert_eq!(line.width(), 24);
    assert!(line_text(&line).starts_with(" branch feature/"));
    assert!(line_text(&line).ends_with('…'));
    assert!(!line_text(&line).contains("↓4"));

    let minimum = status_line(&model, 0, 1);
    assert_eq!(minimum.width(), 1);
    assert_eq!(line_text(&minimum), "…");
}

#[test]
fn status_line_keeps_the_head_visible_with_transient_errors() {
    let mut model = Model::new(RepositorySnapshot {
        head: HeadState::Named {
            name: "main".to_owned(),
            commit: "123456789abcdef".to_owned(),
        },
        ..RepositorySnapshot::default()
    });
    model.error = Some("Checkout failed: local changes".to_owned());

    let line = status_line(&model, 0, 40);
    let text = line_text(&line);
    assert_eq!(line.width(), 40);
    assert!(text.starts_with(" branch main  Checkout"));
    assert!(text.ends_with('…'));
    assert!(!text.contains("1/f1"));
}

#[test]
fn file_list_scrollbars_have_independent_offsets_and_exact_hit_targets() {
    let mut model = file_list_model(30);
    model.file_list_scroll = FileListScroll {
        staged: 3,
        unstaged: 7,
    };
    let area = Rect::new(0, 0, 100, 30);
    let mut renderer = Renderer::new();
    let preparation = renderer.prepare_frame(&model, area);
    model.set_file_list_scrolls(preparation.file_list_scroll);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| renderer.render(frame, &model))
        .unwrap();

    assert_eq!(renderer.file_lists.staged.offset, 3);
    assert_eq!(renderer.file_lists.unstaged.offset, 7);
    assert!(renderer.file_lists.staged.maximum_scroll > 0);
    assert!(renderer.file_lists.unstaged.maximum_scroll > 0);

    let unstaged = renderer.file_lists.unstaged;
    let row_click = mouse_at(MouseEventKind::Down(MouseButton::Left), unstaged.list_area);
    assert_eq!(
        renderer.map_event(&row_click, &model, area),
        Some(Message::SelectFile(diffo_app::FileKey {
            path: PathBuf::from("generated/file-007.rs"),
            area: ChangeArea::Unstaged,
        }))
    );

    let mut action_area = unstaged.list_area;
    action_area.x = action_area.right().saturating_sub(1);
    action_area.width = 1;
    let action_click = mouse_at(MouseEventKind::Down(MouseButton::Left), action_area);
    assert_eq!(
        renderer.map_event(&action_click, &model, area),
        Some(Message::StageFile(PathBuf::from("generated/file-007.rs")))
    );

    let mut bottom = unstaged.scrollbar_area;
    bottom.y = bottom.bottom().saturating_sub(1);
    bottom.height = 1;
    let scrollbar_click = mouse_at(MouseEventKind::Down(MouseButton::Left), bottom);
    assert_eq!(
        renderer.map_event(&scrollbar_click, &model, area),
        Some(Message::SetFileListScroll(
            ChangeArea::Unstaged,
            unstaged.maximum_scroll,
        ))
    );

    let drag_to_top = mouse_at(
        MouseEventKind::Drag(MouseButton::Left),
        unstaged.scrollbar_area,
    );
    assert_eq!(
        renderer.map_event(&drag_to_top, &model, area),
        Some(Message::SetFileListScroll(ChangeArea::Unstaged, 0))
    );
}

#[test]
fn file_list_scrollbars_hide_without_overflow_and_offsets_clamp() {
    let mut model = file_list_model(1);
    model.file_list_scroll = FileListScroll {
        staged: usize::MAX,
        unstaged: usize::MAX,
    };
    let area = Rect::new(0, 0, 100, 30);
    let mut renderer = Renderer::new();
    let preparation = renderer.prepare_frame(&model, area);
    assert_eq!(preparation.file_list_scroll, FileListScroll::default());
    model.set_file_list_scrolls(preparation.file_list_scroll);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| renderer.render(frame, &model))
        .unwrap();

    assert_eq!(renderer.file_lists.staged.maximum_scroll, 0);
    assert_eq!(renderer.file_lists.unstaged.maximum_scroll, 0);
    assert!(renderer.file_lists.staged.scrollbar_area.is_empty());
    assert!(renderer.file_lists.unstaged.scrollbar_area.is_empty());
}

#[test]
fn jumps_between_change_blocks_and_wraps() {
    let mut model = model();
    model.snapshot.files[0].unstaged.as_mut().unwrap().text =
            "@@ -1,7 +1,7 @@\n one\n-old two\n+new two\n three\n four\n-old five\n+new five\n six\n seven\n"
                .to_owned();
    let mut renderer = Renderer::new();
    renderer.prepare_frame(&model, Rect::new(0, 0, 100, 30));

    let first = renderer.change_jump(&model, true).expect("first change");
    model.diff_scroll = first;
    let second = renderer.change_jump(&model, true).expect("second change");
    assert!(second > first);
    model.diff_scroll = second;
    assert_eq!(renderer.change_jump(&model, true), Some(first));
    assert_eq!(renderer.change_jump(&model, false), Some(first));
}

#[test]
fn network_operations_animate_the_frame_and_name_the_operation() {
    let mut model = model();
    model.snapshot.files[0].unstaged = None;
    model.snapshot.upstream = Some(UpstreamState {
        name: "origin/main".to_owned(),
        ahead: 1,
        behind: 0,
    });
    assert_eq!(model.execute_primary_action(), Some(RepositoryAction::Push));

    let mut renderer = Renderer::new();
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| renderer.render(frame, &model))
        .unwrap();
    let first_border = terminal.backend().buffer()[(0, 0)].fg;
    let screen =
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .fold(String::new(), |mut output, cell| {
                output.push_str(cell.symbol());
                output
            });
    assert!(screen.contains("Pushing"));

    for _ in 0..4 {
        terminal
            .draw(|frame| renderer.render(frame, &model))
            .unwrap();
    }
    assert_ne!(terminal.backend().buffer()[(0, 0)].fg, first_border);
}

#[test]
fn renders_and_mouse_dismisses_a_bottom_right_toast() {
    let mut model = model();
    model.snapshot.files[0].staged = model.snapshot.files[0].unstaged.take();
    let action = model.execute_primary_action().expect("commit action");
    assert!(matches!(&action, RepositoryAction::Commit(_)));
    model.complete_operation(
        &action,
        &OperationResult::Commit {
            hash: "a1b2c3d4e5".to_owned(),
        },
        model.snapshot.clone(),
    );
    let id = model.toasts[0].id;
    let mut renderer = Renderer::new();
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    renderer.prepare_frame(&model, Rect::new(0, 0, 100, 30));
    wait_for_syntax_ready(&mut renderer, &model);
    terminal
        .draw(|frame| renderer.render(frame, &model))
        .unwrap();
    assert!(
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .any(|cell| { cell.symbol().contains("Committed") || cell.fg == Color::LightGreen })
    );

    let click = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 70,
        row: 26,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(
        renderer.map_event(&click, &model, Rect::new(0, 0, 100, 30)),
        Some(diffo_app::Message::DismissToast(id))
    );
}

fn diff_lines(
    renderer: &mut Renderer,
    model: &Model,
    first_row: usize,
) -> Vec<ratatui::text::Line<'static>> {
    for _ in 0..200 {
        renderer.prepare_frame(model, Rect::new(0, 0, 100, 30));
        let lines = renderer.diff_lines(model, 80, first_row, 100);
        if !renderer.is_preparing() {
            return lines;
        }
        sleep(Duration::from_millis(1));
    }
    panic!("diff preparation timed out");
}

fn wait_for_viewport_transition(
    renderer: &mut Renderer,
    model: &Model,
) -> super::ViewportTransition {
    for _ in 0..200 {
        let preparation = renderer.prepare_frame(model, Rect::new(0, 0, 100, 30));
        if let Some(viewport) = preparation.viewport_transition {
            return viewport;
        }
        sleep(Duration::from_millis(1));
    }
    panic!("viewport preparation timed out");
}

fn wait_for_syntax_ready(renderer: &mut Renderer, model: &Model) {
    for _ in 0..200 {
        let preparation = renderer.prepare_frame(model, Rect::new(0, 0, 100, 30));
        if preparation.syntax_ready {
            return;
        }
        sleep(Duration::from_millis(1));
    }
    panic!("syntax preparation timed out");
}

#[test]
fn renders_syntax_foregrounds_over_diff_backgrounds() {
    let mut renderer = Renderer::new();
    let model = model();
    let lines = diff_lines(&mut renderer, &model, 0);
    assert!(!lines.is_empty());
    assert!(!renderer.is_preparing());
    let removed = &lines[1];
    let added = &lines[2];

    assert!(removed.spans.iter().any(|span| span.style.fg.is_some()));
    assert!(
        removed
            .spans
            .iter()
            .any(|span| { span.style.bg == Some(Color::Indexed(52)) })
    );
    assert!(
        added
            .spans
            .iter()
            .any(|span| { span.style.bg == Some(Color::Indexed(22)) })
    );
    assert_eq!(removed.spans[0].style.fg, Some(Color::LightRed));
    assert_eq!(added.spans[0].style.fg, Some(Color::LightGreen));
    assert!(
        removed.spans[1..]
            .iter()
            .all(|span| span.style.add_modifier.is_empty()),
        "syntax highlighting should not emit terminal font attributes"
    );
    assert_eq!(
        removed
            .spans
            .iter()
            .map(|span| span.content.chars().count())
            .sum::<usize>(),
        80
    );
}

#[test]
fn prepares_large_diffs_in_the_background() {
    let mut model = model();
    let mut patch = String::from("@@ -0,0 +1,501 @@\n");
    for index in 0..501 {
        writeln!(patch, "+line {index}").unwrap();
    }
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch;
    let mut renderer = Renderer::new();

    renderer.prepare_frame(&model, Rect::new(0, 0, 100, 30));
    let pending = renderer.diff_lines(&model, 80, 0, 100);
    assert!(pending.is_empty());
    assert!(renderer.is_preparing());

    let lines = diff_lines(&mut renderer, &model, 0);
    assert!(!lines.is_empty());
    assert!(!renderer.is_preparing());
}

#[test]
fn keeps_previous_diff_visible_while_preparing() {
    let mut model = model();
    let mut renderer = Renderer::new();
    let previous = diff_lines(&mut renderer, &model, 0);
    let mut patch = String::from("@@ -0,0 +1,501 @@\n");
    for index in 0..501 {
        writeln!(patch, "+line {index}").unwrap();
    }
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch;

    renderer.prepare_frame(&model, Rect::new(0, 0, 100, 30));
    let during_transition = renderer.diff_lines(&model, 80, 0, 100);

    assert_eq!(during_transition, previous);
    assert!(renderer.is_preparing());
}

#[test]
fn commits_a_new_file_and_its_first_change_position_together() {
    let mut model = model();
    let previous_file = model.selected.clone().unwrap();
    let mut patch = String::from("@@ -1,501 +1,501 @@\n");
    for index in 0..501 {
        if index == 449 {
            writeln!(patch, "-old line {index}").unwrap();
            writeln!(patch, "+new line {index}").unwrap();
        } else {
            writeln!(patch, " context line {index}").unwrap();
        }
    }
    model.snapshot.files.push(FileState {
        path: PathBuf::from("src/second.rs"),
        old_path: None,
        kind: ChangeKind::Modified,
        staged: None,
        unstaged: Some(FileDiff { text: patch }),
    });
    let mut renderer = Renderer::new();
    let area = Rect::new(0, 0, 100, 30);
    renderer.prepare_frame(&model, area);
    let previous = renderer.diff_lines(&model, 80, 0, 100);
    model.diff_scroll = 7;
    model.diff_horizontal_scroll = 9;
    model.select_next();

    let pending = renderer.prepare_frame(&model, area);

    assert!(pending.viewport_transition.is_none());
    assert_eq!(pending.displayed_file, Some(previous_file));
    assert_eq!(renderer.diff_lines(&model, 80, 0, 100), previous);
    assert_eq!((model.diff_scroll, model.diff_horizontal_scroll), (7, 9));

    let committed = (0..200)
        .find_map(|_| {
            let preparation = renderer.prepare_frame(&model, area);
            if preparation.viewport_transition.is_some() {
                Some(preparation)
            } else {
                sleep(Duration::from_millis(1));
                None
            }
        })
        .expect("second diff preparation timed out");
    let transition = committed.viewport_transition.unwrap();
    assert_eq!(committed.displayed_file, model.selected);
    assert_eq!(transition.vertical, 450);
    assert_eq!(transition.horizontal, 0);
    assert!(
        renderer.diff_lines(&model, 80, transition.vertical, 1)[0]
            .to_string()
            .contains("old line 449")
    );
}

#[test]
fn staged_and_unstaged_buffers_of_one_path_have_distinct_identities() {
    let mut snapshot = model().snapshot;
    snapshot.files[0].staged = Some(FileDiff {
        text: "@@ -1,3 +1,3 @@\n-old\n+staged\n context\n context\n".to_owned(),
    });
    snapshot.files[0].unstaged = Some(FileDiff {
        text: "@@ -1,3 +1,3 @@\n context\n context\n-old\n+unstaged\n".to_owned(),
    });
    let mut model = Model::new(snapshot);
    model.select_previous();
    let mut renderer = Renderer::new();
    let area = Rect::new(0, 0, 100, 30);
    let staged = renderer.prepare_frame(&model, area);
    let staged_revision = staged.content_revision;
    assert_eq!(staged.viewport_transition.unwrap().vertical, 1);
    model.diff_scroll = 17;
    model.diff_horizontal_scroll = 8;

    model.select_next();
    assert_eq!((model.diff_scroll, model.diff_horizontal_scroll), (17, 8));
    let unstaged = renderer.prepare_frame(&model, area);

    assert!(unstaged.content_revision > staged_revision);
    assert_eq!(unstaged.displayed_file, model.selected);
    let transition = unstaged.viewport_transition.unwrap();
    assert_eq!(transition.vertical, 3);
    assert_eq!(transition.horizontal, 0);
}

#[test]
fn anchors_the_first_visible_row_when_content_moves_above_it() {
    let mut inline_model = model();
    let patch = |prefix: &[&str]| {
        let mut patch = format!("@@ -0,0 +1,{} @@\n", prefix.len() + 40);
        for line in prefix {
            writeln!(patch, "+{line}").unwrap();
        }
        for index in 0..40 {
            writeln!(patch, "+stable line {index}").unwrap();
        }
        patch
    };
    inline_model.snapshot.files[0]
        .unstaged
        .as_mut()
        .unwrap()
        .text = patch(&[]);
    inline_model.diff_scroll = 12;
    let mut renderer = Renderer::new();
    let area = Rect::new(0, 0, 100, 30);
    let initial = renderer.prepare_frame(&inline_model, area);
    assert_eq!(initial.viewport_transition.unwrap().vertical, 1);

    inline_model.snapshot.files[0]
        .unstaged
        .as_mut()
        .unwrap()
        .text = patch(&["inserted one", "inserted two", "inserted three"]);
    let changed = renderer.prepare_frame(&inline_model, area);

    assert_eq!(changed.viewport_transition.unwrap().vertical, 15);

    let mut side_model = model();
    side_model.diff_view_mode = DiffViewMode::SideBySide;
    side_model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch(&[]);
    side_model.diff_scroll = 12;
    let mut side_renderer = Renderer::new();
    side_renderer.prepare_frame(&side_model, area);
    side_model.snapshot.files[0].unstaged.as_mut().unwrap().text =
        patch(&["inserted one", "inserted two", "inserted three"]);

    let side_changed = side_renderer.prepare_frame(&side_model, area);

    assert_eq!(side_changed.viewport_transition.unwrap().vertical, 15);
}

#[test]
fn uses_the_next_visible_row_when_the_anchor_was_deleted() {
    let mut model = model();
    let patch = |skip: Option<usize>| {
        let mut patch = String::from("@@ -0,0 +1,40 @@\n");
        for index in 0..40 {
            if skip != Some(index) {
                writeln!(patch, "+stable line {index}").unwrap();
            }
        }
        patch
    };
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch(None);
    model.diff_scroll = 12;
    let mut renderer = Renderer::new();
    let area = Rect::new(0, 0, 100, 30);
    renderer.prepare_frame(&model, area);

    model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch(Some(11));
    let changed = renderer.prepare_frame(&model, area);

    assert_eq!(changed.viewport_transition.unwrap().vertical, 11);
}

#[test]
fn renders_invalid_patches_as_raw_text() {
    let mut model = model();
    model.snapshot.files[0].unstaged.as_mut().unwrap().text =
        "diff --cc src/main.rs\n@@@ malformed\n+raw line\n".to_owned();
    let mut renderer = Renderer::new();

    renderer.prepare_frame(&model, Rect::new(0, 0, 100, 30));
    let lines = renderer.diff_lines(&model, 80, 0, 100);

    assert_eq!(lines[0].to_string(), "diff --cc src/main.rs");
    assert_eq!(lines[2].to_string(), "+raw line");
    assert!(!renderer.is_preparing());
}

#[test]
fn maps_inset_scrollbar_clicks_to_absolute_positions() {
    let mut model = model();
    let mut patch = String::from("@@ -0,0 +1,100 @@\n");
    for _ in 0..100 {
        writeln!(patch, "+{}", "x".repeat(200)).unwrap();
    }
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch;
    let mut renderer = Renderer::new();
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    renderer.prepare_frame(&model, Rect::new(0, 0, 100, 30));
    terminal
        .draw(|frame| renderer.render(frame, &model))
        .unwrap();

    let vertical = renderer.scrollbars.vertical_area;
    let vertical_click = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: vertical.right().saturating_sub(1),
        row: vertical.bottom().saturating_sub(1),
        modifiers: KeyModifiers::NONE,
    });
    let Some(diffo_app::Message::SetDiffScroll(vertical_target)) =
        renderer.map_event(&vertical_click, &model, Rect::new(0, 0, 100, 30))
    else {
        panic!("vertical scrollbar did not return an absolute target");
    };
    assert!(vertical_target > 0);

    renderer.scrollbar_drag = None;
    let horizontal = renderer.scrollbars.horizontal_area;
    let horizontal_click = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: horizontal.right().saturating_sub(1),
        row: horizontal.bottom().saturating_sub(1),
        modifiers: KeyModifiers::NONE,
    });
    let horizontal_maximum = renderer
        .scrollbars
        .columns
        .saturating_sub(renderer.scrollbars.viewport_columns);
    assert!(matches!(
        renderer.map_event(&horizontal_click, &model, Rect::new(0, 0, 100, 30)),
        Some(diffo_app::Message::SetDiffHorizontalScroll(position))
            if position == horizontal_maximum
    ));
    model.diff_scroll = vertical_target;
    let skeleton = renderer.prepare_frame(&model, Rect::new(0, 0, 100, 30));
    assert!(skeleton.viewport_transition.is_none());
    if !skeleton.syntax_ready {
        wait_for_syntax_ready(&mut renderer, &model);
    }
    assert_eq!(model.diff_scroll, vertical_target);
}

#[test]
fn horizontal_scrollbar_tracks_only_the_visible_vertical_slice() {
    let mut model = model();
    let mut patch = String::from("@@ -1,100 +1,100 @@\n-old first\n+new first\n");
    for line in 0..100 {
        if line == 80 {
            writeln!(patch, " {}", "wide-content-".repeat(20)).unwrap();
        } else {
            writeln!(patch, " short line {line}").unwrap();
        }
    }
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch;
    let mut renderer = Renderer::new();
    let area = Rect::new(0, 0, 100, 30);

    let top = renderer.prepare_frame(&model, area);
    assert_eq!(top.maximum_horizontal_scroll, 0);

    model.diff_scroll = 70;
    let wide = renderer.prepare_frame(&model, area);
    assert!(wide.maximum_horizontal_scroll > 0);
    model.diff_horizontal_scroll = wide.maximum_horizontal_scroll;

    model.diff_scroll = 0;
    let top_again = renderer.prepare_frame(&model, area);
    assert_eq!(top_again.maximum_horizontal_scroll, 0);
    model.clamp_diff_scroll(
        top_again.maximum_vertical_scroll,
        top_again.maximum_horizontal_scroll,
    );
    assert_eq!(model.diff_horizontal_scroll, 0);
}

#[test]
fn uncached_scroll_uses_one_viewport_and_skeleton_until_syntax_is_ready() {
    let mut model = model();
    let mut patch = String::from("@@ -1,700 +1,700 @@\n");
    for line in 1..=700 {
        writeln!(patch, " let value_{line} = {line};").unwrap();
    }
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch;
    let mut renderer = Renderer::new();
    let area = Rect::new(0, 0, 100, 30);
    diff_lines(&mut renderer, &model, 0);

    model.diff_scroll = 600;
    let first = renderer.prepare_frame(&model, area);
    assert!(!first.syntax_ready);
    assert!(first.viewport_transition.is_none());
    let skeleton = renderer.diff_skeleton_lines(80, model.diff_scroll, 20);
    assert!(!skeleton.is_empty());
    assert!(skeleton.iter().all(|line| {
        line.spans.iter().all(|span| {
            span.content.chars().all(|character| {
                character.is_ascii_digit() || character.is_whitespace() || character == '│'
            })
        })
    }));

    model.diff_scroll = 650;
    let newest = renderer.prepare_frame(&model, area);
    assert!(!newest.syntax_ready);
    wait_for_syntax_ready(&mut renderer, &model);
    assert_eq!(model.diff_scroll, 650);
    assert!(renderer.syntax_ready_for_viewport(DiffViewMode::Inline, 650));
}

#[test]
fn hunk_markers_have_a_separate_clickable_rail_beside_the_scrollbar() {
    let mut model = model();
    let mut patch = String::from("@@ -1,100 +1,100 @@\n");
    for line in 1..=100 {
        if matches!(line, 2 | 90) {
            writeln!(patch, "-old {line}").unwrap();
            writeln!(patch, "+new {line}").unwrap();
        } else {
            writeln!(patch, " line {line}").unwrap();
        }
    }
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch;
    let mut renderer = Renderer::new();
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    renderer.prepare_frame(&model, Rect::new(0, 0, 100, 30));
    wait_for_syntax_ready(&mut renderer, &model);
    terminal
        .draw(|frame| renderer.render(frame, &model))
        .unwrap();

    let changes = &renderer.highlighted.as_ref().unwrap().inline_changes;
    let target = changes[1];
    let marker_column = renderer.scrollbars.vertical_area.x.saturating_add(1);
    let marker_row = renderer.scrollbars.vertical_area.y
        + overview_position(
            target,
            renderer.scrollbars.rows,
            renderer.scrollbars.vertical_area.height,
        );
    let click = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: marker_column,
        row: marker_row,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(
        terminal.backend().buffer()[(marker_column, marker_row)].symbol(),
        "▪"
    );
    assert_ne!(
        terminal.backend().buffer()[(renderer.scrollbars.vertical_area.x, marker_row)].symbol(),
        "▪"
    );
    let visible_marker_row = renderer.scrollbars.vertical_area.y
        + overview_position(
            changes[0],
            renderer.scrollbars.rows,
            renderer.scrollbars.vertical_area.height,
        );
    assert_eq!(
        terminal.backend().buffer()[(marker_column, visible_marker_row)].symbol(),
        "▪"
    );
    assert_eq!(
        renderer.change_at_marker(renderer.scrollbars.vertical_area.x, marker_row, &model),
        None
    );
    assert_eq!(
        renderer.scrollbar_at(renderer.scrollbars.vertical_area.x, marker_row),
        Some(super::ScrollbarAxis::Vertical)
    );
    assert_eq!(
        renderer.map_event(&click, &model, Rect::new(0, 0, 100, 30)),
        Some(diffo_app::Message::SetDiffScroll(target))
    );
    model.diff_scroll = target;
    let skeleton = renderer.prepare_frame(&model, Rect::new(0, 0, 100, 30));
    assert!(skeleton.viewport_transition.is_none());
    if !skeleton.syntax_ready {
        wait_for_syntax_ready(&mut renderer, &model);
    }
    assert_eq!(model.diff_scroll, target);
}

#[test]
fn large_hunk_buttons_are_fixed_and_do_not_wrap() {
    let mut model = model();
    let mut patch = String::from("@@ -1,100 +1,100 @@\n");
    for line in 1..=100 {
        if matches!(line, 2 | 50 | 90) {
            writeln!(patch, "-old {line}").unwrap();
            writeln!(patch, "+new {line}").unwrap();
        } else if line == 10 {
            writeln!(patch, " {}", "wide-content-".repeat(20)).unwrap();
        } else {
            writeln!(patch, " line {line}").unwrap();
        }
    }
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch;
    let area = Rect::new(0, 0, 100, 30);
    let mut renderer = Renderer::new();
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    let top_preparation = renderer.prepare_frame(&model, area);
    wait_for_syntax_ready(&mut renderer, &model);
    terminal
        .draw(|frame| renderer.render(frame, &model))
        .unwrap();

    assert!(renderer.hunk_buttons.previous.is_none());
    assert!(buffer_text(terminal.backend().buffer()).contains("Next change (n)"));
    let (next_area, next_target) = renderer.hunk_buttons.next.expect("next button");
    assert!(renderer.scrollbars.horizontal_area.height > 0);
    assert_eq!(next_area.bottom(), renderer.scrollbars.horizontal_area.y);
    assert_eq!(
        renderer.map_event(
            &mouse_at(MouseEventKind::Down(MouseButton::Left), next_area),
            &model,
            area,
        ),
        Some(diffo_app::Message::SetDiffScroll(next_target))
    );

    model.diff_scroll = next_target;
    renderer.prepare_frame(&model, area);
    terminal
        .draw(|frame| renderer.render(frame, &model))
        .unwrap();
    let (previous_area, previous_target) = renderer.hunk_buttons.previous.expect("previous button");
    assert!(buffer_text(terminal.backend().buffer()).contains("Previous change (p)"));
    assert_eq!(previous_area.y, area.y.saturating_add(1));
    assert!(renderer.hunk_buttons.next.is_some());
    assert_eq!(
        renderer.map_event(
            &mouse_at(MouseEventKind::Down(MouseButton::Left), previous_area),
            &model,
            area,
        ),
        Some(diffo_app::Message::SetDiffScroll(previous_target))
    );

    model.diff_scroll = renderer
        .highlighted
        .as_ref()
        .unwrap()
        .inline_changes
        .last()
        .copied()
        .unwrap();
    let end_preparation = renderer.prepare_frame(&model, area);
    terminal
        .draw(|frame| renderer.render(frame, &model))
        .unwrap();
    assert!(renderer.hunk_buttons.next.is_none());
    assert_eq!(
        end_preparation.maximum_vertical_scroll,
        top_preparation.maximum_vertical_scroll
    );
    assert_eq!(
        renderer.hunk_button_target_at(next_area.x, next_area.y),
        None
    );
}

#[test]
fn passive_mouse_movement_does_not_change_hunk_buttons_or_request_actions() {
    let mut model = model();
    let mut patch = String::from("@@ -1,100 +1,100 @@\n");
    for line in 1..=100 {
        if matches!(line, 2 | 50 | 90) {
            writeln!(patch, "-old {line}").unwrap();
            writeln!(patch, "+new {line}").unwrap();
        } else {
            writeln!(patch, " line {line}").unwrap();
        }
    }
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch;
    model.diff_scroll = 50;
    let area = Rect::new(0, 0, 100, 30);
    let mut renderer = Renderer::new();
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    renderer.prepare_frame(&model, area);
    terminal
        .draw(|frame| renderer.render(frame, &model))
        .unwrap();

    let previous = renderer.hunk_buttons.previous.expect("previous button").0;
    let next = renderer.hunk_buttons.next.expect("next button").0;
    let before_movement = terminal.backend().buffer().clone();
    let positions = [
        previous,
        Rect::new(previous.right().saturating_sub(1), previous.y, 1, 1),
        next,
        Rect::new(next.right().saturating_sub(1), next.y, 1, 1),
        Rect::new(area.x, area.y, 1, 1),
    ];

    for _ in 0..100 {
        for position in positions {
            assert_eq!(
                renderer.map_event(&mouse_at(MouseEventKind::Moved, position), &model, area),
                None
            );
        }
    }
    terminal
        .draw(|frame| renderer.render(frame, &model))
        .unwrap();

    assert_eq!(
        terminal.backend().buffer(),
        &before_movement,
        "passive movement must produce zero changed terminal cells"
    );
}

#[test]
fn prepares_view_modes_lazily_caches_them_and_invalidates_changed_patch() {
    let mut renderer = Renderer::new();
    let mut model = model();

    diff_lines(&mut renderer, &model, 0);
    model.diff_view_mode = DiffViewMode::SideBySide;
    diff_lines(&mut renderer, &model, 0);
    assert_eq!(renderer.highlight_computations, 2);

    model.diff_view_mode = DiffViewMode::Inline;
    diff_lines(&mut renderer, &model, 0);
    assert_eq!(renderer.highlight_computations, 2);

    model.snapshot.files[0]
        .unstaged
        .as_mut()
        .expect("unstaged diff")
        .text
        .push_str("\\ No newline at end of file\n");
    diff_lines(&mut renderer, &model, 0);
    assert_eq!(renderer.highlight_computations, 3);
}

#[test]
fn view_mode_and_reset_viewport_commit_together() {
    let mut renderer = Renderer::new();
    let mut model = model();
    let mut patch = String::from("@@ -1,700 +1,700 @@\n");
    for line in 1..=700 {
        if line == 600 {
            writeln!(patch, "-let old_target = {line};").unwrap();
            writeln!(patch, "+let new_target = {line};").unwrap();
        } else {
            writeln!(patch, " let context_{line} = {line};").unwrap();
        }
    }
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch;
    diff_lines(&mut renderer, &model, 0);
    assert_eq!(
        renderer.highlighted.as_ref().unwrap().key.mode,
        DiffViewMode::Inline
    );

    model.diff_scroll = 10;
    model.diff_horizontal_scroll = 5;
    model.diff_view_mode = DiffViewMode::SideBySide;
    let pending = renderer.prepare_frame(&model, Rect::new(0, 0, 100, 30));
    assert!(pending.viewport_transition.is_none());
    assert_eq!(
        renderer.highlighted.as_ref().unwrap().key.mode,
        DiffViewMode::Inline
    );
    assert_eq!((model.diff_scroll, model.diff_horizontal_scroll), (10, 5));

    let transition = wait_for_viewport_transition(&mut renderer, &model);
    assert_eq!((transition.vertical, transition.horizontal), (0, 0));
    let cache = renderer.highlighted.as_ref().unwrap();
    assert_eq!(cache.key.mode, DiffViewMode::SideBySide);
    assert!(cache.inline.is_empty());
    assert!(!cache.side_by_side.is_empty());
}

#[test]
fn reuses_a_prepared_buffer_after_visiting_another_file() {
    let mut renderer = Renderer::new();
    let mut model = model();
    model.snapshot.files.push(FileState {
        path: PathBuf::from("src/second.rs"),
        old_path: None,
        kind: ChangeKind::Modified,
        staged: None,
        unstaged: Some(FileDiff {
            text: "@@ -1 +1 @@\n-let second = 1;\n+let second = 2;\n".to_owned(),
        }),
    });

    diff_lines(&mut renderer, &model, 0);
    assert_eq!(renderer.highlight_computations, 1);
    model.select_next();
    diff_lines(&mut renderer, &model, 0);
    assert_eq!(renderer.highlight_computations, 2);
    model.select_previous();
    diff_lines(&mut renderer, &model, 0);

    assert_eq!(renderer.highlight_computations, 2);
    assert_eq!(
        renderer.highlighted.as_ref().unwrap().key.file.path,
        PathBuf::from("src/main.rs")
    );
}

#[test]
fn syntax_highlighting_uses_a_strict_ten_thousand_file_line_limit() {
    let below_limit = diffo_diff::parse_unified_patch(
        "@@ -9999 +9999 @@\n-pub const VALUE: usize = 1;\n+pub const VALUE: usize = 2;\n",
    )
    .unwrap();
    let at_limit = diffo_diff::parse_unified_patch(
        "@@ -10000 +10000 @@\n-pub const VALUE: usize = 1;\n+pub const VALUE: usize = 2;\n",
    )
    .unwrap();

    assert_eq!(diff_file_lines(&below_limit), 9_999);
    assert!(should_syntax_highlight(&below_limit));
    assert_eq!(diff_file_lines(&at_limit), 10_000);
    assert!(!should_syntax_highlight(&at_limit));
}

#[test]
fn initial_highlighting_is_bounded_around_the_first_change() {
    let mut model = model();
    let mut patch = String::from("@@ -1,9999 +1,9999 @@\n");
    for line in 1..=9_999 {
        if line == 9_000 {
            writeln!(patch, "-pub const OLD_TARGET: usize = 1;").unwrap();
            writeln!(patch, "+pub const NEW_TARGET: usize = 2;").unwrap();
        } else {
            writeln!(patch, " pub const LINE_{line}: usize = {line};").unwrap();
        }
    }
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch;
    let mut renderer = Renderer::new();

    diff_lines(&mut renderer, &model, 0);

    let cache = renderer.highlighted.as_ref().unwrap();
    assert!(
        cache
            .highlighted_old_coverage
            .is_some_and(|range| range.contains(9_000))
    );
    assert!(
        cache
            .highlighted_new_coverage
            .is_some_and(|range| range.contains(9_000))
    );
    assert!(cache.highlighted_lines_processed < 800);
    assert!(!cache.highlighted.new.contains_key(&1));
    assert!(cache.highlighted.new.contains_key(&9_000));
}

#[test]
fn lifts_low_contrast_theme_colors_on_diff_backgrounds() {
    let monokai_comment = Rgb {
        red: 117,
        green: 113,
        blue: 94,
    };
    for kind in [RowKind::Removed, RowKind::Added] {
        let adjusted = contrasting_foreground(monokai_comment, kind);
        let background = diff_background_rgb(kind).expect("changed row has a background");

        assert!(contrast_ratio(adjusted, background) >= 4.5);
    }
    assert_eq!(
        contrasting_foreground(monokai_comment, RowKind::Context),
        monokai_comment
    );
}

#[test]
#[ignore = "manual performance measurement"]
fn measures_large_diff_rendering() {
    let mut model = model();
    let mut patch = String::from("@@ -0,0 +1,100000 @@\n");
    for index in 0..100_000 {
        writeln!(patch, "+pub const ITEM_{index}: usize = {index};").unwrap();
    }
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch;
    let mut renderer = Renderer::new();

    let started = Instant::now();
    renderer.prepare_frame(&model, Rect::new(0, 0, 180, 60));
    let loading = renderer.diff_lines(&model, 160, 0, 50);
    let enqueue = started.elapsed();
    assert!(loading.is_empty());
    let started = Instant::now();
    let lines = loop {
        renderer.prepare_frame(&model, Rect::new(0, 0, 180, 60));
        let lines = renderer.diff_lines(&model, 160, 0, 50);
        if !renderer.is_preparing() {
            break lines;
        }
        sleep(Duration::from_millis(1));
    };
    let prepared = started.elapsed();
    let started = Instant::now();
    for row in (0..10_000).step_by(50) {
        assert_eq!(renderer.diff_lines(&model, 160, row, 50).len(), 50);
    }
    let cached = started.elapsed();

    eprintln!(
        "100k enqueue={enqueue:?} background_prepare={prepared:?} cached_200_viewports={cached:?}"
    );
    assert_eq!(lines.len(), 50);
    assert_eq!(renderer.highlight_computations, 0);
}

#[test]
#[ignore = "manual file-open performance measurement"]
fn measures_bounded_9999_line_file_open() {
    let mut model = model();
    let mut patch = String::from("@@ -1,9999 +1,9999 @@\n");
    for line in 1..=9_999 {
        if line == 9_000 {
            writeln!(patch, "-pub const OLD_TARGET: usize = 1;").unwrap();
            writeln!(patch, "+pub const PERF_TARGET_09000: usize = 2;").unwrap();
        } else {
            writeln!(patch, " pub const LINE_{line}: usize = {line};").unwrap();
        }
    }
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch;
    let mut renderer = Renderer::new();
    let area = Rect::new(0, 0, 100, 30);
    let started = Instant::now();
    let transition = loop {
        let preparation = renderer.prepare_frame(&model, area);
        if let Some(transition) = preparation.viewport_transition {
            break transition;
        }
        sleep(Duration::from_millis(1));
    };
    let elapsed = started.elapsed();
    let cache = renderer.highlighted.as_ref().unwrap();

    eprintln!("bounded 9,999-line open={elapsed:?}");
    assert!(transition.vertical > 8_900);
    assert!(cache.highlighted_lines_processed < 800);
}
