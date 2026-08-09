# ADR 0017: Commit composer and primary action

## UI

Put a commit composer above the Staged and Changes lists.

- The bordered field has a permanent `Commit message` title.
- Empty input shows `Update 1 file` or `Update N files` in placeholder dark gray
  when files are staged. With no staged files it shows `Type a message…`.
- Clicking Commit with the generated placeholder uses that text as the commit
  message. Typed text overrides it.
- Typed text uses normal foreground color.
- Ctrl+C always quits. Esc leaves the input. Enter runs the enabled primary
  action.
- Passive repository watcher refreshes keep the input focused and preserve typed
  text.
- Read-only mode shows the composer but never enables mutations.

Show one primary button. The pure app model chooses its state:

1. Staged files: enabled `Commit`. Use typed text or the generated message.
2. Ahead and behind: disabled `Push + Pull`.
3. Behind: enabled `Pull`.
4. Ahead: enabled `Push`.
5. Otherwise: disabled `Commit`.

Commit wins over sync state when it is ready. Do not guess how to resolve
divergence. The button has a two-row body and one blank row before the file
groups.

## Effects

Add `Commit(String)` and `Push` repository actions. Keep Pull as the existing
action. Git uses non-interactive `git commit -m`, `git push`, and
`git pull --no-edit`.

Keep the commit message while Commit is running. Clear it only after a
successful snapshot refresh. Preserve it when the operation fails.

Keep Commit, Push, and Pull disabled while their repository effect is pending.
Keep the current label visible so the user can see which action is running.

Mock errors include the received action. The mutable mock can create a local
mock commit; remote sync still reports that no remote is configured.
