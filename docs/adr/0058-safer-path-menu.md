# ADR 0058: Safer path menu

## Problem

The path menu has two actions:

- Copy absolute path.
- Copy relative path.

The actions touch. One row apart. Easy to click the wrong one. Then the wrong
path goes to the clipboard.

Right-click is the only way to open the menu. This is weak over SSH. Some
terminals have poor mouse support. Keyboard users also need a way in.

The file actions have no keyboard shortcuts. The menu cannot teach keys that do
not exist.

## Options

Keep the menu as-is. Small change. Problem stays.

Make each action taller. Bigger target. But the targets still touch. A click
near the edge can still run the wrong action.

Put dead space between the actions. This makes the menu one row taller. A click
between actions copies nothing. Safer.

Add one direct copy key. Fast. But there are two path types. One key cannot make
that choice clear.

Add a key that opens the menu. Same choice for mouse and keyboard. Easy to
explain.

Add one key for each action inside the open menu. Clear choice. No clash with
normal Diff keys. The menu can show the key beside each action.

## Decision

Right-click opens the path menu.

Lowercase `c` toggles the same menu for the selected file. Press once to open.
Press again to close. `Esc` also closes the menu. The menu title shows `[c]`.
The Help screen also lists `c`.

Each file action has its own key:

- `[a] Copy absolute path`.
- `[r] Copy relative path`.

These keys work while the menu is open. They run the shown action and close the
menu. Outside the menu, existing `a` and `r` behavior stays the same.

Put one blank row between the two actions. The blank row is not an action.
Clicking it closes the menu and copies nothing.

Keep the key fixed. No config. No uppercase shortcut.

## Result

The menu is one row taller. The actions no longer touch. A near miss does not
copy the wrong path.

Mouse and keyboard open the same menu. The menu teaches its open key and both
file action keys.

Tests cover the blank row, mouse actions, action keys, visible keys, `c` toggle,
`Esc` close, Diff, Explorer, and uppercase rejection.
