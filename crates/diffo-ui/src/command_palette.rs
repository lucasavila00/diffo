//! Command palette state, input handling, layout, and rendering.

use crate::{design, enabled_control_style, theme};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CommandId(&'static str);

impl CommandId {
    #[must_use]
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Command {
    pub id: CommandId,
    pub label: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaletteEvent {
    Consumed,
    Execute(CommandId),
    Quit,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommandPalette {
    open: bool,
    commands: Vec<Command>,
    query: String,
    selected: usize,
    last_executed: Option<CommandId>,
}

impl CommandPalette {
    pub fn open(&mut self, commands: impl IntoIterator<Item = Command>) {
        self.open = true;
        self.commands = commands.into_iter().collect();
        self.query.clear();
        self.selected = 0;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.commands.clear();
        self.query.clear();
        self.selected = 0;
    }

    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.open
    }

    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    #[must_use]
    pub const fn selected(&self) -> usize {
        self.selected
    }

    #[must_use]
    pub fn matches(&self) -> Vec<&Command> {
        let mut matches = self
            .commands
            .iter()
            .enumerate()
            .filter_map(|(order, command)| {
                fuzzy_score(command.label, &self.query).map(|score| (command, score, order))
            })
            .collect::<Vec<_>>();
        matches.sort_by(|left, right| {
            if self.query.is_empty() {
                let left_is_last = Some(left.0.id) == self.last_executed;
                let right_is_last = Some(right.0.id) == self.last_executed;
                right_is_last
                    .cmp(&left_is_last)
                    .then_with(|| left.2.cmp(&right.2))
            } else {
                right.1.cmp(&left.1).then_with(|| left.2.cmp(&right.2))
            }
        });
        matches.into_iter().map(|(command, _, _)| command).collect()
    }

    pub fn handle_event(&mut self, event: &Event, area: Rect) -> Option<PaletteEvent> {
        if !self.open {
            return None;
        }
        if let Event::Mouse(mouse) = event
            && mouse.kind == MouseEventKind::Down(MouseButton::Left)
        {
            let (_, results) = command_palette_layout(area);
            if results.contains((mouse.column, mouse.row).into()) {
                let index = usize::from(mouse.row.saturating_sub(results.y));
                if index < self.matches().len() {
                    self.select(index);
                    return Some(self.execute_selected());
                }
            }
            return Some(PaletteEvent::Consumed);
        }
        let Event::Key(key) = event else {
            return Some(PaletteEvent::Consumed);
        };
        if key.kind != KeyEventKind::Press {
            return Some(PaletteEvent::Consumed);
        }
        let result = match key.code {
            KeyCode::Esc => {
                self.close();
                PaletteEvent::Consumed
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                PaletteEvent::Quit
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.selected = 0;
                PaletteEvent::Consumed
            }
            KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
                PaletteEvent::Consumed
            }
            KeyCode::Down => {
                self.selected = self
                    .selected
                    .saturating_add(1)
                    .min(self.matches().len().saturating_sub(1));
                PaletteEvent::Consumed
            }
            KeyCode::Enter => self.execute_selected(),
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.query.push(character);
                self.selected = 0;
                PaletteEvent::Consumed
            }
            _ => PaletteEvent::Consumed,
        };
        Some(result)
    }

    pub fn render(&self, frame: &mut Frame, content_area: Rect) {
        if !self.open {
            return;
        }
        let commands = self.matches();
        let (area, _) = command_palette_layout(content_area);
        frame.render_widget(Clear, area);
        frame.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::CHROME))
                .title(" Command Palette "),
            area,
        );
        let inner = area.inner(design::DIALOG_INSET);
        let sections = command_palette_sections(inner);
        frame.render_widget(
            Paragraph::new(format!("> {}█", self.query)).style(
                Style::default()
                    .fg(theme::TEXT)
                    .add_modifier(Modifier::BOLD),
            ),
            sections[0],
        );
        frame.render_widget(
            Paragraph::new("─".repeat(usize::from(sections[1].width)))
                .style(Style::default().fg(theme::CHROME)),
            sections[1],
        );
        let items = if commands.is_empty() {
            vec![ListItem::new("No matching commands").style(Style::default().fg(theme::CHROME))]
        } else {
            commands
                .iter()
                .map(|command| ListItem::new(command.label).style(enabled_control_style()))
                .collect()
        };
        let list = List::new(items)
            .highlight_symbol("› ")
            .highlight_style(enabled_control_style().bg(theme::SELECTION_BACKGROUND));
        let mut state = ListState::default().with_selected(
            (!commands.is_empty()).then_some(self.selected.min(commands.len().saturating_sub(1))),
        );
        frame.render_stateful_widget(list, sections[2], &mut state);
        frame.render_widget(
            Paragraph::new(Line::styled(
                "↑/↓ select · Enter run · Esc close",
                Style::default().fg(theme::CHROME),
            )),
            sections[3],
        );
    }

    fn select(&mut self, index: usize) {
        self.selected = index.min(self.matches().len().saturating_sub(1));
    }

    fn execute_selected(&mut self) -> PaletteEvent {
        let command = self.matches().get(self.selected).map(|command| command.id);
        if let Some(command) = command {
            self.last_executed = Some(command);
        }
        self.close();
        command.map_or(PaletteEvent::Consumed, PaletteEvent::Execute)
    }
}

