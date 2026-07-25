# Cache parser-state checkpoints

## Idea

Syntax at line 9,000 can depend on text that appeared earlier in the file. Today we
replay a fixed number of earlier lines whenever we jump there.

Save syntect's parser state at regular line intervals. A deep jump could resume from
the nearest saved point instead of rebuilding that state for every request.

## What counts as a win

All `window/*/deep` cases improve by at least 30%, no top-window case regresses by
more than 5%, and highlighted output remains unchanged.
