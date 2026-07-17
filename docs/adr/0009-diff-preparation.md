# ADR 0009: Prepare diffs without blocking input

Status: Accepted

## Decision

Keep parsing, projection, and syntax work out of the input loop for large diffs.

- Parse patches up to 64 KiB and 500 lines now. Show them on the first frame.
- Send larger patches to one bounded worker.
- Keep the last finished diff visible while the next diff loads.
- Show an empty pane when the first large diff loads. Do not flash loading text.
- Poll every 16 ms while work is pending. Poll every 250 ms when idle.
- Render only visible rows. Cache the parsed diff and both view projections.
- Skip syntax highlighting above 256 KiB or 2,000 lines.
- Show raw patch text when parsing fails.

This keeps `q`, Ctrl-C, navigation, and resize responsive.

## Tests

- Small diff renders on the first call.
- Large diff uses the worker.
- Old content stays until new content is ready.
- Bad patch shows raw text.
- Keep the ignored 100k-line timing test.
