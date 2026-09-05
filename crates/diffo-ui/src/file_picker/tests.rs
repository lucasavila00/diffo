use super::*;
use crossterm::event::{MouseEvent, MouseEventKind};
use ratatui::{
    Terminal,
    backend::TestBackend,
    style::{Color, Modifier},
};

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

fn rows(count: usize) -> Vec<Row<usize>> {
    (0..count)
        .map(|id| Row::flat(id, Line::raw(format!("file-{id}"))))
        .collect()
}

#[test]
fn flat_navigation_and_scrollbar_use_one_contract() {
    let mut picker = FilePicker::default();
    picker.prepare(
        Rect::new(2, 3, 20, 6),
        Document::flat("Files", rows(8)),
        None,
    );
    assert_eq!(picker.selected(), Some(&0));
    assert_eq!(
        picker.navigate(Navigation::Next),
        Some(Outcome::Selected(1))
    );
    assert_eq!(picker.metrics().maximum_offset, 4);
    assert_eq!(
        picker.navigate(Navigation::Last),
        Some(Outcome::Selected(7))
    );
    assert_eq!(picker.metrics().offset, 4);

    let bar = picker.metrics().scrollbar_area;
    assert_eq!(
        picker.handle_event(
            &mouse(
                MouseEventKind::Down(MouseButton::Left),
                bar.x,
                bar.bottom() - 1,
            ),
            Rect::new(0, 0, 80, 24),
        ),
        Some(Outcome::Consumed)
    );
    assert_eq!(picker.metrics().offset, 4);

    picker.prepare(
        Rect::new(2, 3, 20, 6),
        Document::flat("Files", rows(2)),
        None,
    );
    assert_eq!(picker.selected(), Some(&1));
    assert_eq!(picker.metrics().offset, 0);
}

#[test]
fn flat_and_tree_wheel_use_the_same_scroll_core() {
    let area = Rect::new(2, 3, 20, 6);
    let mut flat = FilePicker::default();
    flat.prepare(area, Document::flat("Files", rows(8)), None);
    let mut tree = FilePicker::default();
    tree.prepare(
        area,
        Document::tree(
            "Explorer",
            (0..8)
                .map(|id| TreeNode::leaf(id, Line::raw(format!("file-{id}"))))
                .collect(),
        ),
        None,
    );
    let down = mouse(MouseEventKind::ScrollDown, area.x + 1, area.y + 1);

    for _ in 0..3 {
        assert_eq!(
            flat.handle_event(&down, Rect::new(0, 0, 80, 24)),
            Some(Outcome::Consumed)
        );
        assert_eq!(
            tree.handle_event(&down, Rect::new(0, 0, 80, 24)),
            Some(Outcome::Consumed)
        );
    }

    assert_eq!(flat.metrics().offset, 3);
    assert_eq!(tree.metrics().offset, flat.metrics().offset);

    flat.prepare(area, Document::flat("Files", rows(8)), None);
    tree.prepare(
        area,
        Document::tree(
            "Explorer",
            (0..8)
                .map(|id| TreeNode::leaf(id, Line::raw(format!("file-{id}"))))
                .collect(),
        ),
        None,
    );
    assert_eq!(flat.metrics().offset, 3);
    assert_eq!(tree.metrics().offset, 3);
}

#[test]
fn rendering_preserves_label_style_and_owns_the_action_style() {
    let label_style = Style::default()
        .fg(Color::Red)
        .add_modifier(Modifier::CROSSED_OUT);
    let mut picker = FilePicker::default();
    picker.prepare(
        Rect::new(0, 0, 30, 4),
        Document::flat(
            "Files",
            vec![
                Row::flat(0, Line::styled("D deleted.txt", label_style))
                    .with_action(crate::icons::ADD),
            ],
        ),
        None,
    );
    let backend = TestBackend::new(30, 4);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal.draw(|frame| picker.render(frame, true)).unwrap();

    insta::assert_debug_snapshot!("styled_selected_row", terminal.backend().buffer());
}

