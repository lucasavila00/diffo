# Cache the last syntax lookup

## Idea

Syntect finds a language by scanning the bundled syntax list. Repeated uncached
viewport requests for one file resolve the same file name each time.

Remember the last successful file-name lookup inside each highlighter. Keep only one
entry, and do not cache first-line detection because file content can change.

## What counts as a win

The geometric mean of all top-window benchmarks improves by at least 5%, no
deep-window case regresses by more than 5%, and syntax detection tests remain
unchanged.
