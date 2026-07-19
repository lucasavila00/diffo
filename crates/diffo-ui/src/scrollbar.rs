use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::Style,
    symbols::{block, line},
    widgets::ScrollbarOrientation,
};

use crate::maximum_scroll;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ScrollbarGeometry {
    thumb_start: usize,
    thumb_length: usize,
}

/// Renders a scrollbar with a thumb whose rounded length is independent of its position.
///
/// The scrollbar uses Ratatui's double-line track and full-block thumb without arrow symbols.
pub fn render_scrollbar(
    frame: &mut Frame,
    area: Rect,
    orientation: &ScrollbarOrientation,
    content_length: usize,
    viewport_length: usize,
    position: usize,
    style: Style,
) {
    let Some((track_start, track_length)) = track(area, orientation) else {
        return;
    };
    let geometry = scrollbar_geometry(track_length, content_length, viewport_length, position);
    let vertical = orientation.is_vertical();
    let buffer = frame.buffer_mut();
    for offset in 0..track_length {
        let in_thumb = offset >= geometry.thumb_start
            && offset < geometry.thumb_start.saturating_add(geometry.thumb_length);
        let symbol = if in_thumb {
            block::FULL
        } else if vertical {
            line::DOUBLE_VERTICAL
        } else {
            line::DOUBLE_HORIZONTAL
        };
        let offset = u16::try_from(offset).unwrap_or(u16::MAX);
        let position = if vertical {
            Position::new(track_start.x, track_start.y.saturating_add(offset))
        } else {
            Position::new(track_start.x.saturating_add(offset), track_start.y)
        };
        if let Some(cell) = buffer.cell_mut(position) {
            cell.set_symbol(symbol).set_style(style);
        }
    }
}

fn track(area: Rect, orientation: &ScrollbarOrientation) -> Option<(Position, usize)> {
    if area.is_empty() {
        return None;
    }
    let (start, length) = match orientation {
        ScrollbarOrientation::VerticalRight => (
            Position::new(area.right().saturating_sub(1), area.y),
            area.height,
        ),
        ScrollbarOrientation::VerticalLeft => (Position::new(area.x, area.y), area.height),
        ScrollbarOrientation::HorizontalBottom => (
            Position::new(area.x, area.bottom().saturating_sub(1)),
            area.width,
        ),
        ScrollbarOrientation::HorizontalTop => (Position::new(area.x, area.y), area.width),
    };
    Some((start, usize::from(length)))
}

fn scrollbar_geometry(
    track_length: usize,
    content_length: usize,
    viewport_length: usize,
    position: usize,
) -> ScrollbarGeometry {
    if track_length == 0 {
        return ScrollbarGeometry {
            thumb_start: 0,
            thumb_length: 0,
        };
    }

    let thumb_length = if content_length == 0 || viewport_length >= content_length {
        track_length
    } else {
        rounded_ratio(viewport_length, track_length, content_length).clamp(1, track_length)
    };
    let maximum = maximum_scroll(content_length, viewport_length);
    let available_travel = track_length.saturating_sub(thumb_length);
    let thumb_start = if maximum == 0 {
        0
    } else {
        rounded_ratio(position.min(maximum), available_travel, maximum).min(available_travel)
    };

    ScrollbarGeometry {
        thumb_start,
        thumb_length,
    }
}

fn rounded_ratio(value: usize, scale: usize, divisor: usize) -> usize {
    let value = u128::try_from(value).unwrap_or(u128::MAX);
    let scale = u128::try_from(scale).unwrap_or(u128::MAX);
    let divisor = u128::try_from(divisor).unwrap_or(u128::MAX);
    let rounded = value.saturating_mul(scale).saturating_add(divisor / 2) / divisor;
    usize::try_from(rounded).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumb_length_is_stable_at_every_legal_position() {
        for track_length in 1..=24 {
            for content_length in 1..=48 {
                for viewport_length in 1..=content_length {
                    let maximum = content_length - viewport_length;
                    let expected =
                        scrollbar_geometry(track_length, content_length, viewport_length, 0)
                            .thumb_length;
                    for position in 0..=maximum {
                        assert_eq!(
                            scrollbar_geometry(
                                track_length,
                                content_length,
                                viewport_length,
                                position,
                            )
                            .thumb_length,
                            expected,
                            "track={track_length}, content={content_length}, viewport={viewport_length}, position={position}",
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn thumb_reaches_both_track_endpoints_in_both_axes() {
        for orientation in [
            ScrollbarOrientation::VerticalRight,
            ScrollbarOrientation::HorizontalBottom,
        ] {
            let area = if orientation.is_vertical() {
                Rect::new(2, 3, 1, 17)
            } else {
                Rect::new(2, 3, 17, 1)
            };
            let (_, track_length) = track(area, &orientation).expect("non-empty track");
            let first = scrollbar_geometry(track_length, 100, 20, 0);
            let last = scrollbar_geometry(track_length, 100, 20, 80);

            assert_eq!(first.thumb_start, 0);
            assert_eq!(last.thumb_start + last.thumb_length, track_length);
        }
    }

    #[test]
    fn geometry_handles_minimum_empty_oversized_and_clamped_inputs() {
        assert_eq!(
            scrollbar_geometry(3, 10_000, 1, 0),
            ScrollbarGeometry {
                thumb_start: 0,
                thumb_length: 1,
            }
        );
        assert_eq!(
            scrollbar_geometry(0, 100, 20, 80),
            ScrollbarGeometry {
                thumb_start: 0,
                thumb_length: 0,
            }
        );
        assert_eq!(
            scrollbar_geometry(7, 5, 10, usize::MAX),
            ScrollbarGeometry {
                thumb_start: 0,
                thumb_length: 7,
            }
        );
        assert_eq!(
            scrollbar_geometry(10, 100, 20, usize::MAX),
            scrollbar_geometry(10, 100, 20, 80),
        );
        assert!(track(Rect::new(0, 0, 0, 10), &ScrollbarOrientation::VerticalRight).is_none());
        assert!(
            track(
                Rect::new(0, 0, 10, 0),
                &ScrollbarOrientation::HorizontalBottom,
            )
            .is_none()
        );
    }
}