#[test]
fn panel_actions_are_high_contrast_and_distinct_from_borders() {
    let mut flat_document = Document::flat("Changes", Vec::<Row<usize>>::new());
    flat_document.panel_action = Some(format!("{} Stage All", crate::icons::ADD));
    let mut flat = FilePicker::default();
    flat.prepare(Rect::new(0, 0, 30, 4), flat_document, None);
    let backend = TestBackend::new(30, 4);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| flat.render(frame, false)).unwrap();

    insta::assert_debug_snapshot!("flat_panel_action", terminal.backend().buffer());

    let mut tree = FilePicker::default();
    tree.prepare(
        Rect::new(0, 0, 30, 4),
        Document::tree("Explorer", Vec::<TreeNode<usize>>::new()),
        None,
    );
    terminal.draw(|frame| tree.render(frame, false)).unwrap();

    insta::assert_debug_snapshot!("tree_panel_actions", terminal.backend().buffer());
}

#[test]
fn tree_expansion_is_the_only_projection_difference() {
    let mut picker = FilePicker::default();
    let nodes = vec![
        TreeNode::leaf(0, Line::raw("README")),
        TreeNode::branch(
            1,
            Line::raw("src"),
            vec![TreeNode::branch(
                2,
                Line::raw("nested"),
                vec![TreeNode::leaf(3, Line::raw("main.rs"))],
            )],
        ),
    ];
    picker.prepare(
        Rect::new(0, 0, 20, 8),
        Document::tree("Explorer", nodes),
        None,
    );
    assert_eq!(picker.visible.len(), 2);
    picker.navigate(Navigation::Last);
    picker.navigate(Navigation::Activate);
    assert_eq!(picker.visible.len(), 3);
    picker.expand_all();
    picker.navigate(Navigation::Last);
    assert_eq!(picker.selected(), Some(&3));
    picker.collapse_all();
    assert_eq!(picker.visible.len(), 2);
    assert_eq!(picker.selected(), Some(&1));
    picker.expand_all();
    assert_eq!(picker.visible.len(), 4);
}

#[test]
fn tree_refresh_preserves_expansion_by_stable_node_id() {
    let document = || {
        Document::tree(
            "Explorer",
            vec![TreeNode::branch(
                1,
                Line::raw("src"),
                vec![TreeNode::leaf(2, Line::raw("main.rs"))],
            )],
        )
    };
    let mut picker = FilePicker::default();
    let area = Rect::new(0, 0, 20, 5);
    picker.prepare(area, document(), None);
    picker.navigate(Navigation::Activate);
    assert_eq!(picker.visible_rows(), 2);

    picker.prepare(area, document(), None);

    assert_eq!(picker.visible_rows(), 2);
    assert_eq!(picker.selected(), Some(&1));
}

#[test]
fn tree_branch_context_menu_is_independent_of_disclosure() {
    let mut picker = FilePicker::default();
    picker.prepare(
        Rect::new(0, 0, 20, 5),
        Document::tree(
            "Explorer",
            vec![
                TreeNode::branch(
                    1,
                    Line::raw("src"),
                    vec![TreeNode::leaf(2, Line::raw("main.rs"))],
                )
                .with_context_menu(),
            ],
        ),
        None,
    );

    assert_eq!(picker.visible_rows(), 1);
    assert_eq!(
        picker.handle_event(
            &mouse(MouseEventKind::Down(MouseButton::Right), 1, 1),
            Rect::new(0, 0, 40, 10),
        ),
        Some(Outcome::Selected(1))
    );
    assert!(picker.has_open_menu());
    assert_eq!(picker.visible_rows(), 1);
    assert_eq!(
        picker.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE)),
            Rect::new(0, 0, 40, 10),
        ),
        Some(Outcome::CopyPath {
            id: 1,
            absolute: false,
        })
    );
    assert_eq!(picker.visible_rows(), 1);
}

