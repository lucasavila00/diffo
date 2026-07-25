# Stop scanning after the requested window

## Idea

Diff line numbers are ordered. Once the highlighter reaches a line after the
requested range, every remaining line is also outside that range.

Stop the loop there instead of checking the rest of the file. This should help top
windows most because nearly the whole side follows them.

## What counts as a win

Every `window/*/top` case improves by at least 5%, with no deep-window or syntax
output regression.
