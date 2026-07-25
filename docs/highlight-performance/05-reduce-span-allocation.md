# Reduce styled-span allocation

## Idea

Syntect returns pieces of each highlighted line. We currently turn every piece into
a new owned string and place it in a new output structure.

Measure where those allocations happen, then try to reuse line text or store token
ranges when that can be done without complicating rendering.

## What counts as a win

`reference-boundary/rust/9999-lines` improves by at least 15%, and a majority of
`window/*` cases improve without any regression above 5%.
