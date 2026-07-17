# ADR 0008: Diff colors

Status: Accepted

## Decision

Use the bat syntax set and Monokai Extended colors for code.

Do not use theme bold, italic, or underline. Terminal fonts can make these too loud.
Use syntax foreground colors only.

Changed rows use xterm-256 backgrounds:

- Removed: red, color 52.
- Added: green, color 22.

Fill the full row in inline mode. Fill the full old or new cell in side-by-side
mode. Make `-` bright red and `+` bright green.

## Contrast

Monokai expects a dark plain background. Some colors, such as comments, are hard to
read on red or green.

Keep a syntax color when its contrast is at least 4.5:1. Otherwise lighten it until
it reaches 4.5:1. Do this only on added and removed rows. Do not change context-row
colors.

## Reason

- Works over SSH with `TERM=xterm-256color`.
- Keeps added and removed rows obvious.
- Keeps comments and dark tokens readable.
- Keeps syntax color without letting the theme hide text.
