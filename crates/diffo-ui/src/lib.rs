use diffo_core::ChangeKind;
use diffo_highlight::HighlightedLine;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
};

const DEFAULT_PANE_PERCENT: u16 = 25;
const MAX_PANE_PERCENT: u16 = 80;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PaneAreas {
    pub leading: Rect,
    pub trailing: Rect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PaneSplit {
    percent: u16,
    expanded_percent: u16,
    dragging: bool,
}

impl Default for PaneSplit {
    fn default() -> Self {
        Self {
            percent: DEFAULT_PANE_PERCENT,
            expanded_percent: DEFAULT_PANE_PERCENT,
            dragging: false,
        }
    }
}

impl PaneSplit {
    #[must_use]
    pub const fn percent(self) -> u16 {
        self.percent
    }

    #[must_use]
    pub const fn is_dragging(self) -> bool {
        self.dragging
    }

    #[must_use]
    pub fn areas(self, area: Rect) -> PaneAreas {
        let columns = Layout::horizontal([
            Constraint::Percentage(self.percent),
            Constraint::Percentage(100_u16.saturating_sub(self.percent)),
        ])
        .split(area);
        PaneAreas {
            leading: columns[0],
            trailing: columns[1],
        }
    }

    #[must_use]
    pub fn contains_seam(self, area: Rect, column: u16, row: u16) -> bool {
        if row < area.y || row >= area.bottom().saturating_sub(2) {
            return false;
        }
        column.abs_diff(self.areas(area).trailing.x) <= 1
    }

    pub fn begin_drag(&mut self) {
        self.dragging = true;
    }

    pub fn drag_to(&mut self, area: Rect, column: u16) {
        if !self.dragging || area.width == 0 {
            return;
        }
        let offset = column.saturating_sub(area.x).min(area.width);
        let percent = u16::try_from(u32::from(offset) * 100 / u32::from(area.width))
            .unwrap_or(100)
            .min(MAX_PANE_PERCENT);
        self.percent = percent;
        if percent > 0 {
            self.expanded_percent = percent;
        }
    }

    pub fn end_drag(&mut self) {
        self.dragging = false;
    }

    pub fn toggle(&mut self) {
        if self.percent == 0 {
            self.percent = self.expanded_percent;
        } else {
            self.expanded_percent = self.percent;
            self.percent = 0;
        }
        self.dragging = false;
    }

    #[must_use]
    pub fn border_style(self) -> Style {
        if self.dragging {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ToolAreas {
    pub content: Rect,
    pub status: Rect,
}

#[must_use]
pub fn tool_areas(area: Rect) -> ToolAreas {
    let rows = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).split(area);
    ToolAreas {
        content: rows[0],
        status: rows[1],
    }
}

#[must_use]
pub fn change_kind_style(kind: ChangeKind, selected: bool) -> Style {
    let style = match kind {
        ChangeKind::Added | ChangeKind::Untracked => Style::default().fg(Color::LightGreen),
        ChangeKind::Modified => Style::default().fg(Color::Yellow),
        ChangeKind::Deleted => Style::default()
            .fg(Color::LightRed)
            .add_modifier(Modifier::CROSSED_OUT),
        ChangeKind::Renamed | ChangeKind::Copied => Style::default().fg(Color::LightCyan),
        ChangeKind::Conflicted => Style::default()
            .fg(Color::LightRed)
            .add_modifier(Modifier::BOLD),
    };
    if selected {
        style.add_modifier(Modifier::BOLD)
    } else {
        style
    }
}

#[must_use]
pub fn plain_syntax_spans(line: &HighlightedLine) -> Vec<Span<'static>> {
    line.spans
        .iter()
        .map(|span| {
            let mut modifiers = Modifier::empty();
            if span.bold {
                modifiers.insert(Modifier::BOLD);
            }
            if span.italic {
                modifiers.insert(Modifier::ITALIC);
            }
            if span.underline {
                modifiers.insert(Modifier::UNDERLINED);
            }
            Span::styled(
                terminal_safe_text(&span.text),
                Style::default()
                    .fg(Color::Rgb(
                        span.foreground.red,
                        span.foreground.green,
                        span.foreground.blue,
                    ))
                    .add_modifier(modifiers),
            )
        })
        .collect()
}

/// Replaces characters that terminals interpret as cursor or screen commands.
///
/// Tabs are expanded to a fixed width so horizontal offsets stay deterministic.
#[must_use]
pub fn terminal_safe_text(text: &str) -> String {
    let mut safe = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '\t' => safe.push_str("    "),
            '\u{0000}'..='\u{001f}' => {
                safe.push(char::from_u32(u32::from(character) + 0x2400).unwrap_or('\u{fffd}'));
            }
            '\u{007f}' => safe.push('\u{2421}'),
            character if character.is_control() => safe.push('\u{fffd}'),
            character => safe.push(character),
        }
    }
    safe
}

