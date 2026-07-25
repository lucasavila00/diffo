# Do not allocate spans for hidden look-behind lines

## Idea

Syntect's convenience API returns a new vector of styled text for every parsed line.
Diffo discards that vector for look-behind lines because they exist only to prepare
parser state.

Use syntect's lower-level iterator to advance the same state without collecting
hidden spans. For visible lines, copy directly from that iterator into Diffo's
result instead of building an intermediate vector.

## What counts as a win

Every deep-window benchmark improves by at least 10%, visible syntax output remains
unchanged, and no top-window case regresses by more than 5%.
