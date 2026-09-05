use crate::diff::{
    Color, HighlightedDiff, HighlightedLine, Line, RenderLine, Rgb, RowKind, SideBySideRow, Span,
    Style, StyledSpan, terminal_safe_text,
};
use diffo_ui::theme;

pub(in crate::diff) fn inline_line(
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

pub(crate) fn raw_hunk_line(
    prefix: Option<char>,
    text: &str,
    kind: RowKind,
    highlighted: Option<&HighlightedLine>,
) -> Line<'static> {
    if matches!(kind, RowKind::Header | RowKind::Meta) {
        return Line::styled(terminal_safe_text(text), row_style(kind));
    }
    let mut spans = vec![Span::styled(
        prefix.unwrap_or(' ').to_string(),
        gutter_style(kind),
    )];
    let background = diff_background(kind);
    if let Some(highlighted) = highlighted {
        spans.extend(syntax_spans(highlighted, background, kind));
    } else {
        spans.push(Span::styled(
            terminal_safe_text(text),
            code_text_style(kind),
        ));
    }
    Line::from(spans)
}

pub(in crate::diff) fn inline_skeleton_line(row: &RenderLine) -> Line<'static> {
    Line::from(Span::styled(
        row.number
            .map_or_else(|| "       ".to_owned(), |number| format!("{number:>4}   ")),
        gutter_style(row.kind),
    ))
}

pub(in crate::diff) fn side_by_side_skeleton_line(
    row: &SideBySideRow,
    column_width: usize,
) -> Line<'static> {
    let number = |line: Option<&RenderLine>| {
        let text = line
            .and_then(|line| line.number)
            .map_or_else(|| "    ".to_owned(), |number| format!("{number:>4}"));
        match line {
            Some(line) => Span::styled(text, gutter_style(line.kind)),
            None => Span::raw(text),
        }
    };
    Line::from(vec![
        number(row.old.as_ref()),
        Span::raw(" ".repeat(column_width.saturating_sub(4))),
        Span::styled(" │ ", theme::chrome_style()),
        number(row.new.as_ref()),
    ])
}

pub(in crate::diff) fn side_by_side_line(
    row: &SideBySideRow,
    column_width: usize,
    horizontal: usize,
    highlighted: &HighlightedDiff,
) -> Line<'static> {
    let mut spans = format_cell(row.old.as_ref(), column_width, horizontal, highlighted);
    spans.push(Span::styled(" │ ", theme::chrome_style()));
    spans.extend(format_cell(
        row.new.as_ref(),
        column_width,
        horizontal,
        highlighted,
    ));
    Line::from(spans)
}

pub(in crate::diff) fn format_cell(
    line: Option<&RenderLine>,
    width: usize,
    horizontal: usize,
    highlighted: &HighlightedDiff,
) -> Vec<Span<'static>> {
    let gutter_width = usize::from(diffo_ui::design::SIDE_BY_SIDE_GUTTER_WIDTH);
    let code_width = width.saturating_sub(gutter_width);
    let Some(line) = line else {
        return vec![Span::styled(" ".repeat(width), theme::code_style())];
    };
    let number = line
        .number
        .map_or_else(|| "    ".to_owned(), |number| format!("{number:>4}"));
    let style = if matches!(line.kind, RowKind::Header | RowKind::Meta) {
        row_style(line.kind)
    } else {
        gutter_style(line.kind)
    };
    let mut spans = vec![Span::styled(format!("{number} "), style)];
    let code = if matches!(line.kind, RowKind::Header | RowKind::Meta) {
        vec![Span::styled(
            terminal_safe_text(&line.text),
            row_style(line.kind),
        )]
    } else {
        code_spans(line, highlighted)
    };
    spans.extend(clip_and_pad_scrolled(
        code,
        code_width,
        horizontal,
        diff_background(line.kind),
    ));
    spans
}

pub(in crate::diff) fn code_spans(
    row: &RenderLine,
    highlighted: &HighlightedDiff,
) -> Vec<Span<'static>> {
    let highlighted_line = row.number.and_then(|number| match row.kind {
        RowKind::Removed => highlighted.old.get(&number),
        RowKind::Added | RowKind::Context | RowKind::Changed => highlighted.new.get(&number),
        RowKind::Header | RowKind::Conflict | RowKind::Meta => None,
    });
    let background = diff_background(row.kind);
    highlighted_line.map_or_else(
        || {
            vec![Span::styled(
                terminal_safe_text(&row.text),
                code_text_style(row.kind),
            )]
        },
        |line| syntax_spans(line, background, row.kind),
    )
}

