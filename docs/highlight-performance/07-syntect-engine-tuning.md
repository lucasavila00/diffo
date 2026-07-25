# Tune the syntect engine

## Idea

Syntect offers different build features and uses a regular-expression engine to
match language rules. The bat-compatible bundle also contains many languages.

Profile those parts separately, then test whether a different supported regex setup
or a smaller bundle reduces work while keeping the languages Diffo promises.

## What counts as a win

The geometric mean of all `window/*` cases improves by at least 15%, initialization
does not regress, and the existing curated syntax coverage remains intact.
