# ADR 0019: Commit message modal

Status: Accepted

## Problem

The small inline field has unclear focus. Clicking elsewhere does not leave edit
mode. The field is too narrow for reviewing a message.

## Decision

Keep the inline field as a message preview and launcher. Do not edit text inline.

Clicking it opens a centered commit-message modal. The modal has:

- a wide text field and visible cursor;
- the generated `Update N files` placeholder;
- `Commit` and `Cancel` buttons;
- `Enter: commit` and `Esc: cancel` help;
- normal global `Ctrl+C: quit` behavior.

The modal owns text input while open. File navigation and staging keys do nothing.
Clicking outside the modal closes it, same as Esc. Closing keeps the draft so the
user can reopen it. A successful commit clears the draft.

Repository watcher refreshes must not close the modal, move its cursor, or change
its draft. Read-only mode may show the message but cannot open an editable modal.

## State

Put modal state in the pure app model:

```text
CommitEditor: Closed | Open { draft, cursor }
```

Input mapping emits open, edit, submit, and close messages. The TUI only renders
the state. Repository effects stay outside both layers.

## Tests

- Pure tests cover open, edit, outside-click close, Esc, submit, and draft restore.
- Input tests prove the modal captures typing but never Ctrl+C.
- Compiled PTY tests open the modal by mouse, type, close by outside click, reopen,
  submit, and verify the real Git commit message.
- A compiled PTY test changes the repository while editing and verifies focus,
  cursor, and draft stay unchanged.
- Every E2E wait keeps the existing five-second timeout.
