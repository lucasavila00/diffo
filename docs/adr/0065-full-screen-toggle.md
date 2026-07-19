# ADR 0065: Full-screen buffer toggle

Status: Accepted

Refines [ADR 0043](0043-shared-text-buffer-view.md).

## Goal

Make buffer text easy to copy from the terminal. Keep useful buffer styling. Remove
outer UI text and layout marks from the copied lines.

## Decision

Lowercase `f` toggles full-screen mode for the active text buffer.

The normal buffer has a Nerd Font `` button in its top-right border. A short
horizontal line separates the buffer label from the button. Diff shows only the
view mode there, not a `File Diff` label. Clicking the button enters full-screen
mode. It uses the same prepared transition as `f`.

Diff uses the open diff buffer. Explorer uses the open file buffer. If no text
buffer is open, `f` does nothing.

Full-screen mode has two parts:

1. One header row at the top.
2. The text buffer in every row below it.

The header reuses the committed buffer title on the left. Keep the same text, file
icon, Git status, colors, and modifiers shown on the normal buffer. Do not rebuild
the title from the path. Show an `X` button on the right. Clicking `X` exits
full-screen mode. `X` is not a keyboard shortcut.

The full-screen buffer has no workbench decoration:

- no border;
- no title inside the buffer;
- no line numbers;
- no gutters;
- no status row;
- no activity bar;
- no file pane;
- no loading, selection, or resize markers over the text.

Keep syntax highlighting. Styling does not add copied characters.

Explorer shows the committed file lines. Keep the same syntax highlighting. Keep
the normal vertical and horizontal scroll controls. Do not show Explorer line
numbers, change gutters, tree state, or file actions.

Diff shows raw unified hunks. Show hunk headers, context lines, removed lines, added
lines, and hunk metadata. Keep the current diff backgrounds and syntax highlighting.
Do not show the inline line-number projection or the side-by-side projection. Keep
the normal vertical and horizontal scroll controls. Do not show change buttons or
the hunk-marker rail.

Keep buffer characters. Do not add characters for layout or state. Scroll controls
use their own edge cells and are not part of buffer lines.

Dragging buffer cells does not change application state. Mouse capture remains on
for `X`, wheel, and scrollbar input. Use the terminal's selection modifier when the
terminal requires one for copy.

Arrow, page, wheel, and scrollbar scrolling still work. Entering and leaving
full-screen mode keeps the same open buffer and scroll position. The change happens
in one frame.

Do not expose new rows without their committed syntax coverage. Keep the previous
screen until the full visible range is ready.

Pressing lowercase `f` again exits. Uppercase `F` does nothing. An open prompt,
palette, menu, or text input keeps input priority. In text input, `f` remains text.

## Verification

- Press `f` in Explorer. The header, syntax-highlighted file text, and scroll
  controls remain. Workbench chrome, line numbers, and gutters disappear.
- Click `` on the normal Explorer or Diff buffer. Full-screen mode opens.
- Press `f` in Diff. The header, styled raw hunks, and scroll controls remain.
  Inline and side-by-side layout marks disappear.
- Press `f` again. The normal page returns with the same buffer and scroll position.
- Click `X`. The normal page returns.
- Compare the normal and full-screen titles. Text and style are identical.
- Copy several buffer lines. The copy contains only buffer characters.
- Test syntax and diff styles in both modes.
- Test arrow, page, wheel, and scrollbar scrolling.
- Test that borders, gutters, marker rails, change buttons, and status text are
  absent.
- Test atomic entry with delayed syntax coverage.
- Test modal and text-input priority.
- Test that uppercase `F` is not a shortcut.
- Run `make all` when implementing this ADR.
