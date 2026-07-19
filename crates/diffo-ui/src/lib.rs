#![doc = include_str!("../README.md")]

pub mod command_palette;
pub mod file_icons;
pub mod file_picker;
mod scrollbar;
pub mod search_picker;
pub mod text_view;

pub use scrollbar::render_scrollbar;

use crossterm::event::MouseEventKind;
use diffo_core::ChangeKind;
use diffo_highlight::HighlightedLine;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders},
};

/// Fixed semantic colors for Diffo's application chrome.
///
/// Renderers use these roles instead of choosing terminal colors locally. Diff
/// content and syntax highlighting keep their separate, content-specific palettes.
pub mod theme {
    use ratatui::style::Color;

    pub const TEXT: Color = Color::White;
    pub const CHROME: Color = Color::DarkGray;
    pub const INFORMATION: Color = Color::LightCyan;
    pub const SELECTION_BACKGROUND: Color = CHROME;
    pub const SUCCESS: Color = Color::LightGreen;
    pub const WARNING: Color = Color::Yellow;
    pub const DANGER: Color = Color::LightRed;
    pub const CONFLICT_FOREGROUND: Color = Color::LightYellow;
    pub const CONFLICT_BACKGROUND: Color = Color::Indexed(58);
}

/// Fixed Nerd Font icons used by Diffo's interface.
pub mod icons {
    pub const ACTIVITY_EXPLORER: &str = "";
    pub const ACTIVITY_SEARCH: &str = "";
    pub const ACTIVITY_DIFF: &str = "";
    pub const TREE_COLLAPSED: &str = " ";
    pub const TREE_EXPANDED: &str = " ";
    pub const TREE_LEAF: &str = "  ";
    pub const EDIT: &str = "";
    pub const DISMISS: &str = "";
    pub const MAXIMIZE: &str = "";
    pub const PANE_DRAG: &str = "";
    pub const CHANGE_PREVIOUS: &str = "";
    pub const CHANGE_NEXT: &str = "";
    pub const CHANGE_MARKER: &str = "";
    pub const ADD: &str = "";
    pub const REMOVE: &str = "";
    pub const SELECTION: &str = " ";
    pub const SPINNER: [&str; 4] = ["", "", "", ""];
}

#[must_use]
pub fn command_progress_style(tick: usize) -> Style {
    const GRADIENT: [u8; 12] = [24, 25, 31, 37, 43, 42, 36, 30, 24, 60, 54, 53];
    Style::default()
        .fg(Color::Indexed(GRADIENT[(tick / 4) % GRADIENT.len()]))
        .add_modifier(Modifier::BOLD)
}

/// Fixed layout tokens for Diffo's structural chrome.
pub mod design {
    use ratatui::layout::Margin;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct ResponsiveWidth {
        percent: u16,
        minimum: u16,
        maximum: u16,
    }

    impl ResponsiveWidth {
        #[must_use]
        pub const fn new(percent: u16, minimum: u16, maximum: u16) -> Self {
            Self {
                percent,
                minimum,
                maximum,
            }
        }

        #[must_use]
        pub fn resolve(self, available: u16) -> u16 {
            (available.saturating_mul(self.percent) / FULL_PERCENT)
                .clamp(self.minimum.min(available), self.maximum.min(available))
        }
    }

    pub const BORDER_WIDTH: u16 = 1;
    pub const PANEL_BORDER_OVERHEAD: u16 = BORDER_WIDTH * 2;
    pub const SINGLE_LINE_HEIGHT: u16 = 1;
    pub const PANEL_INSET: Margin = Margin {
        horizontal: 1,
        vertical: 1,
    };
    pub const DIALOG_INSET: Margin = Margin {
        horizontal: 2,
        vertical: 1,
    };
    pub const INLINE_GAP: u16 = 2;
    pub const FULL_PERCENT: u16 = 100;
    pub const EQUAL_SPLIT_PERCENT: u16 = 50;
    pub const STATUS_HEIGHT: u16 = 1;
    pub const MIN_TOOL_CONTENT_HEIGHT: u16 = 3;
    pub const DEFAULT_PANE_PERCENT: u16 = 25;
    pub const MAX_PANE_PERCENT: u16 = 80;
    pub const PANE_DRAG_BOTTOM_GUARD: u16 = PANEL_BORDER_OVERHEAD;

