use super::*;

pub(super) fn file_context_menu_area(model: &Model, area: Rect) -> Option<Rect> {
    let menu = model.file_context_menu.as_ref()?;
    let width = 24_u16.min(area.width);
    let height = 4_u16.min(area.height);
    Some(Rect::new(
        menu.column.min(area.right().saturating_sub(width)),
        menu.row.min(area.bottom().saturating_sub(height)),
        width,
        height,
    ))
}

pub(super) fn map_file_context_menu_event(
    event: &Event,
    model: &Model,
    area: Rect,
) -> Option<diffo_app::Message> {
    match event {
        Event::Key(key)
            if key.kind == crossterm::event::KeyEventKind::Press
                && key.code == crossterm::event::KeyCode::Esc =>
        {
            Some(diffo_app::Message::CloseFileContextMenu)
        }
        Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
            let menu = file_context_menu_area(model, area)?;
            if mouse.column > menu.x && mouse.column < menu.right().saturating_sub(1) {
                match mouse.row.saturating_sub(menu.y) {
                    1 => Some(diffo_app::Message::CopyAbsolutePath),
                    2 => Some(diffo_app::Message::CopyRelativePath),
                    _ => Some(diffo_app::Message::CloseFileContextMenu),
                }
            } else {
                Some(diffo_app::Message::CloseFileContextMenu)
            }
        }
        Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Right) => {
            file_at_position(model, area, mouse.column, mouse.row)
                .map(|file| diffo_app::Message::OpenFileContextMenu(file, mouse.column, mouse.row))
        }
        _ => None,
    }
}

pub(super) fn render_file_context_menu(frame: &mut Frame, model: &Model) {
    let Some(area) = file_context_menu_area(model, frame.area()) else {
        return;
    };
    frame.render_widget(Clear, area);
    frame.render_widget(
        List::new(["Copy absolute path", "Copy relative path"]).block(
            Block::default()
                .title(" Path ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        ),
        area,
    );
}

pub(super) fn render_command_palette(frame: &mut Frame, model: &Model) {
    let Some(palette) = model.command_palette.as_ref() else {
        return;
    };
    let commands = palette.matches();
    let (area, results_area) = command_palette_layout(frame.area());
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Command Palette "),
        area,
    );
    let inner = area.inner(ratatui::layout::Margin {
        vertical: 1,
        horizontal: 2,
    });
    let sections = command_palette_sections(inner);
    frame.render_widget(
        Paragraph::new(format!("> {}█", palette.query)).style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        sections[0],
    );
    frame.render_widget(
        Paragraph::new("─".repeat(usize::from(sections[1].width)))
            .style(Style::default().fg(Color::DarkGray)),
        sections[1],
    );
    let items = if commands.is_empty() {
        vec![ListItem::new("No matching commands").style(Style::default().fg(Color::DarkGray))]
    } else {
        commands
            .iter()
            .map(|command| ListItem::new(command.label))
            .collect()
    };
    let list = List::new(items).highlight_symbol("› ").highlight_style(
        Style::default()
            .bg(Color::Indexed(24))
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    );
    let mut state = ListState::default().with_selected(
        (!commands.is_empty()).then_some(palette.selected.min(commands.len().saturating_sub(1))),
    );
    frame.render_stateful_widget(list, results_area, &mut state);
    frame.render_widget(
        Paragraph::new("type to search · ↑↓ select · enter run · esc close")
            .style(Style::default().fg(Color::DarkGray)),
        sections[3],
    );
}

pub(super) fn render_help(frame: &mut Frame, model: &Model) {
    if !model.help_open {
        return;
    }
    let area = help_layout(frame.area());
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Help ");
    let inner = block.inner(area).inner(ratatui::layout::Margin {
        vertical: 1,
        horizontal: 2,
    });
    frame.render_widget(block, area);
    let sections = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(inner);
    let rows = input::help_rows(model.access_mode)
        .into_iter()
        .map(|(keys, description)| {
            Row::new([
                Cell::from(keys).style(
                    Style::default()
                        .fg(Color::LightCyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Cell::from(description).style(Style::default().fg(Color::White)),
            ])
        });
    let table = Table::new(rows, [Constraint::Length(22), Constraint::Min(24)])
        .header(
            Row::new(["Shortcut", "Action"])
                .style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )
                .bottom_margin(1),
        )
        .column_spacing(2);
    frame.render_widget(table, sections[0]);
    let footer = if model.access_mode == AccessMode::ReadOnly {
        "Esc: close  ·  Read-only: repository actions are disabled"
    } else {
        "Esc: close"
    };
    frame.render_widget(
        Paragraph::new(footer).style(Style::default().fg(Color::DarkGray)),
        sections[1],
    );
}

pub(super) fn render_toasts(frame: &mut Frame, model: &Model) {
    for (toast, area) in model.toasts.iter().zip(toast_areas(model, frame.area())) {
        let color = match toast.kind {
            ToastKind::Success => Color::LightGreen,
            ToastKind::Info => Color::LightCyan,
            ToastKind::Error => Color::LightRed,
        };
        frame.render_widget(Clear, area);
        let text = toast.detail.as_ref().map_or_else(
            || toast.title.clone(),
            |detail| format!("{}\n{detail}", toast.title),
        );
        frame.render_widget(
            Paragraph::new(text)
                .wrap(ratatui::widgets::Wrap { trim: true })
                .style(Style::default().fg(Color::White))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(color)),
                ),
            area,
        );
    }
}