#[cfg(test)]
mod tests {
    use super::*;
    use diffo_highlight::{HighlightedLine, Rgb, StyledSpan};

    #[test]
    fn pane_split_drags_collapses_restores_and_bounds_width() {
        let area = Rect::new(5, 2, 100, 20);
        let mut split = PaneSplit::default();
        assert_eq!(split.areas(area).trailing.x, 30);
        assert!(split.contains_seam(area, 29, 10));
        assert!(!split.contains_seam(area, 28, 10));
        assert!(!split.contains_seam(area, 30, area.bottom().saturating_sub(2)));

        split.drag_to(area, 65);
        assert_eq!(split.percent(), 25);
        split.begin_drag();
        split.drag_to(area, 65);
        split.end_drag();
        assert_eq!(split.percent(), 60);
        split.toggle();
        assert_eq!(split.percent(), 0);
        split.toggle();
        assert_eq!(split.percent(), 60);
        split.begin_drag();
        split.drag_to(area, area.right());
        assert_eq!(split.percent(), 80);
    }

    #[test]
    fn pane_split_handles_narrow_and_offset_areas() {
        let area = Rect::new(7, 9, 0, 0);
        let mut split = PaneSplit::default();
        split.begin_drag();
        split.drag_to(area, u16::MAX);
        assert_eq!(split.percent(), 25);
        assert!(!split.contains_seam(area, 7, 9));
    }

    #[test]
    fn shared_change_styles_cover_neutral_selection_and_git_status() {
        assert_eq!(
            change_kind_style(ChangeKind::Added, false).fg,
            Some(Color::LightGreen)
        );
        assert_eq!(
            change_kind_style(ChangeKind::Modified, false).fg,
            Some(Color::Yellow)
        );
        assert!(
            change_kind_style(ChangeKind::Deleted, false)
                .add_modifier
                .contains(Modifier::CROSSED_OUT)
        );
        assert!(
            change_kind_style(ChangeKind::Conflicted, true)
                .add_modifier
                .contains(Modifier::BOLD)
        );
    }

    #[test]
    fn shared_syntax_spans_preserve_terminal_modifiers() {
        let spans = plain_syntax_spans(&HighlightedLine {
            spans: vec![StyledSpan {
                text: "value".to_owned(),
                foreground: Rgb {
                    red: 1,
                    green: 2,
                    blue: 3,
                },
                bold: true,
                italic: true,
                underline: true,
            }],
        });
        assert_eq!(spans[0].style.fg, Some(Color::Rgb(1, 2, 3)));
        assert!(spans[0].style.add_modifier.contains(Modifier::BOLD));
        assert!(spans[0].style.add_modifier.contains(Modifier::ITALIC));
        assert!(spans[0].style.add_modifier.contains(Modifier::UNDERLINED));
    }

    #[test]
    fn terminal_text_makes_control_sequences_visible_and_inert() {
        let safe = terminal_safe_text("before\t\x1b[2J\x08after\u{0085}");

        assert_eq!(safe, "before    ␛[2J␈after�");
        assert!(!safe.chars().any(char::is_control));
    }
}
