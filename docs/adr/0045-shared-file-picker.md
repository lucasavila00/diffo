# ADR 0045: One file picker

Status: Accepted

## Problem

Diff and Explorer have separate file pickers. They duplicate selection, scrolling,
mouse handling, rendering, and menus. They already behave differently.

## Decision

Create a `diffo-file-picker` crate. Diff, Explorer, and future file pickers must use
it. Delete private picker implementations.

The shared picker owns:

- selection and keeping it visible;
- previous, next, first, last, click, and activate;
- offset, bounds, wheel scrolling, and a vertical scrollbar;
- row layout, clipping, styles, hit targets, empty/loading/error states;
- the fixed key and mouse mappings; and
- the contextual menu, including copy relative path and copy absolute path.

Keys, mouse behavior, scroll distance, visuals, and menus are identical everywhere.
Use the existing Diff file-navigation bindings as the initial shared bindings.

Each picker is one panel. There are two modes:

- Flat: Diff creates two unrelated picker instances, one for Staged and one for
  Changes.
- Tree: Explorer supplies hierarchy data. The picker owns indentation, disclosure,
  expansion, and collapse.

Mode is the only behavior difference.

Activities supply stable row IDs, labels, status styles, panel titles, and domain
actions. The picker returns generic selection, activation, disclosure, menu, row
action, and panel-action outcomes. Diff still owns staging and diff preparation.
Explorer still owns path loading, status projection, and file loading. Neither owns
picker behavior.

Commit rows, tree projection, selection, offset, bounds, and hit targets together
during frame preparation. Rendering reads committed state only. Stale Explorer path
results cannot change the picker.

This replaces the selection, expansion, scroll, and tree-rendering ownership in ADR
0035. Explorer remains an independent tool.

`diffo-file-picker` may depend on `diffo-ui`, Ratatui, and Crossterm. It must not
depend on Diff, Explorer, Git, or application models.

## Acceptance

- Diff uses two flat picker instances. Explorer uses one tree picker instance.
- All instances use the same state, renderer, input path, and context menu. Only
  flat/tree projection differs.
- Shared tests cover navigation, selection visibility, scrolling, scrollbar and
  action hit targets, menus, empty areas, and narrow terminals.
- Diff tests cover two independent flat panels. Tree tests cover expand/collapse and
  stable selection.
- A delayed PTY test proves Explorer path replacement is atomic and rejects stale
  results.
- Tests reject uppercase shortcuts and private picker key tables.
