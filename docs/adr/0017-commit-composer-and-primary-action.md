# ADR 0017: Commit composer

## UI

Put a commit composer above the Staged and Changes lists.

- The bordered field has a permanent `Commit message` title.
- Empty input shows `Update 1 file` or `Update N files` in the disabled style
  when files are staged. With no staged files it shows `Type a message…`.
- Clicking Commit with the generated placeholder uses that text as the commit
  message. Typed text overrides it.
- Typed text uses normal foreground color.
- Ctrl+C always quits. Esc leaves the input. Enter runs Commit when enabled.
- Passive repository watcher refreshes keep the input focused and preserve typed
  text.

Commit and Sync are separate fixed controls under ADR 0071. The composer never
changes into a network action.

## Effects

Commit uses non-interactive `git commit -m`.

Keep the commit message while Commit is running. Clear it only after a
successful snapshot refresh. Preserve it when the operation fails.

Mock errors include the received action. The mutable mock can create a local
mock commit.
