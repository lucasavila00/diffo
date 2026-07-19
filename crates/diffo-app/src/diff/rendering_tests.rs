use std::fmt::Write;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;
use std::time::Instant;

use crate::diff::{ChangeArea, DiffViewMode, Message, Model, Toast, ToastKind, ToastQueue};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use diffo_core::{ChangeKind, FileDiff, FileState, HeadState, RepositorySnapshot, UpstreamState};
use diffo_diff::RowKind;
use diffo_highlight::Rgb;
use diffo_ui::tool_areas;
use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
};

use super::{
    Renderer, RendererEvent, contrast_ratio, contrasting_foreground, diff_background,
    diff_background_rgb, diff_file_lines, file_label, highlight_prefetch_viewports,
    horizontal_panes, main_area, overview_position, picker_document, render_status, row_style,
    scrollbar_position_count, should_syntax_highlight, status_line,
};

#[test]
fn file_picker_renders_every_git_change_kind_with_its_status_color() {
    let kinds = [
        (ChangeKind::Added, Color::LightGreen),
        (ChangeKind::Modified, Color::Yellow),
        (ChangeKind::Deleted, Color::LightRed),
        (ChangeKind::Renamed, Color::LightCyan),
        (ChangeKind::Copied, Color::LightCyan),
        (ChangeKind::Untracked, Color::LightGreen),
        (ChangeKind::Conflicted, Color::LightRed),
    ];
    let files = kinds
        .iter()
        .enumerate()
        .map(|(index, (kind, _))| FileState {
            path: PathBuf::from(format!("file-{index}.rs")),
            old_path: None,
            kind: *kind,
            staged: None,
            unstaged: Some(FileDiff {
                text: String::new(),
            }),
        })
        .collect::<Vec<_>>();
    let mut picker = diffo_ui::file_picker::FilePicker::default();
    picker.prepare(
        Rect::new(0, 0, 40, 10),
        picker_document(
            "Changes",
            " Stage All",
            files.iter(),
            ChangeArea::Unstaged,
            Style::default(),
        ),
        None,
    );
    let backend = TestBackend::new(40, 10);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal.draw(|frame| picker.render(frame, false)).unwrap();

    insta::assert_debug_snapshot!(terminal.backend().buffer());
}

#[test]
fn diff_buffer_title_matches_the_committed_picker_label() {
    let model = model();
    let area = Rect::new(0, 0, 100, 30);
    let mut renderer = Renderer::new();
    renderer.prepare_frame(&model, area);

    let picker_label = file_label(&model.snapshot.files[0]);
    assert_eq!(renderer.displayed_key().unwrap().title, picker_label);

    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| renderer.render(frame, &model))
        .unwrap();

    let diff = horizontal_panes(main_area(area), model.file_pane_percent)[1];
    insta::assert_debug_snapshot!(buffer_region(
        terminal.backend().buffer(),
        Rect::new(diff.x, diff.y, 30, 1),
    ));
}