#[must_use]
pub fn command_palette_layout(area: Rect) -> (Rect, Rect) {
    let width = design::COMMAND_PALETTE_WIDTH.resolve(area.width);
    let top = area.y.saturating_add(
        area.height
            .saturating_mul(design::COMMAND_PALETTE_TOP_PERCENT)
            / 100,
    );
    let height = design::COMMAND_PALETTE_MAX_HEIGHT.min(area.bottom().saturating_sub(top));
    let palette = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        top,
        width,
        height,
    );
    let inner = palette.inner(design::DIALOG_INSET);
    let sections = command_palette_sections(inner);
    (palette, sections[2])
}

fn command_palette_sections(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::vertical([
        Constraint::Length(design::SINGLE_LINE_HEIGHT),
        Constraint::Length(design::SINGLE_LINE_HEIGHT),
        Constraint::Min(design::SINGLE_LINE_HEIGHT),
        Constraint::Length(design::SINGLE_LINE_HEIGHT),
    ])
    .split(area)
}

fn fuzzy_score(candidate: &str, query: &str) -> Option<i64> {
    if query.is_empty() {
        return Some(0);
    }
    let candidate = candidate.as_bytes();
    let mut cursor = 0;
    let mut previous_match = None;
    let mut score = 0_i64;
    for needle in query.bytes().map(|byte| byte.to_ascii_lowercase()) {
        let offset = candidate[cursor..]
            .iter()
            .position(|byte| byte.to_ascii_lowercase() == needle)?;
        let index = cursor + offset;
        let boundary = index == 0 || !candidate[index - 1].is_ascii_alphanumeric();
        score += if previous_match == Some(index.saturating_sub(1)) {
            100
        } else if boundary {
            40
        } else {
            10
        };
        score -= i64::try_from(offset).unwrap_or(i64::MAX);
        previous_match = Some(index);
        cursor = index + 1;
    }
    Some(score - i64::try_from(candidate.len()).unwrap_or(i64::MAX) / 10)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEvent;
    use ratatui::{Terminal, backend::TestBackend};

    const FETCH: CommandId = CommandId::new("git.fetch");
    const PULL: CommandId = CommandId::new("git.pull");
    const COMMANDS: [Command; 2] = [
        Command {
            id: FETCH,
            label: "Git: Fetch",
        },
        Command {
            id: PULL,
            label: "Git: Pull",
        },
    ];

    #[test]
    fn catalogs_are_per_palette_and_search_is_shared() {
        let mut palette = CommandPalette::default();
        palette.open(COMMANDS);
        let _ = palette.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE)),
            Rect::default(),
        );
        assert_eq!(palette.matches()[0].id, PULL);
    }

    #[test]
    fn enter_executes_the_selected_opaque_command() {
        let mut palette = CommandPalette::default();
        palette.open(COMMANDS);
        let event = palette.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Rect::default(),
        );
        assert_eq!(event, Some(PaletteEvent::Execute(FETCH)));
        assert!(!palette.is_open());
    }

    #[test]
    fn last_executed_command_is_first_when_reopened_without_a_query() {
        let mut palette = CommandPalette::default();
        palette.open(COMMANDS);
        let _ = palette.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
            Rect::default(),
        );
        let event = palette.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Rect::default(),
        );
        assert_eq!(event, Some(PaletteEvent::Execute(PULL)));

        palette.open(COMMANDS);

        assert_eq!(
            palette
                .matches()
                .into_iter()
                .map(|command| command.id)
                .collect::<Vec<_>>(),
            vec![PULL, FETCH]
        );
        assert_eq!(palette.selected(), 0);
    }

    #[test]
    fn a_query_uses_fuzzy_order_instead_of_command_history() {
        let mut palette = CommandPalette::default();
        palette.open(COMMANDS);
        let _ = palette.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
            Rect::default(),
        );
        let _ = palette.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Rect::default(),
        );
        palette.open(COMMANDS);

        let _ = palette.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE)),
            Rect::default(),
        );

        assert_eq!(palette.matches()[0].id, FETCH);
    }

    #[test]
    fn renders_and_handles_mouse_without_app_specific_state() {
        let mut palette = CommandPalette::default();
        palette.open(COMMANDS);
        let area = Rect::new(0, 0, 100, 30);
        let (palette_area, results_area) = command_palette_layout(area);
        assert_eq!(palette_area.y, 6);
        assert_eq!(palette_area.height, 18);
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| palette.render(frame, area)).unwrap();
        let buffer = terminal.backend().buffer();
        let rendered = (palette_area.y..palette_area.bottom())
            .map(|row| {
                (palette_area.x..palette_area.right())
                    .map(|column| buffer[(column, row)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect::<Vec<_>>();
        let selected_style = buffer
            .content
            .iter()
            .find(|cell| cell.symbol() == "G")
            .expect("selected command")
            .style();
        insta::assert_debug_snapshot!((rendered, selected_style));

        let blank = Event::Mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: results_area.x,
            row: results_area.y.saturating_add(5),
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(
            palette.handle_event(&blank, area),
            Some(PaletteEvent::Consumed)
        );
        assert!(palette.is_open());

        let event = Event::Mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: results_area.x,
            row: results_area.y.saturating_add(1),
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(
            palette.handle_event(&event, area),
            Some(PaletteEvent::Execute(PULL))
        );
    }
}
