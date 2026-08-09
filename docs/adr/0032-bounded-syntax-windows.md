# ADR 0032: Bound syntax work around the committed viewport

## Problem

Opening a file below the 10,000-line syntax cutoff highlighted both complete
file versions before the diff buffer could commit. The cost was visible even
though diff parsing, projection, first-change discovery, and rendering were
already fast.

Selection-to-first-change measurements used a warmed Diffo process, a generated
Rust file, and a change near the end of the file. Times are milliseconds:

|  Lines | Debug, full syntax | Release, full syntax |
| -----: | -----------------: | -------------------: |
|    500 |              211.5 |                    — |
|  2,000 |        686.2–765.0 |                131.3 |
|  5,000 |    1,713.0–1,799.3 |                326.7 |
|  9,999 |    3,450.0–3,563.8 |                640.0 |
| 10,000 |          20.6–20.8 |                 17.5 |

The sharp drop at 10,000 lines is where syntax highlighting is skipped. An
independent ignored 100,000-line timing test measured about 63 ms for background
parsing and both projections. Syntax work, not first-hunk navigation, caused the
delay.

## Decision

Keep syntax colors in the atomic open, but bound the work to the viewport that
will be committed.

- Files below 10,000 lines remain eligible for syntax highlighting.
- Highlight the target viewport plus three viewports of look-ahead.
- Start each side at most 256 logical lines before the requested range to
  recover useful parser context without scanning from the beginning of the file.
- Highlight old and new sides concurrently.
- Build only the requested inline or side-by-side projection. A mode toggle
  keeps the committed mode and viewport visible until the other projection and
  its syntax window are ready; prepared modes are cached independently.
- Limit each side to 512 KiB of syntax input. Reaching the budget is a completed
  plain-text fallback, not a request that can wait forever.
- Share patch storage between repeated frame requests and retain four prepared
  buffers for warm back-navigation.
- If vertical navigation targets a window that is not covered, keep the current
  viewport visible. Commit the requested scroll position only with the prepared
  syntax window. Horizontal scrolling remains immediate.
- Drain and install window results only during frame preparation. Results for an
  old file or superseded scroll target cannot commit.

The constants are product behavior, not configuration.

## Results

With bounded windows, five end-to-end debug-build measurements for the same
9,999-line fixture were 78.6, 92.4, 98.4, 94.1, and 76.3 ms. After inactive
projection work was removed, the focused preparation benchmark measured 71.6 ms
in debug and 31.0 ms in release. The first displayed buffer still contained the
target line, first-change viewport, and syntax-ready coverage in one traced
frame.

## Cost

A multiline string or comment that begins more than 256 lines before a cold
window can receive imperfect colors. A very large line can use the deterministic
plain-text fallback. Neither case changes content, geometry, hunk targets, or
scroll position.