#[test]
fn diff_file_icon_inherits_status_style_in_a_narrow_row() {
    let files = [FileState {
        path: PathBuf::from("very-long-name.rs"),
        old_path: None,
        kind: ChangeKind::Modified,
        staged: None,
        unstaged: Some(FileDiff {
            text: String::new(),
        }),
    }];
    let mut picker = diffo_ui::file_picker::FilePicker::default();
    picker.prepare(
        Rect::new(0, 0, 15, 4),
        picker_document(
            "Changes",
            " Stage All",
            files.iter(),
            ChangeArea::Unstaged,
            Style::default(),
        ),
        None,
    );
    let backend = TestBackend::new(15, 4);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal.draw(|frame| picker.render(frame, false)).unwrap();

    insta::assert_debug_snapshot!(terminal.backend().buffer());
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

fn assert_jump_event(
    renderer: &mut Renderer,
    event: &Event,
    model: &Model,
    area: Rect,
    target: usize,
) {
    assert_eq!(
        renderer.map_event(event, model, area),
        Some(RendererEvent::Message(Message::JumpDiffToPosition(target)))
    );
}

fn viewport_control_bottom(viewport: crate::diff::DiffViewportMetrics) -> u16 {
    viewport
        .content_area
        .bottom()
        .saturating_add(viewport.horizontal_area.height)
}

fn buffer_region(buffer: &Buffer, area: Rect) -> Buffer {
    let mut region = Buffer::empty(Rect::new(0, 0, area.width, area.height));
    for y in 0..area.height {
        for x in 0..area.width {
            region[(x, y)] = buffer[(area.x + x, area.y + y)].clone();
        }
    }
    region
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

    insta::assert_debug_snapshot!("clean_with_divergence", status_line(&model, 0, 80));

    model.snapshot.files.push(FileState {
        path: PathBuf::from("changed.rs"),
        old_path: None,
        kind: ChangeKind::Modified,
        staged: None,
        unstaged: Some(FileDiff {
            text: "@@ -1 +1 @@\n-old\n+new\n".to_owned(),
        }),
    });
    insta::assert_debug_snapshot!("unstaged_changes", status_line(&model, 0, 80));

    model.snapshot.files[0].staged = model.snapshot.files[0].unstaged.clone();
    insta::assert_debug_snapshot!("staged_changes", status_line(&model, 0, 80));

    model.snapshot.files[0].kind = ChangeKind::Conflicted;
    insta::assert_debug_snapshot!("conflicts", status_line(&model, 0, 80));
}

#[test]
fn status_line_distinguishes_unborn_and_detached_head() {
    let mut model = Model::new(RepositorySnapshot {
        head: HeadState::Unborn {
            name: "main".to_owned(),
        },
        ..RepositorySnapshot::default()
    });
    insta::assert_debug_snapshot!("unborn", status_line(&model, 0, 80));

    model.snapshot.head = HeadState::Detached {
        commit: "123456789abcdef".to_owned(),
    };
    insta::assert_debug_snapshot!("detached", status_line(&model, 0, 80));
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
    insta::assert_debug_snapshot!("unicode_truncation", line);

    let minimum = status_line(&model, 0, 1);
    assert_eq!(minimum.width(), 1);
    insta::assert_debug_snapshot!("minimum_width", minimum);
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
    assert_eq!(line.width(), 40);
    insta::assert_debug_snapshot!(line);
}

#[test]
fn status_line_makes_error_control_characters_inert() {
    let mut model = Model::new(RepositorySnapshot {
        head: HeadState::Named {
            name: "main".to_owned(),
            commit: "123456789abcdef".to_owned(),
        },
        ..RepositorySnapshot::default()
    });
    model.error = Some("Sync failed\ncontinue?\r\x1b[2J\t\x08".to_owned());

    let line = status_line(&model, 0, 80);
    let text = line_text(&line);

    assert!(line.width() <= 80);
    assert!(!text.chars().any(char::is_control));
    insta::assert_debug_snapshot!(line);
}

#[test]
fn rendered_footer_keeps_newline_errors_on_the_footer_row() {
    let mut model = Model::new(RepositorySnapshot {
        head: HeadState::Named {
            name: "main".to_owned(),
            commit: "123456789abcdef".to_owned(),
        },
        ..RepositorySnapshot::default()
    });
    model.error = Some("Fetch failed\nSSH host is unknown".to_owned());
    let area = Rect::new(0, 0, 80, 12);
    let mut renderer = Renderer::new();
    renderer.prepare_frame(&model, area);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            renderer.render(frame, &model);
            render_status(frame, tool_areas(area).status, &model);
        })
        .unwrap();

    insta::assert_debug_snapshot!(terminal.backend().buffer());
}

#[test]
fn file_list_scrollbars_have_independent_offsets_and_exact_hit_targets() {
    let model = file_list_model(30);
    let area = Rect::new(0, 0, 100, 30);
    let mut renderer = Renderer::new();
    renderer.prepare_frame(&model, area);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| renderer.render(frame, &model))
        .unwrap();

    for _ in 0..3 {
        let wheel = mouse_at(
            MouseEventKind::ScrollDown,
            renderer.staged_picker.metrics().list_area,
        );
        assert_eq!(
            renderer.map_event(&wheel, &model, area),
            Some(RendererEvent::Consumed)
        );
    }
    for _ in 0..7 {
        let wheel = mouse_at(
            MouseEventKind::ScrollDown,
            renderer.unstaged_picker.metrics().list_area,
        );
        assert_eq!(
            renderer.map_event(&wheel, &model, area),
            Some(RendererEvent::Consumed)
        );
    }
    assert_eq!(renderer.staged_picker.metrics().offset, 3);
    assert_eq!(renderer.unstaged_picker.metrics().offset, 7);

    let unstaged = renderer.unstaged_picker.metrics();
    let row_click = mouse_at(MouseEventKind::Down(MouseButton::Left), unstaged.list_area);
    assert_eq!(
        renderer.map_event(&row_click, &model, area),
        Some(RendererEvent::Message(Message::SelectFile(
            crate::diff::FileKey {
                path: PathBuf::from("generated/file-007.rs"),
                area: ChangeArea::Unstaged,
            },
        )))
    );

    let mut action_area = unstaged.list_area;
    action_area.x = action_area.right().saturating_sub(1);
    action_area.width = 1;
    let action_click = mouse_at(MouseEventKind::Down(MouseButton::Left), action_area);
    assert_eq!(
        renderer.map_event(&action_click, &model, area),
        Some(RendererEvent::Message(Message::StageFile(PathBuf::from(
            "generated/file-007.rs",
        ))))
    );

    let mut bottom = unstaged.scrollbar_area;
    bottom.y = bottom.bottom().saturating_sub(1);
    bottom.height = 1;
    let scrollbar_click = mouse_at(MouseEventKind::Down(MouseButton::Left), bottom);
    assert_eq!(
        renderer.map_event(&scrollbar_click, &model, area),
        Some(RendererEvent::Consumed)
    );
    assert_eq!(
        renderer.unstaged_picker.metrics().offset,
        unstaged.maximum_offset
    );

    let drag_to_top = mouse_at(
        MouseEventKind::Drag(MouseButton::Left),
        unstaged.scrollbar_area,
    );
    assert_eq!(
        renderer.map_event(&drag_to_top, &model, area),
        Some(RendererEvent::Consumed)
    );
    assert_eq!(renderer.unstaged_picker.metrics().offset, 0);
}