#[test]
fn flat_rows_start_with_their_label_without_a_dot() {
    let mut picker = FilePicker::default();
    picker.prepare(
        Rect::new(0, 0, 20, 4),
        Document::flat("Files", rows(1)),
        None,
    );
    let backend = TestBackend::new(20, 4);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| picker.render(frame, false)).unwrap();

    insta::assert_debug_snapshot!(terminal.backend().buffer());
}

#[test]
fn tree_rows_keep_their_structure_when_selected() {
    let mut picker = FilePicker::default();
    picker.prepare(
        Rect::new(0, 0, 20, 5),
        Document::tree(
            "Explorer",
            vec![
                TreeNode::branch(0, Line::raw("src"), Vec::new()),
                TreeNode::leaf(1, Line::raw("README")),
            ],
        ),
        None,
    );
    let backend = TestBackend::new(20, 5);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| picker.render(frame, false)).unwrap();

    insta::assert_debug_snapshot!("unfocused", terminal.backend().buffer());

    terminal.draw(|frame| picker.render(frame, true)).unwrap();

    insta::assert_debug_snapshot!("focused", terminal.backend().buffer());
}

#[test]
fn tree_caret_tracks_expansion_and_the_folder_icon_stays_fixed() {
    let mut picker = FilePicker::default();
    picker.prepare(
        Rect::new(0, 0, 20, 5),
        Document::tree(
            "Explorer",
            vec![TreeNode::branch(
                0,
                Line::raw("src"),
                vec![TreeNode::leaf(1, Line::raw("main.rs"))],
            )],
        ),
        None,
    );
    let backend = TestBackend::new(20, 5);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal.draw(|frame| picker.render(frame, false)).unwrap();
    insta::assert_debug_snapshot!("collapsed", terminal.backend().buffer());

    picker.navigate(Navigation::Activate);
    terminal.draw(|frame| picker.render(frame, false)).unwrap();
    insta::assert_debug_snapshot!("expanded", terminal.backend().buffer());
}

#[test]
fn long_labels_use_three_dots_without_hiding_row_actions() {
    let label_style = Style::default().fg(Color::Yellow);
    let mut flat = FilePicker::default();
    flat.prepare(
        Rect::new(0, 0, 18, 4),
        Document::flat(
            "Files",
            vec![
                Row::flat(0, Line::styled("very-long-file-name.rs", label_style))
                    .with_action(crate::icons::ADD),
            ],
        ),
        None,
    );
    let backend = TestBackend::new(18, 4);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| flat.render(frame, false)).unwrap();

    insta::assert_debug_snapshot!("flat_truncation", terminal.backend().buffer());

    let mut tree = FilePicker::default();
    tree.prepare(
        Rect::new(0, 0, 18, 4),
        Document::tree(
            "Explorer",
            vec![TreeNode::leaf(0, Line::raw("very-long-tree-file-name.rs"))],
        ),
        None,
    );
    terminal.draw(|frame| tree.render(frame, false)).unwrap();

    insta::assert_debug_snapshot!("tree_truncation", terminal.backend().buffer());
}

#[test]
fn empty_narrow_picker_has_no_phantom_target() {
    let mut picker = FilePicker::<usize>::default();
    picker.prepare(
        Rect::new(0, 0, 1, 1),
        Document::flat("Files", Vec::new()),
        None,
    );

    assert_eq!(picker.selected(), None);
    assert!(picker.metrics().list_area.is_empty());
    assert_eq!(picker.navigate(Navigation::Next), None);
    assert_eq!(
        picker.handle_event(
            &mouse(MouseEventKind::Down(MouseButton::Right), 0, 0),
            Rect::new(0, 0, 1, 1),
        ),
        None
    );
}

fn prepared_picker() -> FilePicker<usize> {
    let mut picker = FilePicker::default();
    picker.prepare(
        Rect::new(0, 0, 20, 5),
        Document::flat("Files", rows(1)),
        None,
    );
    picker
}

