# Avoid collecting complete diff sides

## Idea

Before highlighting a viewport, we build lists containing every old and new line in
the diff. Most of those lines are not part of the requested window.

Walk the diff once and collect only the lines needed for the visible range and its
look-behind.

## What counts as a win

All `window/*/deep` cases improve by at least 15%, with no `window/*/top` regression
above 5%.
