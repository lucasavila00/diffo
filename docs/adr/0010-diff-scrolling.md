# ADR 0010: Diff scrolling

Status: Accepted

## Controls

- Up, Down, and mouse wheel move four lines in their natural direction.
- Page Up and Page Down move one visible page.
- Left, Right, and `d` move four columns. Lowercase `a` stays stage-all toggle.
- `a` stages all when unstaged files exist. It unstages all when none exist.

Keep key definitions and help text in the binding registry.

## Scrollbars

Show a vertical scrollbar when rows overflow. Show a horizontal scrollbar in inline
mode when columns overflow.

- Click the track to jump.
- Drag the track to scroll.
- Put both tracks one cell inside the diff pane.
- Do not use the terminal edge. Ghostty can draw its own scrollbar there.

The renderer owns scrollbar geometry. It turns mouse positions into absolute scroll
messages. The app model owns the scroll positions.

## Terminal

Use Ratatui's alternate screen. Purge that buffer once at startup. Do not clear every
frame.
