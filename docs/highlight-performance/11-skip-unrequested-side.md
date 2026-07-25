# Skip an unrequested diff side

## Idea

Added and deleted files may need syntax colors for only one side of the diff. The
highlighter currently gathers both complete side lists before it notices that one
side was not requested.

Do not gather or start highlighting work for a side whose requested range is
`None`.

## What counts as a win

The new one-sided Rust benchmarks improve by at least 10%, with no regression in
the existing two-sided Rust windows.