    pub const ACTIVITY_RAIL_WIDTH: u16 = 5;
    pub const ACTIVITY_CONTROL_HEIGHT: u16 = 3;
    pub const ACTIVITY_CONTROL_CONTENT_OFFSET: u16 = 1;
    pub const FILE_COMPOSER_HEIGHT: u16 = 6;
    pub const MIN_FILE_GROUP_HEIGHT: u16 = 2;
    pub const COMMIT_FIELD_HEIGHT: u16 = 3;
    pub const PRIMARY_ACTION_HEIGHT: u16 = 2;

    pub const COMMAND_PALETTE_WIDTH: ResponsiveWidth = ResponsiveWidth::new(70, 30, 80);
    pub const COMMAND_PALETTE_TOP_PERCENT: u16 = 20;
    pub const COMMAND_PALETTE_MAX_HEIGHT: u16 = 18;
    pub const HELP_WIDTH: ResponsiveWidth = ResponsiveWidth::new(80, 40, 90);
    pub const HELP_TOP_PERCENT: u16 = 10;
    pub const HELP_MAX_HEIGHT: u16 = 27;
    pub const HELP_SHORTCUT_COLUMN_WIDTH: u16 = 22;
    pub const HELP_ACTION_MIN_WIDTH: u16 = 24;
    pub const HELP_COLUMN_GAP: u16 = 2;
    pub const COMMIT_EDITOR_WIDTH: ResponsiveWidth = ResponsiveWidth::new(70, 34, 84);
    pub const COMMIT_EDITOR_MAX_HEIGHT: u16 = 11;
    pub const PROMPT_MESSAGE_HEIGHT: u16 = 2;
    pub const SEARCH_PICKER_WIDTH: ResponsiveWidth = ResponsiveWidth::new(70, 30, 80);
    pub const SEARCH_PICKER_TOP_PERCENT: u16 = 20;
    pub const SEARCH_PICKER_MAX_HEIGHT: u16 = 18;

    pub const TOAST_MAX_WIDTH: u16 = 44;
    pub const TOAST_MIN_WIDTH: u16 = 4;
    pub const TOAST_MIN_HEIGHT: u16 = 3;
    pub const TOAST_MAX_HEIGHT: u16 = 6;
    pub const PATH_MENU_WIDTH: u16 = 24;
    pub const PATH_MENU_HEIGHT: u16 = 5;
    pub const TREE_HEADER_MIN_WIDTH: u16 = 12;
    pub const TREE_HEADER_ACTION_WIDTH: u16 = 3;
    pub const TREE_HEADER_ACTION_GAP: u16 = 1;
    pub const TREE_HEADER_ACTIONS_WIDTH: u16 =
        TREE_HEADER_ACTION_WIDTH * 2 + TREE_HEADER_ACTION_GAP + BORDER_WIDTH;
    pub const PATH_MENU_FIRST_ACTION_ROW: u16 = 1;
    pub const PATH_MENU_SECOND_ACTION_ROW: u16 = 3;
    pub const SIDE_BY_SIDE_DIVIDER_WIDTH: u16 = 3;
    pub const SIDE_BY_SIDE_COLUMN_COUNT: u16 = 2;
    pub const DIFF_RIGHT_RAIL_WIDTH: u16 = BORDER_WIDTH * 2;
    pub const DIFF_PAGE_NON_CONTENT_ROWS: u16 = 3;

    #[must_use]
    pub const fn panel_content_extent(outer: u16) -> u16 {
        outer.saturating_sub(PANEL_BORDER_OVERHEAD)
    }
}

#[must_use]
pub fn modal_block(title: impl Into<String>) -> Block<'static> {
    let title = terminal_safe_text(&title.into());
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::CHROME))
        .title(format!(" {title} "))
}

