# Remove per-call thread creation

## Idea

Every highlight request starts one temporary thread for the old file and another for
the new file. For a small visible window, starting and joining those threads may cost
more than the work they save.

Compare the current approach with either processing both sides in sequence or
sending them to workers that already exist.

## What counts as a win

Every `window/*/top` case improves by at least 10%, without slowing any deep-window
or reference-boundary case by more than 5%.