#[test]
fn file_list_scrollbars_hide_without_overflow_and_offsets_clamp() {
    let model = file_list_model(1);
    let area = Rect::new(0, 0, 100, 30);
    let mut renderer = Renderer::new();
    renderer.prepare_frame(&model, area);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| renderer.render(frame, &model))
        .unwrap();

    assert_eq!(renderer.staged_picker.metrics().maximum_offset, 0);
    assert_eq!(renderer.unstaged_picker.metrics().maximum_offset, 0);
    assert!(renderer.staged_picker.metrics().scrollbar_area.is_empty());
    assert!(renderer.unstaged_picker.metrics().scrollbar_area.is_empty());
}

#[test]
fn diff_file_picker_uses_the_shared_path_menu() {
    let model = file_list_model(1);
    let area = Rect::new(0, 0, 100, 30);
    let mut renderer = Renderer::new();
    renderer.prepare_frame(&model, area);
    let list = renderer.unstaged_picker.metrics().list_area;
    let right_click = mouse_at(MouseEventKind::Down(MouseButton::Right), list);

    assert_eq!(
        renderer.map_event(&right_click, &model, area),
        Some(RendererEvent::Message(Message::SelectFile(
            crate::diff::FileKey {
                path: PathBuf::from("generated/file-000.rs"),
                area: ChangeArea::Unstaged,
            },
        )))
    );
    assert!(renderer.has_open_picker_menu());

    let copy_absolute = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: list.x.saturating_add(1),
        row: list.y.saturating_add(1),
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(
        renderer.map_event(&copy_absolute, &model, area),
        Some(RendererEvent::CopyPath {
            path: PathBuf::from("generated/file-000.rs"),
            absolute: true,
        })
    );

    let shortcut = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
    assert_eq!(
        renderer.map_event(&shortcut, &model, area),
        Some(RendererEvent::Consumed)
    );
    assert!(renderer.has_open_picker_menu());
}

#[test]
fn diff_navigation_hands_off_between_flat_picker_instances() {
    let mut model = file_list_model(2);
    let area = Rect::new(0, 0, 100, 30);
    let mut renderer = Renderer::new();
    renderer.prepare_frame(&model, area);
    let previous = Event::Key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));

    assert_eq!(
        renderer.map_event(&previous, &model, area),
        Some(RendererEvent::Message(Message::SelectFile(
            crate::diff::FileKey {
                path: PathBuf::from("generated/file-001.rs"),
                area: ChangeArea::Staged,
            },
        )))
    );

    model.select_file(&crate::diff::FileKey {
        path: PathBuf::from("generated/file-001.rs"),
        area: ChangeArea::Staged,
    });
    renderer.prepare_frame(&model, area);
    let next = Event::Key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
    assert_eq!(
        renderer.map_event(&next, &model, area),
        Some(RendererEvent::Message(Message::SelectFile(
            crate::diff::FileKey {
                path: PathBuf::from("generated/file-000.rs"),
                area: ChangeArea::Unstaged,
            },
        )))
    );
}

#[test]
fn change_navigation_stops_at_the_first_and_last_changes() {
    let mut model = model();
    let mut patch = String::from("@@ -1,60 +1,60 @@\n");
    for line in 0..60 {
        if matches!(line, 5 | 30 | 55) {
            writeln!(patch, "-old {line}").unwrap();
            writeln!(patch, "+new {line}").unwrap();
        } else {
            writeln!(patch, " context {line}").unwrap();
        }
    }
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch;
    let mut renderer = Renderer::new();
    let area = Rect::new(0, 0, 100, 15);
    renderer.prepare_frame(&model, area);
    let changes = renderer
        .highlighted
        .as_ref()
        .unwrap()
        .inline_changes
        .clone();

    model.diff_scroll = changes[0].first;
    let second = renderer
        .change_jump(&model, area, true)
        .expect("second change");
    model.diff_scroll = second;
    let third = renderer
        .change_jump(&model, area, true)
        .expect("third change");
    assert!(third > second);
    model.diff_scroll = third;
    assert_eq!(renderer.change_jump(&model, area, true), None);
    assert_eq!(renderer.change_jump(&model, area, false), Some(second));
    model.diff_scroll = changes[0].first;
    assert_eq!(renderer.change_jump(&model, area, false), None);
}

mod chrome;
mod diff;
