# diffo-ui

`diffo-ui` contains terminal UI primitives and components shared across Diffo
activities.

It provides pane geometry, bounded scroll and scrollbar math, shared scrollbar
rendering, syntax-ready prepared viewport transitions, direction-neutral syntax
window placement, bounded syntax-coverage retention, terminal-safe text, fixed
structural and interactive styles, a shared Nerd Font icon vocabulary, and
semantic layout tokens for widths, heights, insets, gaps, and overlay bounds. It
also owns the fixed file-icon table, shared command palette, reusable searchable
modal picker, file picker, and read-only text view. Diffo requires a terminal
font with Nerd Font symbols. Callers own typed picker items, command execution,
file activation, content loading, and activity state.

Activity-specific behavior remains in `diffo-app`. Semantic content, diff
content, and syntax colors remain owned by their content renderers.
