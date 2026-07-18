use super::{
    ChangeArea, Color, HighlightedDiff, HighlightedLine, Line, Modifier, RenderLine, Rgb, RowKind,
    SideBySideRow, Span, Style, StyledSpan, terminal_safe_text,
};

#[must_use]
pub(super) fn file_action_style(change_area: ChangeArea) -> Style {
    let color = match change_area {
        ChangeArea::Staged => Color::LightRed,
        ChangeArea::Unstaged => Color::LightGreen,
    };
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

pub(super) fn inline_line(
    row: &RenderLine,
    highlighted: &HighlightedDiff,
    width: usize,
) -> Line<'static> {
    let prefix = match row.kind {
        RowKind::Removed => "-",
        RowKind::Added => "+",
        RowKind::Conflict => "!",
        RowKind::Header => "@",
        _ => " ",
    };
    let number = row
        .number
        .map_or_else(|| "    ".to_owned(), |number| format!("{number:>4}"));
    if matches!(row.kind, RowKind::Header | RowKind::Meta) {
        return Line::styled(
            format!("{number} {prefix} {}", terminal_safe_text(&row.text)),
            row_style(row.kind),
        );
    }
    let mut spans = vec![Span::styled(
        format!("{number} {prefix} "),
        gutter_style(row.kind),
    )];
    spans.extend(code_spans(row, highlighted));
    pad_to_width(&mut spans, width, diff_background(row.kind));
    Line::from(spans)
}

pub(super) fn side_by_side_line(
    row: &SideBySideRow,
    column_width: usize,
    highlighted: &HighlightedDiff,
) -> Line<'static> {
    let mut spans = format_cell(row.old.as_ref(), column_width, highlighted);
    spans.push(Span::raw(" │ "));
    spans.extend(format_cell(row.new.as_ref(), column_width, highlighted));
    Line::from(spans)
}

pub(super) fn format_cell(
    line: Option<&RenderLine>,
    width: usize,
    highlighted: &HighlightedDiff,
) -> Vec<Span<'static>> {
    let Some(line) = line else {
        return vec![Span::raw(" ".repeat(width))];
    };
    let number = line
        .number
        .map_or_else(|| "    ".to_owned(), |number| format!("{number:>4}"));
    if matches!(line.kind, RowKind::Header | RowKind::Meta) {
        return clip_and_pad(
            vec![Span::styled(
                format!("{number} {}", line.text),
                row_style(line.kind),
            )],
            width,
            Style::default(),
        );
    }
    let mut spans = vec![Span::styled(format!("{number} "), gutter_style(line.kind))];
    spans.extend(code_spans(line, highlighted));
    clip_and_pad(spans, width, diff_background(line.kind))
}

pub(super) fn code_spans(row: &RenderLine, highlighted: &HighlightedDiff) -> Vec<Span<'static>> {
    let highlighted_line = row.number.and_then(|number| match row.kind {
        RowKind::Removed => highlighted.old.get(&number),
        RowKind::Added | RowKind::Context | RowKind::Changed => highlighted.new.get(&number),
        RowKind::Header | RowKind::Conflict | RowKind::Meta => None,
    });
    let background = diff_background(row.kind);
    highlighted_line.map_or_else(
        || vec![Span::styled(terminal_safe_text(&row.text), background)],
        |line| syntax_spans(line, background, row.kind),
    )
}

pub(super) fn syntax_spans(
    line: &HighlightedLine,
    background: Style,
    row_kind: RowKind,
) -> Vec<Span<'static>> {
    line.spans
        .iter()
        .map(|span| {
            Span::styled(
                terminal_safe_text(&span.text),
                syntax_style(span, row_kind).patch(background),
            )
        })
        .collect()
}

pub(super) fn syntax_style(span: &StyledSpan, row_kind: RowKind) -> Style {
    let foreground = contrasting_foreground(span.foreground, row_kind);
    Style::default().fg(Color::Rgb(
        foreground.red,
        foreground.green,
        foreground.blue,
    ))
}

pub(super) fn contrasting_foreground(foreground: Rgb, row_kind: RowKind) -> Rgb {
    let Some(background) = diff_background_rgb(row_kind) else {
        return foreground;
    };
    if contrast_ratio(foreground, background) >= 4.5 {
        return foreground;
    }
    for step in 1..=10 {
        let candidate = Rgb {
            red: lighten_channel(foreground.red, step),
            green: lighten_channel(foreground.green, step),
            blue: lighten_channel(foreground.blue, step),
        };
        if contrast_ratio(candidate, background) >= 4.5 {
            return candidate;
        }
    }
    Rgb {
        red: u8::MAX,
        green: u8::MAX,
        blue: u8::MAX,
    }
}

