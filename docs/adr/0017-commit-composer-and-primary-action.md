# ADR 0017: Commit composer and primary action

Status: Accepted

## UI

Put a commit composer above the Staged and Changes lists.

- The bordered field has a permanent `Commit message` title.
- Empty input shows `Type a message…` in dark gray.
- Typed text uses normal foreground color.
- Ctrl+C always quits. Esc leaves the input. Enter runs the enabled primary action.
- Read-only mode shows the composer but never enables mutations.

Show one primary button. The pure app model chooses its state:

1. Staged files plus a non-empty message: enabled `Commit`.
2. Ahead and behind: disabled `Push + Pull`.
3. Behind: enabled `Pull`.
4. Ahead: enabled `Push`.
5. Otherwise: disabled `Commit`.

Commit wins over sync state when it is ready. Do not guess how to resolve divergence.

## Effects

Add `Commit(String)` and `Push` repository actions. Keep Pull as the existing action.
Git uses non-interactive `git commit -m`, `git push`, and `git pull --no-edit`.

Keep the commit message while Commit is running. Clear it only after a successful
snapshot refresh. Preserve it when the operation fails.

Mock errors include the received action. The mutable mock can create a local mock
commit; remote sync still reports that no remote is configured.

## Tests

- Pure tests cover Commit, Push, Pull, disabled, and Push + Pull states.
- A compiled PTY test types a message, commits, sees Push, pushes, and verifies the
  remote HEAD.
- A focused commit field cannot consume Ctrl+C.
- Coordinate tests cover the input and enabled/disabled button.