fn open_menu_with_key(picker: &mut FilePicker<usize>) {
    assert_eq!(
        picker.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)),
            Rect::new(0, 0, 40, 10),
        ),
        Some(Outcome::Consumed)
    );
    assert!(picker.has_open_menu());
}

#[test]
fn context_menu_renders_spacing_and_shortcuts() {
    let mut picker = prepared_picker();
    assert_eq!(
        picker.handle_event(
            &mouse(MouseEventKind::Down(MouseButton::Right), 2, 1),
            Rect::new(0, 0, 40, 10),
        ),
        Some(Outcome::Selected(0))
    );

    let backend = TestBackend::new(30, 8);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            picker.render(frame, true);
            picker.render_menu(frame);
        })
        .unwrap();

    insta::assert_debug_snapshot!(terminal.backend().buffer());
}

#[test]
fn context_menu_mouse_actions_have_an_inert_row_between_them() {
    let mut picker = prepared_picker();
    open_menu_with_key(&mut picker);
    assert_eq!(
        picker.handle_event(
            &mouse(MouseEventKind::Down(MouseButton::Left), 3, 2),
            Rect::new(0, 0, 40, 10),
        ),
        Some(Outcome::CopyPath {
            id: 0,
            absolute: true,
        })
    );

    open_menu_with_key(&mut picker);
    assert_eq!(
        picker.handle_event(
            &mouse(MouseEventKind::Down(MouseButton::Left), 3, 3),
            Rect::new(0, 0, 40, 10),
        ),
        Some(Outcome::Consumed)
    );
    assert!(!picker.has_open_menu());

    open_menu_with_key(&mut picker);
    assert_eq!(
        picker.handle_event(
            &mouse(MouseEventKind::Down(MouseButton::Left), 3, 4),
            Rect::new(0, 0, 40, 10),
        ),
        Some(Outcome::CopyPath {
            id: 0,
            absolute: false,
        })
    );
}

#[test]
fn context_menu_action_shortcuts_are_lowercase() {
    let mut picker = prepared_picker();
    open_menu_with_key(&mut picker);
    assert_eq!(
        picker.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)),
            Rect::new(0, 0, 40, 10),
        ),
        Some(Outcome::CopyPath {
            id: 0,
            absolute: true,
        })
    );

    open_menu_with_key(&mut picker);
    assert_eq!(
        picker.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE)),
            Rect::new(0, 0, 40, 10),
        ),
        Some(Outcome::CopyPath {
            id: 0,
            absolute: false,
        })
    );

    open_menu_with_key(&mut picker);
    for character in ['A', 'R', 'C'] {
        assert_eq!(
            picker.handle_event(
                &Event::Key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE,)),
                Rect::new(0, 0, 40, 10),
            ),
            None
        );
        assert!(picker.has_open_menu());
    }
}

#[test]
fn context_menu_open_key_toggles_and_escape_closes() {
    let mut picker = prepared_picker();

    open_menu_with_key(&mut picker);
    assert_eq!(
        picker.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)),
            Rect::new(0, 0, 40, 10),
        ),
        Some(Outcome::Consumed)
    );
    assert!(!picker.has_open_menu());

    open_menu_with_key(&mut picker);
    assert_eq!(
        picker.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            Rect::new(0, 0, 40, 10),
        ),
        Some(Outcome::Consumed)
    );
    assert!(!picker.has_open_menu());
}

#[test]
fn uppercase_shortcuts_are_rejected() {
    for character in ['J', 'W', 'K', 'L', 'S', 'G', 'C'] {
        assert_eq!(
            navigation(&KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE)),
            None
        );
    }
}

#[test]
fn first_and_last_file_shortcuts_are_rejected() {
    for code in [KeyCode::Home, KeyCode::End, KeyCode::Char('g')] {
        assert_eq!(navigation(&KeyEvent::new(code, KeyModifiers::NONE)), None);
    }
}