pub(in crate::diff) fn syntax_spans(
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

pub(in crate::diff) fn syntax_style(span: &StyledSpan, row_kind: RowKind) -> Style {
    let foreground = contrasting_foreground(span.foreground, row_kind);
    Style::default().fg(Color::Rgb(
        foreground.red,
        foreground.green,
        foreground.blue,
    ))
}

pub(in crate::diff) fn contrasting_foreground(foreground: Rgb, row_kind: RowKind) -> Rgb {
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

pub(in crate::diff) fn lighten_channel(channel: u8, step: u16) -> u8 {
    let channel = u16::from(channel);
    let lightened = channel + (u16::from(u8::MAX) - channel) * step / 10;
    u8::try_from(lightened).expect("lightened color channel remains within u8")
}

pub(in crate::diff) fn contrast_ratio(foreground: Rgb, background: Rgb) -> f64 {
    let foreground = relative_luminance(foreground);
    let background = relative_luminance(background);
    (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05)
}

pub(in crate::diff) fn relative_luminance(color: Rgb) -> f64 {
    0.2126 * linear_channel(color.red)
        + 0.7152 * linear_channel(color.green)
        + 0.0722 * linear_channel(color.blue)
}

pub(in crate::diff) fn linear_channel(channel: u8) -> f64 {
    let channel = f64::from(channel) / 255.0;
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

pub(in crate::diff) fn clip_and_pad(
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

pub(in crate::diff) fn clip_and_pad_scrolled(
    spans: Vec<Span<'static>>,
    width: usize,
    horizontal: usize,
    padding_style: Style,
) -> Vec<Span<'static>> {
    let mut skipped = horizontal;
    let spans = spans
        .into_iter()
        .filter_map(|span| {
            let text = span
                .content
                .chars()
                .skip_while(|character| {
                    if skipped == 0 {
                        return false;
                    }
                    skipped = skipped.saturating_sub(Span::raw(character.to_string()).width());
                    true
                })
                .collect::<String>();
            (!text.is_empty()).then(|| Span::styled(text, span.style))
        })
        .collect();
    clip_and_pad(spans, width, padding_style)
}

pub(in crate::diff) fn pad_to_width(
    spans: &mut Vec<Span<'static>>,
    width: usize,
    padding_style: Style,
) {
    let used = spans.iter().map(Span::width).sum::<usize>();
    if used < width {
        spans.push(Span::styled(" ".repeat(width - used), padding_style));
    }
}

pub(in crate::diff) fn gutter_style(kind: RowKind) -> Style {
    match kind {
        RowKind::Removed => Style::default().fg(theme::DIFF_REMOVED_FOREGROUND),
        RowKind::Added => Style::default().fg(theme::DIFF_ADDED_FOREGROUND),
        RowKind::Conflict => Style::default().fg(theme::CONFLICT_FOREGROUND),
        RowKind::Header | RowKind::Context | RowKind::Changed | RowKind::Meta => {
            theme::code_style().add_modifier(ratatui::style::Modifier::DIM)
        }
    }
    .patch(diff_background(kind))
}

pub(in crate::diff) fn diff_background(kind: RowKind) -> Style {
    match kind {
        // Use xterm-256 colors here instead of RGB. These survive SSH and terminal
        // multiplexers that advertise `xterm-256color` but filter true-color backgrounds.
        RowKind::Removed => Style::default().bg(Color::Indexed(52)),
        RowKind::Added => Style::default().bg(Color::Indexed(22)),
        RowKind::Conflict => Style::default().bg(theme::CONFLICT_BACKGROUND),
        RowKind::Header | RowKind::Context | RowKind::Changed | RowKind::Meta => {
            Style::default().bg(theme::CODE_BACKGROUND)
        }
    }
}

pub(in crate::diff) fn diff_background_rgb(kind: RowKind) -> Option<Rgb> {
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

pub(in crate::diff) fn row_style(kind: RowKind) -> Style {
    match kind {
        RowKind::Header | RowKind::Context | RowKind::Changed => theme::code_style(),
        RowKind::Removed => Style::default().fg(Color::Red),
        RowKind::Added => Style::default().fg(Color::Green),
        RowKind::Conflict => Style::default()
            .fg(theme::CONFLICT_FOREGROUND)
            .bg(theme::CONFLICT_BACKGROUND),
        RowKind::Meta => theme::code_style().fg(theme::WARNING),
    }
}

fn code_text_style(kind: RowKind) -> Style {
    theme::code_style().patch(diff_background(kind))
}
