# Use lightweight language lexers

## Idea

A simple lexer that only recognizes the tokens Diffo colors could be much faster
than a complete grammar engine. Rust and JSON are useful first experiments because
they are already represented by generated stress files.

Keep syntect for other languages rather than committing to a custom lexer for every
format.

## What counts as a win

Rust and JSON top/deep window cases improve by at least 40%, fallback benchmarks do
not regress by more than 5%, and highlighted output passes syntax snapshots.
