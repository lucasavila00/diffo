use super::*;
use crossterm::event::{MouseEvent, MouseEventKind};
use ratatui::{Terminal, backend::TestBackend, style::Color};

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

fn rendered_row(buffer: &ratatui::buffer::Buffer, area: Rect) -> String {
    (area.x..area.right())
        .map(|column| buffer[(column, area.y)].symbol())
        .collect()
}

fn assert_enabled_control(cell: &ratatui::buffer::Cell) {
    assert_eq!(cell.fg, theme::TEXT);
    assert_ne!(cell.fg, theme::CHROME);
    assert!(cell.modifier.contains(Modifier::BOLD));
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
                .map(|id| Row::tree(id, Line::raw(format!("file-{id}")), 0, false))
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
                .map(|id| Row::tree(id, Line::raw(format!("file-{id}")), 0, false))
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
        .fg(Color::LightRed)
        .add_modifier(Modifier::CROSSED_OUT);
    let mut picker = FilePicker::default();
    picker.prepare(
        Rect::new(0, 0, 30, 4),
        Document::flat(
            "Files",
            vec![Row::flat(0, Line::styled("D  deleted.txt", label_style)).with_action("[+]")],
        ),
        None,
    );
    let backend = TestBackend::new(30, 4);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal.draw(|frame| picker.render(frame, true)).unwrap();

    let buffer = terminal.backend().buffer();
    let marker = &buffer[(3, 1)];
    assert_eq!(marker.symbol(), "D");
    assert_eq!(marker.fg, Color::LightRed);
    let filename = &buffer[(6, 1)];
    assert_eq!(filename.symbol(), "d");
    assert_eq!(filename.fg, Color::LightRed);
    assert_eq!(filename.bg, theme::SELECTION_BACKGROUND);
    assert!(filename.modifier.contains(Modifier::CROSSED_OUT));
    assert!(filename.modifier.contains(Modifier::BOLD));

    let action = &buffer[(26, 1)];
    assert_eq!(action.symbol(), "[");
    assert_enabled_control(action);
    assert_eq!(action.bg, theme::SELECTION_BACKGROUND);
    let selection = &buffer[(1, 1)];
    assert_eq!(selection.symbol(), "·");
    assert_enabled_control(selection);
    assert_eq!(selection.bg, theme::SELECTION_BACKGROUND);
}

#[test]
fn panel_actions_are_high_contrast_and_distinct_from_borders() {
    let mut flat_document = Document::flat("Changes", Vec::<Row<usize>>::new());
    flat_document.panel_action = Some("[+] Stage All".to_owned());
    let mut flat = FilePicker::default();
    flat.prepare(Rect::new(0, 0, 30, 4), flat_document, None);
    let backend = TestBackend::new(30, 4);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| flat.render(frame, false)).unwrap();

    let buffer = terminal.backend().buffer();
    let plus = buffer
        .content
        .iter()
        .find(|cell| cell.symbol() == "+")
        .expect("flat panel action");
    assert_enabled_control(plus);
    assert_eq!(buffer[(0, 0)].fg, theme::CHROME);

    let mut tree = FilePicker::default();
    tree.prepare(
        Rect::new(0, 0, 30, 4),
        Document::tree("Explorer", Vec::<Row<usize>>::new()),
        None,
    );
    terminal.draw(|frame| tree.render(frame, false)).unwrap();

    let buffer = terminal.backend().buffer();
    for symbol in ["-", "+"] {
        let control = buffer
            .content
            .iter()
            .find(|cell| cell.symbol() == symbol)
            .unwrap_or_else(|| panic!("tree {symbol} action"));
        assert_enabled_control(control);
    }
    assert_eq!(buffer[(0, 0)].fg, theme::CHROME);
}

