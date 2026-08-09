# ADR 0009: Prepare diffs without blocking input

Atomic buffer installation is specified by ADR 0024.

## Decision

Keep parsing, projection, and syntax work out of the input loop for large diffs.

- Parse patches up to 64 KiB and 500 lines now. Show them on the first frame.
- Send larger patches to one bounded worker.
- Keep the last finished diff visible while the next diff loads.
- Show an empty pane when the first large diff loads. Do not flash loading text.
- Poll every 16 ms while work is pending. Poll every 250 ms when idle.
- Render only visible rows. Build the active view projection and cache up to
  four recently prepared file-and-mode buffers; prepare the other mode only when
  asked.
- Syntax-highlight files below 10,000 lines using the bounded viewport windows
  from ADR 0032. File line count comes from parsed old/new line numbers,
  independent of patch byte size and the synchronous-work threshold. Skip syntax
  highlighting at 10,000 lines and above.
- Show raw patch text when parsing fails.

This keeps `q`, Ctrl-C, navigation, and resize responsive.