pub(super) fn toast_areas(model: &Model, area: Rect) -> Vec<Rect> {
    let width = 44.min(area.width.saturating_sub(2));
    let right = area.right().saturating_sub(1);
    let mut bottom = area.bottom().saturating_sub(2);
    model
        .toasts
        .iter()
        .filter_map(|toast| {
            let inner_width = usize::from(width.saturating_sub(2)).max(1);
            let text_rows = std::iter::once(toast.title.as_str())
                .chain(toast.detail.as_deref())
                .map(|text| text.chars().count().div_ceil(inner_width))
                .sum::<usize>();
            let height = u16::try_from(text_rows)
                .unwrap_or(u16::MAX)
                .saturating_add(2)
                .clamp(3, 6);
            if width < 4 || bottom < area.y.saturating_add(height) {
                return None;
            }
            let rect = Rect::new(right.saturating_sub(width), bottom - height, width, height);
            bottom = rect.y;
            Some(rect)
        })
        .collect()
}

pub(super) fn toast_at_position(model: &Model, area: Rect, column: u16, row: u16) -> Option<u64> {
    model
        .toasts
        .iter()
        .zip(toast_areas(model, area))
        .find_map(|(toast, area)| area.contains((column, row).into()).then_some(toast.id))
}

pub(super) fn render_commit_editor(frame: &mut Frame, model: &Model) {
    if !model.commit_input_focused() {
        return;
    }
    let (area, input, commit, cancel, footer) = commit_editor_layout(frame.area());
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Commit message "),
        area,
    );

    let empty = model.commit_message.is_empty();
    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::LightCyan));
    let input_inner = input_block.inner(input);
    let field_width = usize::from(input_inner.width);
    let cursor = model.commit_message_cursor();
    let (message, cursor_offset) = if empty {
        (
            model
                .suggested_commit_message()
                .unwrap_or_else(|| "Type a message…".to_owned()),
            0,
        )
    } else {
        let start = cursor.saturating_sub(field_width.saturating_sub(1));
        (
            model
                .commit_message
                .chars()
                .skip(start)
                .take(field_width)
                .collect(),
            cursor.saturating_sub(start),
        )
    };
    frame.render_widget(
        Paragraph::new(message).style(if empty {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::White)
        }),
        input_inner,
    );
    frame.render_widget(input_block, input);

    let commit_style = if model.primary_action() == diffo_app::PrimaryAction::Commit
        && model.primary_action_enabled()
    {
        Style::default()
            .bg(Color::Indexed(24))
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    frame.render_widget(
        Paragraph::new("[ Commit ]")
            .alignment(Alignment::Center)
            .style(commit_style),
        commit,
    );
    frame.render_widget(
        Paragraph::new("[ Cancel ]")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::White)),
        cancel,
    );
    frame.render_widget(
        Paragraph::new("Enter: commit · Esc: cancel · click outside: close")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray)),
        footer,
    );

    let cursor_offset = cursor_offset.min(usize::from(input_inner.width.saturating_sub(1)));
    let cursor_x = input_inner
        .x
        .saturating_add(u16::try_from(cursor_offset).unwrap_or(u16::MAX));
    frame.set_cursor_position((cursor_x, input_inner.y));
}

pub(super) fn commit_editor_layout(area: Rect) -> (Rect, Rect, Rect, Rect, Rect) {
    let width = (area.width.saturating_mul(7) / 10).clamp(34.min(area.width), 84.min(area.width));
    let height = 11.min(area.height);
    let modal = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let inner = modal.inner(ratatui::layout::Margin {
        vertical: 1,
        horizontal: 2,
    });
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(inner);
    let buttons =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[2]);
    (modal, rows[0], buttons[0], buttons[1], rows[4])
}

pub(crate) fn commit_editor_action_at_position(
    model: &Model,
    area: Rect,
    column: u16,
    row: u16,
) -> Option<diffo_app::Message> {
    let (dialog_area, _input, commit, cancel, _footer) = commit_editor_layout(area);
    let position = (column, row).into();
    if !dialog_area.contains(position) {
        return Some(diffo_app::Message::BlurCommitInput);
    }
    if cancel.contains(position) {
        return Some(diffo_app::Message::BlurCommitInput);
    }
    if commit.contains(position)
        && model.primary_action() == diffo_app::PrimaryAction::Commit
        && model.primary_action_enabled()
    {
        return Some(diffo_app::Message::ExecutePrimaryAction);
    }
    None
}

pub(super) fn help_layout(area: Rect) -> Rect {
    let width = (area.width.saturating_mul(4) / 5).clamp(40.min(area.width), 90.min(area.width));
    let top = area.y.saturating_add(area.height.saturating_mul(10) / 100);
    let height = 26.min(area.bottom().saturating_sub(top));
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        top,
        width,
        height,
    )
}

pub(super) fn command_palette_layout(area: Rect) -> (Rect, Rect) {
    let width = (area.width.saturating_mul(7) / 10).clamp(30.min(area.width), 80.min(area.width));
    let top = area.y.saturating_add(area.height.saturating_mul(20) / 100);
    let height = 18.min(area.bottom().saturating_sub(top));
    let palette = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        top,
        width,
        height,
    );
    let inner = palette.inner(ratatui::layout::Margin {
        vertical: 1,
        horizontal: 2,
    });
    let sections = command_palette_sections(inner);
    (palette, sections[2])
}

pub(super) fn command_palette_sections(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area)
}
