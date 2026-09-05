# ADR 0015: Special file states

## Decision

Render special Git states as useful file views.

- File picker colors: added/untracked green, modified yellow, deleted red and
  crossed out, renamed/copied cyan, conflicted red.
- Untracked and added files show the whole file as added lines.
- Conflicted files appear only in Changes with a red `U` marker. Semantic state
  alone does not add bold; bold is reserved for mouse affordances.
- Conflicts show the whole worktree file. Conflict markers use `!`, bold yellow
  text, and xterm-256 background 58.
- Rename-only files show the whole file as neutral context lines. Keep syntax
  colors. Do not show red or green change backgrounds.
- Read staged rename content from the Git index. Do not leak unstaged edits.
- Binary files show a binary message. Preserve no-final-newline metadata.
