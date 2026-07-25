# Reduce parser look-behind

## Idea

A deep jump currently highlights 256 hidden lines before the visible window so the
parser has some earlier context. That safety margin may be larger than we need.

Try smaller values against multiline strings, comments, and other constructs that
cross viewport boundaries. This is useful only if the visible colors stay correct.

## What counts as a win

All `window/*/deep` cases improve by at least 15%. Syntax snapshots and multiline
construct tests must remain unchanged; speed alone is not a win.
