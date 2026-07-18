# ADR 0013: Commands and file actions

Status: Accepted

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

## Git commands

The palette has exactly two commands:

- `Git: Fetch`
- `Git: Pull`

Enter runs the selected command. Run Git through the repository refresh worker. Do
not block input or rendering. Disable terminal prompts. Refresh the snapshot after
the command. Show failures in the status bar.

## File actions

Show an action at the right of every writable file row:

- `[+]` stages an unstaged file.
- `[-]` unstages a staged file.

Clicking the action does not depend on selection. Clicking the rest of the row only
selects it. Hide actions in read-only mode.

The Changes header shows `[+] Stage All`. Only `+` is clickable. The Staged header
shows `[-] Unstage All`. Only `-` is clickable. Labels and brackets are not buttons.

Mock remote failures name the received action, for example:

```text
mock repository cannot execute Fetch: no remote configured
```

This proves the command reached the repository effect boundary.

## Watch refresh and scroll

Do not reset scroll for every watcher snapshot.

Preserve vertical and horizontal scroll when the selected `FileKey` and its diff are
unchanged. Reset scroll when selection changes or the selected diff changes.

## Regression tests

- Fetch and pull use a real local remote.
- Palette Enter produces the correct repository action.
- Mouse clicks produce stage and unstage actions for the clicked row.
- A real watcher refresh caused by an ignored file preserves scroll.
- Changing the selected diff resets scroll.
- Live integration tests have one hard deadline and finish in under 10 seconds.
