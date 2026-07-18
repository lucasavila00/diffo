# ADR 0015: Special file states

Status: Accepted

## Decision

Render special Git states as useful file views.

- File picker colors: added/untracked green, modified yellow, deleted red and crossed
  out, renamed/copied cyan, conflicted red.
- Untracked and added files show the whole file as added lines.
- Conflicted files appear only in Changes with a bold red `U` marker.
- Conflicts show the whole worktree file. Conflict markers use `!`, bold yellow text,
  and xterm-256 background 58.
- Rename-only files show the whole file as neutral context lines. Keep syntax colors.
  Do not show red or green change backgrounds.
- Read staged rename content from the Git index. Do not leak unstaged edits.
- Binary files show a binary message. Preserve no-final-newline metadata.

## Tests

- Snapshot real untracked, conflicted, and renamed Git states.
- Create a real merge conflict and verify `U` plus all conflict markers in the TUI.
- Mock an empty rename and a rename with content.
- Select the content rename in a compiled TUI test and verify its body is visible.
