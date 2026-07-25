# Replace syntect with tree-sitter

## Idea

Tree-sitter builds a syntax tree instead of applying TextMate-style regular
expressions. It can update an existing tree when text changes, which may make later
viewport jumps cheaper.

Build a small backend for the four benchmark languages before considering a wider
migration. Include grammar loading and query work when judging the real cost.

## What counts as a win

The geometric mean of all `window/*` cases improves by at least 30%, every language
in the harness remains supported, and no individual case regresses by more than 5%.
