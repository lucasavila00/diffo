# ADR 0058: Match file buffer titles to picker labels

Refines [ADR 0049](0049-shared-file-picker.md) and
[ADR 0050](0050-file-picker-status-colors.md).

## Context

A file can have one label in the picker and another in the file buffer title.
This hides status and makes the file look different after opening it.

## Decision

The file buffer title is the opened file's picker label. Same text. Same spans.
Same foreground, background, and modifiers.

Use the label from the committed picker row. Do not rebuild it from the path. Do
not include picker-owned selection marks, indentation, disclosure marks, or
actions.

## Consequences

The file keeps one identity and one format from picker to buffer. Label changes
must update both together.

## Verification

- Open each file status and compare the picker label with the buffer title.
- Verify async file loading never pairs content with another row's label.
- Run `make all`.
