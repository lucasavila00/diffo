# ADR 0013: Commands and file actions

## Footer and overlays

Keep the footer small:

```text
1/f1: commands  2/f2: help
```

- `1` and F1 open the command palette.
- `2` and F2 toggle the help panel.
- Esc closes either overlay.
- The palette stays at 20% from the top. Filtering does not move it.
- Keyboard and mouse can select palette rows.

## Commands

The palette presents the fixed command catalog owned by the workbench and the
operation-specific ADRs. Enter runs the selected command. Run repository work
through the serialized repository service worker. Do not block input or
rendering or give Git the terminal. Network prompts use the typed askpass path
from ADR 0053. Refresh the snapshot after commands. Show failures in the shared
acknowledgement modal.

## File actions

Show an action at the right of every writable file row:

- `[+]` stages an unstaged file.
- `[-]` unstages a staged file.

Clicking the action does not depend on selection. Clicking the rest of the row
only selects it.

The Changes header shows `[+] Stage All`. Only `+` is clickable. The Staged
header shows `[-] Unstage All`. Only `-` is clickable. Labels and brackets are
not buttons.

Mock remote failures name the received action, for example:

```text
mock repository cannot execute Fetch: no remote configured
```

This proves the command reached the repository effect boundary.

## Watch refresh and scroll

Do not reset scroll for every watcher snapshot.

Preserve vertical and horizontal scroll when the selected `FileKey` and its diff
are unchanged. Reset scroll when selection changes or the selected diff changes.
