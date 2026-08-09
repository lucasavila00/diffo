# ADR 0064: Separate Diff status and Explorer disclosure columns

Refines [ADR 0050](0050-file-picker-status-colors.md),
[ADR 0054](0054-readable-tree-labels-and-controls.md),
[ADR 0061](0061-align-explorer-tree-names.md), and
[ADR 0063](0063-terminal-native-file-type-badges.md).

## Context

Diff and Explorer have different space and navigation needs. Diff shows only
changed files, so a Git-status letter is useful on every row. Explorer shows the
whole tree, where explicit folder disclosure and compact indentation are more
valuable.

After adding file-type icons, Diff's leading dot duplicates the icon as a
persistent row affordance. Explorer's Git-status column consumes space while an
icon-only folder state is less explicit than the previous disclosure caret.

## Decision

Diff flat rows have a two-cell Git-status column and no generic leading dot:

```text
M main.rs
A package.json
```

The first cell is the status letter and the second is a space. The file icon
follows immediately before the path. Existing Git-status colors and modifiers
remain.

Explorer tree rows have two cells of indentation per depth followed by a
two-cell disclosure column:

```text
▸ src
▾ src
  main.rs
```

Closed folders use `▸`, expanded folders use `▾`, and files use two spaces. One
fixed folder icon identifies directories; only the caret communicates expansion.
Explorer tree rows and viewer titles do not show Git-status letters, colors, or
modifiers. Internal Git state and viewer gutter markers remain unchanged.

Icons remain directly adjacent to names. Names at the same depth align because
every row reserves the same disclosure width. Selection, row hit targets,
context menus, truncation, and fixed actions do not change. No hover behavior or
configuration is added.

This replaces ADR 0054's flat-row dot, ADR 0061's Explorer status column, and
ADR 0063's open and closed folder icons. It narrows ADR 0050's status decoration
rule to Diff's flat pickers.

## Consequences

- Diff gains the two cells previously used by the dot and separator, then spends
  two cells consistently on Git status.
- Explorer gains two cells by removing its status column while restoring
  explicit disclosure state.
- Explorer no longer communicates Git state in tree labels; Diff remains the
  dedicated changed-file view.

## Verification

- Assert Diff rows have a two-cell status column, no dot, and preserved status
  style.
- Assert Explorer uses `▸`, `▾`, and a two-space file disclosure column.
- Assert one folder icon is stable across expansion and names align at every
  depth.
- Assert Explorer tree labels and viewer titles have no Git-status decoration.
- Test narrow truncation, row actions, mouse selection, and expansion
  transitions.
- Run `make all` when implementing the ADR.