#[test]
fn tree_expansion_is_the_only_projection_difference() {
    let mut picker = FilePicker::default();
    let rows = vec![
        Row::tree(0, Line::raw("README"), 0, false),
        Row::tree(1, Line::raw("src"), 0, true),
        Row::tree(2, Line::raw("nested"), 1, true),
        Row::tree(3, Line::raw("main.rs"), 2, false),
    ];
    picker.prepare(
        Rect::new(0, 0, 20, 8),
        Document::tree("Explorer", rows),
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
fn every_unselected_flat_row_has_a_persistent_click_marker() {
    let mut picker = FilePicker::default();
    picker.prepare(
        Rect::new(0, 0, 20, 4),
        Document::flat("Files", rows(1)),
        None,
    );
    let backend = TestBackend::new(20, 4);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| picker.render(frame, false)).unwrap();

    let marker = &terminal.backend().buffer()[(1, 1)];
    assert_eq!(marker.symbol(), "·");
    assert_enabled_control(marker);
}

#[test]
fn tree_rows_keep_their_structure_when_selected() {
    let mut picker = FilePicker::default();
    picker.prepare(
        Rect::new(0, 0, 20, 5),
        Document::tree(
            "Explorer",
            vec![
                Row::tree(0, Line::raw("src"), 0, true),
                Row::tree(1, Line::raw("README"), 0, false),
            ],
        ),
        None,
    );
    let backend = TestBackend::new(20, 5);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| picker.render(frame, false)).unwrap();

    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(1, 1)].symbol(), " ");
    assert_eq!(buffer[(3, 1)].symbol(), "▸");
    assert!(!rendered_row(buffer, Rect::new(1, 1, 18, 1)).contains('·'));
    assert!(!rendered_row(buffer, Rect::new(1, 2, 18, 1)).contains('·'));

    terminal.draw(|frame| picker.render(frame, true)).unwrap();

    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(1, 1)].symbol(), " ");
    assert_eq!(buffer[(3, 1)].symbol(), "▸");
    assert!(!rendered_row(buffer, Rect::new(1, 1, 18, 1)).contains('›'));
    let selected_label = &buffer[(5, 1)];
    assert_eq!(selected_label.symbol(), "s");
    assert_eq!(selected_label.bg, theme::SELECTION_BACKGROUND);
    assert!(selected_label.modifier.contains(Modifier::BOLD));
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
                    .with_action("[+]"),
            ],
        ),
        None,
    );
    let backend = TestBackend::new(18, 4);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| flat.render(frame, false)).unwrap();

    let row = rendered_row(terminal.backend().buffer(), flat.metrics().list_area);
    assert!(row.contains("..."), "{row:?}");
    assert!(row.ends_with("[+]"), "{row:?}");
    let first_dot = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .find(|cell| cell.symbol() == ".")
        .expect("truncation dots");
    assert_eq!(first_dot.fg, Color::Yellow);
    let action = &terminal.backend().buffer()[(
        flat.metrics().list_area.right().saturating_sub(3),
        flat.metrics().list_area.y,
    )];
    assert_eq!(action.symbol(), "[");
    assert_enabled_control(action);

    let mut tree = FilePicker::default();
    tree.prepare(
        Rect::new(0, 0, 18, 4),
        Document::tree(
            "Explorer",
            vec![Row::tree(
                0,
                Line::raw("very-long-tree-file-name.rs"),
                0,
                false,
            )],
        ),
        None,
    );
    terminal.draw(|frame| tree.render(frame, false)).unwrap();

    let row = rendered_row(terminal.backend().buffer(), tree.metrics().list_area);
    assert!(row.contains("..."), "{row:?}");
    assert!(!row.starts_with('·'), "{row:?}");
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

    let copy_action = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .find(|cell| cell.symbol() == "C")
        .expect("copy action");
    assert_enabled_control(copy_action);
    let shortcut = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .find(|cell| cell.symbol() == "c")
        .expect("menu shortcut");
    assert_enabled_control(shortcut);
    assert_eq!(
        rendered_row(terminal.backend().buffer(), Rect::new(3, 2, 22, 1)),
        "[a] Copy absolute path"
    );
    assert_eq!(
        rendered_row(terminal.backend().buffer(), Rect::new(3, 3, 22, 1)).trim(),
        ""
    );
    assert_eq!(
        rendered_row(terminal.backend().buffer(), Rect::new(3, 4, 22, 1)),
        "[r] Copy relative path"
    );
    let dismiss = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .find(|cell| cell.symbol() == interaction::DISMISS)
        .expect("menu dismiss control");
    assert_enabled_control(dismiss);
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
