# ADR 0112: Give folders the file path menu

Refines [ADR 0049](0049-shared-file-picker.md) and
[ADR 0058](0058-safer-path-menu.md).

## Problem

Explorer files expose a path menu that copies either their absolute path or
their worktree-relative path. Explorer folders already have stable
worktree-relative paths, but the shared picker disables context menus for every
tree branch. Right-clicking a folder is therefore consumed without opening a
menu, and `c` cannot open the menu for a selected folder.

This makes the same path-valued rows behave differently based on whether they
are files or folders. Copying a folder path requires copying a file path and
then editing it outside Diffo.

## Decision

Give every folder shown in Explorer the same path menu as an Explorer file.
Right-click selects the folder and opens the menu. Lowercase `c` toggles the
menu for the selected folder. The menu retains the existing actions, spacing,
keys, and dismissal behavior:

- `[a] Copy absolute path`;
- one inert row; and
- `[r] Copy relative path`.

Treat path-menu eligibility as row behavior, independent of whether a tree row
is a branch or a leaf. Explorer enables that behavior for both files and
folders; other file-picker users keep choosing eligibility for their own rows.
Do not add a folder-specific menu, outcome, command, or clipboard path.

Both folder actions pass the folder's existing worktree-relative path through
the same copy-path outcome and application effect used by files. Relative copy
uses that path directly. Absolute copy resolves it against the repository root
at the existing clipboard boundary.

Opening or using the path menu does not expand or collapse a folder. Left-click
and `Enter` retain their existing disclosure behavior. The fixed lowercase
shortcuts and Help entry remain shared with the file menu.

## Alternatives

- Add folder-specific menu state and copy events. Rejected because the actions
  and path semantics are identical and would duplicate the shared picker and
  workbench flow.
- Copy a child file path and derive its parent. Rejected because it does not
  address the selected folder directly and makes the result depend on a visible
  child.
- Support folders only through right-click. Rejected because ADR 0058 provides
  keyboard access for terminals and SSH sessions where mouse input is weak.

## Consequences

Explorer files and folders have one consistent path-copy interaction. The shared
tree document must carry context-menu eligibility separately from its branch
flag, but the rendered menu and clipboard integration remain unchanged.

Folders continue to exist only where Explorer already presents them; this
decision does not add empty directories or change filesystem discovery. Diff's
flat file rows and folder disclosure behavior are unaffected.