pub(crate) fn fuzzy_score(candidate: &str, query: &str) -> Option<i64> {
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

const WHEEL_SCROLL_ROWS: i64 = 1;

#[must_use]
pub const fn wheel_scroll_delta(kind: MouseEventKind) -> Option<i64> {
    match kind {
        MouseEventKind::ScrollUp => Some(-WHEEL_SCROLL_ROWS),
        MouseEventKind::ScrollDown => Some(WHEEL_SCROLL_ROWS),
        _ => None,
    }
}

#[must_use]
pub fn scroll_offset(position: usize, amount: i64, maximum: usize) -> usize {
    let magnitude = usize::try_from(amount.unsigned_abs()).unwrap_or(usize::MAX);
    if amount < 0 {
        position.saturating_sub(magnitude)
    } else {
        position.saturating_add(magnitude).min(maximum)
    }
}

#[must_use]
pub const fn maximum_scroll(content: usize, viewport: usize) -> usize {
    if viewport == 0 {
        0
    } else {
        content.saturating_sub(viewport)
    }
}

#[must_use]
pub fn scrollbar_position(coordinate: u16, track_length: u16, maximum: usize) -> usize {
    if track_length <= 1 {
        return 0;
    }
    usize::from(coordinate.min(track_length - 1)) * maximum / usize::from(track_length - 1)
}

#[must_use]
pub const fn scrollbar_position_count(content: usize, viewport: usize) -> usize {
    maximum_scroll(content, viewport).saturating_add(1)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PaneAreas {
    pub leading: Rect,
    pub trailing: Rect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PaneSplit {
    percent: u16,
    dragging: bool,
}

impl Default for PaneSplit {
    fn default() -> Self {
        Self {
            percent: design::DEFAULT_PANE_PERCENT,
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
            Constraint::Percentage(design::FULL_PERCENT.saturating_sub(self.percent)),
        ])
        .split(area);
        PaneAreas {
            leading: columns[0],
            trailing: columns[1],
        }
    }

    #[must_use]
    pub fn contains_seam(self, area: Rect, column: u16, row: u16) -> bool {
        if row < area.y || row >= area.bottom().saturating_sub(design::PANE_DRAG_BOTTOM_GUARD) {
            return false;
        }
        column.abs_diff(self.areas(area).trailing.x) <= 1
    }

    #[must_use]
    pub fn seam_marker_area(self, area: Rect) -> Rect {
        let height = area.height.saturating_sub(design::PANE_DRAG_BOTTOM_GUARD);
        if area.width == 0 || height == 0 {
            return Rect::default();
        }
        Rect::new(
            self.areas(area)
                .trailing
                .x
                .min(area.right().saturating_sub(design::BORDER_WIDTH)),
            area.y.saturating_add(height / 2),
            design::BORDER_WIDTH,
            design::SINGLE_LINE_HEIGHT,
        )
    }

    pub fn begin_drag(&mut self) {
        self.dragging = true;
    }

    pub fn drag_to(&mut self, area: Rect, column: u16) {
        if !self.dragging || area.width == 0 {
            return;
        }
        let offset = column.saturating_sub(area.x).min(area.width);
        let percent = u16::try_from(
            u32::from(offset) * u32::from(design::FULL_PERCENT) / u32::from(area.width),
        )
        .unwrap_or(design::FULL_PERCENT)
        .min(design::MAX_PANE_PERCENT);
        self.percent = percent;
    }

    pub fn end_drag(&mut self) {
        self.dragging = false;
    }

    #[must_use]
    pub fn border_style(self) -> Style {
        let style = Style::default().fg(theme::CHROME);
        if self.dragging {
            style.add_modifier(Modifier::BOLD)
        } else {
            style
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
    let rows = Layout::vertical([
        Constraint::Min(design::MIN_TOOL_CONTENT_HEIGHT),
        Constraint::Length(design::STATUS_HEIGHT),
    ])
    .split(area);
    ToolAreas {
        content: rows[0],
        status: rows[1],
    }
}

#[must_use]
pub fn change_kind_style(kind: ChangeKind, selected: bool) -> Style {
    let style = match kind {
        ChangeKind::Added | ChangeKind::Untracked => Style::default().fg(theme::SUCCESS),
        ChangeKind::Modified => Style::default().fg(theme::WARNING),
        ChangeKind::Deleted => Style::default()
            .fg(theme::DANGER)
            .add_modifier(Modifier::CROSSED_OUT),
        ChangeKind::Renamed | ChangeKind::Copied => Style::default().fg(theme::INFORMATION),
        ChangeKind::Conflicted => Style::default()
            .fg(theme::DANGER)
            .add_modifier(Modifier::BOLD),
    };
    if selected {
        style.add_modifier(Modifier::BOLD)
    } else {
        style
    }
}

/// Returns the fixed style for an enabled interactive control.
#[must_use]
pub fn enabled_control_style() -> Style {
    Style::default()
        .fg(theme::TEXT)
        .add_modifier(Modifier::BOLD)
}

/// Returns the fixed style for a visible control that cannot currently activate.
#[must_use]
pub fn disabled_control_style() -> Style {
    Style::default().fg(theme::CHROME)
}

#[must_use]
pub fn plain_syntax_spans(line: &HighlightedLine) -> Vec<Span<'static>> {
    line.spans
        .iter()
        .map(|span| {
            Span::styled(
                terminal_safe_text(&span.text),
                Style::default().fg(Color::Rgb(
                    span.foreground.red,
                    span.foreground.green,
                    span.foreground.blue,
                )),
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
    use crossterm::event::MouseEventKind;
    use diffo_highlight::{HighlightedLine, Rgb, StyledSpan};
    use ratatui::text::Line;

    #[test]
    fn interface_icons_are_one_cell_nerd_font_glyphs() {
        let icons = [
            icons::ACTIVITY_EXPLORER,
            icons::ACTIVITY_SEARCH,
            icons::ACTIVITY_DIFF,
            icons::TREE_COLLAPSED,
            icons::TREE_EXPANDED,
            icons::EDIT,
            icons::DISMISS,
            icons::MAXIMIZE,
            icons::PANE_DRAG,
            icons::CHANGE_PREVIOUS,
            icons::CHANGE_NEXT,
            icons::CHANGE_MARKER,
            icons::ADD,
            icons::REMOVE,
            icons::SELECTION,
        ]
        .into_iter()
        .chain(icons::SPINNER);

        for icon in icons {
            let glyphs = icon.chars().filter(|character| !character.is_whitespace());
            assert!(
                glyphs
                    .clone()
                    .all(|glyph| ('\u{e000}'..='\u{f8ff}').contains(&glyph)),
                "icon {icon:?} is outside the Nerd Font private-use range"
            );
            assert_eq!(glyphs.count(), 1, "icon {icon:?} must contain one glyph");
            assert_eq!(Line::raw(icon.trim_end()).width(), 1, "icon {icon:?}");
        }
    }

    #[test]
    fn pane_split_drags_and_bounds_width() {
        let area = Rect::new(5, 2, 100, 20);
        let mut split = PaneSplit::default();
        assert_eq!(split.areas(area).trailing.x, 30);
        assert_eq!(split.seam_marker_area(area), Rect::new(30, 11, 1, 1));
        let marker = split.seam_marker_area(area);
        assert!(split.contains_seam(area, marker.x, marker.y));
        assert!(split.contains_seam(area, 29, 10));
        assert!(!split.contains_seam(area, 28, 10));
        assert!(!split.contains_seam(area, 30, area.bottom().saturating_sub(2)));

        split.drag_to(area, 65);
        assert_eq!(split.percent(), 25);
        split.begin_drag();
        split.drag_to(area, 65);
        split.end_drag();
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
        assert!(split.seam_marker_area(area).is_empty());
    }

    #[test]
    fn shared_scroll_core_has_fixed_wheel_distance_and_bounds() {
        assert_eq!(wheel_scroll_delta(MouseEventKind::ScrollUp), Some(-1));
        assert_eq!(wheel_scroll_delta(MouseEventKind::ScrollDown), Some(1));
        assert_eq!(wheel_scroll_delta(MouseEventKind::Moved), None);
        assert_eq!(scroll_offset(3, -10, 20), 0);
        assert_eq!(scroll_offset(3, 10, 8), 8);
        assert_eq!(maximum_scroll(120, 25), 95);
        assert_eq!(maximum_scroll(120, 0), 0);
        assert_eq!(scrollbar_position(9, 10, 37), 37);
        assert_eq!(scrollbar_position_count(120, 25), 96);
    }

    #[test]
    fn shared_change_styles_cover_neutral_selection_and_git_status() {
        let styles = [
            ChangeKind::Added,
            ChangeKind::Modified,
            ChangeKind::Deleted,
            ChangeKind::Renamed,
            ChangeKind::Copied,
            ChangeKind::Untracked,
            ChangeKind::Conflicted,
        ]
        .map(|kind| {
            (
                kind,
                change_kind_style(kind, false),
                change_kind_style(kind, true),
            )
        });

        insta::assert_debug_snapshot!(styles);
    }

    #[test]
    fn shared_syntax_spans_use_only_the_token_foreground() {
        let spans = plain_syntax_spans(&HighlightedLine {
            spans: vec![StyledSpan {
                text: "value".to_owned(),
                foreground: Rgb {
                    red: 1,
                    green: 2,
                    blue: 3,
                },
            }],
        });
        insta::assert_debug_snapshot!(spans);
    }

    #[test]
    fn terminal_text_makes_control_sequences_visible_and_inert() {
        let safe = terminal_safe_text("before\t\x1b[2J\x08after\u{0085}");

        assert_eq!(safe, "before    ␛[2J␈after�");
        assert!(!safe.chars().any(char::is_control));
    }

    #[test]
    fn terminal_text_makes_newline_sequences_visible_and_inert() {
        let safe = terminal_safe_text("first\nsecond\r\nthird");

        assert_eq!(safe, "first␊second␍␊third");
        assert_eq!(safe.lines().count(), 1);
        assert!(!safe.chars().any(char::is_control));
    }
}