pub(super) fn lighten_channel(channel: u8, step: u16) -> u8 {
    let channel = u16::from(channel);
    let lightened = channel + (u16::from(u8::MAX) - channel) * step / 10;
    u8::try_from(lightened).expect("lightened color channel remains within u8")
}

pub(super) fn contrast_ratio(foreground: Rgb, background: Rgb) -> f64 {
    let foreground = relative_luminance(foreground);
    let background = relative_luminance(background);
    (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05)
}

pub(super) fn relative_luminance(color: Rgb) -> f64 {
    0.2126 * linear_channel(color.red)
        + 0.7152 * linear_channel(color.green)
        + 0.0722 * linear_channel(color.blue)
}

pub(super) fn linear_channel(channel: u8) -> f64 {
    let channel = f64::from(channel) / 255.0;
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

pub(super) fn clip_and_pad(
    spans: Vec<Span<'static>>,
    width: usize,
    padding_style: Style,
) -> Vec<Span<'static>> {
    let mut remaining = width;
    let mut clipped = Vec::new();
    for span in spans {
        if remaining == 0 {
            break;
        }
        let mut text = String::new();
        for character in span.content.chars() {
            let character_width = Span::raw(character.to_string()).width();
            if character_width > remaining {
                break;
            }
            text.push(character);
            remaining = remaining.saturating_sub(character_width);
        }
        clipped.push(Span::styled(text, span.style));
    }
    if remaining > 0 {
        clipped.push(Span::styled(" ".repeat(remaining), padding_style));
    }
    clipped
}

pub(super) fn pad_to_width(spans: &mut Vec<Span<'static>>, width: usize, padding_style: Style) {
    let used = spans.iter().map(Span::width).sum::<usize>();
    if used < width {
        spans.push(Span::styled(" ".repeat(width - used), padding_style));
    }
}

pub(super) fn gutter_style(kind: RowKind) -> Style {
    let foreground = match kind {
        RowKind::Removed => Color::LightRed,
        RowKind::Added => Color::LightGreen,
        RowKind::Conflict => Color::LightYellow,
        RowKind::Header | RowKind::Context | RowKind::Changed | RowKind::Meta => Color::DarkGray,
    };
    Style::default().fg(foreground).patch(diff_background(kind))
}

pub(super) fn diff_background(kind: RowKind) -> Style {
    match kind {
        // Use xterm-256 colors here instead of RGB. These survive SSH and terminal
        // multiplexers that advertise `xterm-256color` but filter true-color backgrounds.
        RowKind::Removed => Style::default().bg(Color::Indexed(52)),
        RowKind::Added => Style::default().bg(Color::Indexed(22)),
        RowKind::Conflict => Style::default().bg(Color::Indexed(58)),
        RowKind::Header | RowKind::Context | RowKind::Changed | RowKind::Meta => Style::default(),
    }
}

pub(super) fn diff_background_rgb(kind: RowKind) -> Option<Rgb> {
    match kind {
        RowKind::Removed => Some(Rgb {
            red: 95,
            green: 0,
            blue: 0,
        }),
        RowKind::Added => Some(Rgb {
            red: 0,
            green: 95,
            blue: 0,
        }),
        RowKind::Conflict => Some(Rgb {
            red: 95,
            green: 95,
            blue: 0,
        }),
        RowKind::Header | RowKind::Context | RowKind::Changed | RowKind::Meta => None,
    }
}

pub(super) fn row_style(kind: RowKind) -> Style {
    match kind {
        RowKind::Header => Style::default().fg(Color::Cyan),
        RowKind::Removed => Style::default().fg(Color::Red),
        RowKind::Added => Style::default().fg(Color::Green),
        RowKind::Conflict => Style::default()
            .fg(Color::LightYellow)
            .bg(Color::Indexed(58))
            .add_modifier(Modifier::BOLD),
        RowKind::Meta => Style::default().fg(Color::Yellow),
        RowKind::Context | RowKind::Changed => Style::default(),
    }
}

pub(super) fn network_animation_style(tick: usize) -> Style {
    const GRADIENT: [u8; 12] = [24, 25, 31, 37, 43, 42, 36, 30, 24, 60, 54, 53];
    Style::default()
        .fg(Color::Indexed(GRADIENT[(tick / 4) % GRADIENT.len()]))
        .add_modifier(Modifier::BOLD)
}
