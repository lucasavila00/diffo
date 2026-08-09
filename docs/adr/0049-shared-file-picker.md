# ADR 0049: One file picker

## Problem

Diff and Explorer duplicate picker state, input, rendering, scrolling, and
menus.

## Decision

All file panels use `diffo-file-picker`. It owns:

- selection and navigation;
- offset, bounds, wheel, and scrollbar math;
- layout, rendering, clipping, and hit targets;
- fixed keys and mouse behavior; and
- one context menu with copy-relative and copy-absolute.

Two modes. Only projection differs:

- Flat. Diff creates two unrelated instances: Staged and Changes.
- Tree. Explorer creates one instance. The picker owns disclosure and
  indentation.

Activities supply IDs, labels, styles, titles, and domain actions. Diff keeps
staging and diff preparation. Explorer keeps path and file loading.

Scrolling follows ADR 0050.

Prepare rows, selection, offsets, bounds, and hit targets together. Render
committed state only. Stale Explorer results cannot change the picker.

`diffo-file-picker` may depend on `diffo-ui`, Ratatui, and Crossterm. Not Diff,
Explorer, Git, or app models.

Replaces picker ownership in ADRs 0033 and 0035.

## Acceptance

- Diff: two flat instances. Explorer: one tree instance.
- Same state, input, rendering, menu, wheel speed, and scrollbar core.
- Different main-buffer scrollbar rendering keeps hunk markers separate.
- Tests cover both modes, independent offsets, menus, actions, bounds, and
  uppercase.
- Delayed PTY rejects stale Explorer file results.
