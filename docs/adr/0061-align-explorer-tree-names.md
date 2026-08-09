# ADR 0061: Align Explorer tree names with a fixed status column

Refines [ADR 0035](0035-explorer-file-view.md),
[ADR 0049](0049-shared-file-picker.md), and
[ADR 0050](0050-file-picker-status-colors.md).

## Context

Files have a two-cell status prefix. Folders do not.

Sibling file and folder names do not line up. The file looks nested under the
folder.

Keep the status letter. It shows Git status without relying on color.

## Decision

Give every Explorer row a two-cell status column.

- Changed file: status letter and one space.
- Unchanged file: two spaces.
- Folder: two spaces.

Names at the same depth now start in the same column.

Explorer owns this column. The shared picker still owns tree depth and folder
arrows.

## Consequences

- Sibling names line up.
- Status letters stay.
- Folder names lose two cells of available width.
- The shared picker does not change.

## Alternatives

- Remove status letters. Rejected. Status must work without color.
- Change tree indentation by entry kind. Rejected. Same depth must mean same
  indent.
- Teach the shared picker about Git. Rejected. Explorer owns Git labels.

## Acceptance

- Sibling file and folder names start in the same column at every depth.
- Status letters and folder arrows still work.
- Narrow names still truncate with `...`.
- File buffer titles still match file labels.
- `make all` passes.
