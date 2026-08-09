# ADR 0052: Render in-app errors as inert terminal text

## Context

Repository watcher, file-loading, and operation errors cross system boundaries
as arbitrary strings. They render in the Diff footer, the Explorer error panel,
or a workbench toast. The Diff footer currently places its error directly in a
Ratatui span, and toasts currently treat embedded newlines as layout delimiters.
A line feed can therefore turn a one-line status into multiline content or
inject an extra toast row. Terminal controls such as escape, carriage return,
and backspace can also move the cursor or alter unrelated cells. Width
calculation and truncation then operate on different content from what the
terminal interprets.

Diffo already uses `diffo_ui::terminal_safe_text` for repository file content,
paths, and Explorer errors. It expands tabs to a fixed width, renders C0
controls and DEL as visible control pictures, and replaces remaining control
characters. The resulting string contains no terminal control characters.

## Decision

Treat every error field as untrusted at the terminal rendering boundary. Before
creating a footer span, Explorer paragraph, or toast title or detail, pass the
complete field through `terminal_safe_text`. Perform display-width measurement,
wrapping, sizing, and truncation only after that conversion.

The footer remains a single `Line` and a single terminal row. In particular, an
embedded line feed inside any error field is shown as `␊`; it never creates
another rendered line. Toasts may still put a title and a separately modeled
detail on different rows; that separator is renderer-owned rather than
content-owned. Escape sequences, carriage returns, backspaces, tabs, and other
controls are visible or replaced but are never executed. The renderer supplies
error styles, so error content cannot supply terminal styling.

Keep the original error in application state. Sanitization is a presentation
concern, not a mutation of failure classification or diagnostic data. Every
terminal view that renders externally sourced text must apply the shared
rendering boundary; do not add a surface-specific escaping algorithm.

This decision makes text inert but does not redact secrets. Credentials and
interactive Git prompts must not be converted into ordinary errors in the first
place; that flow is specified separately by
[ADR 0053](0053-broker-git-interactions.md).

## Alternatives

- Keep only the first line. Rejected because it silently discards actionable
  context and still needs control-character handling.
- Split errors on embedded newlines. Rejected because content must not resize
  the footer, Explorer panel, or toast; only explicitly modeled UI fields may
  define layout.
- Sanitize when the error enters the model. Rejected because the model should
  not store a terminal-specific projection and other consumers may need the
  original diagnostic.
- Remove ANSI-looking substrings with a regular expression. Rejected because
  cursor controls are not limited to ANSI color sequences and malformed
  sequences would remain ambiguous.

## Verification

- A renderer state test supplies line feed, carriage return, escape, backspace,
  and tab characters in one error and verifies that the resulting footer
  contains no control characters.
- A complete Diff frame verifies that text after an embedded newline stays on
  the footer row and that the newline is visible as `␊`.
- Explorer and error-toast frame tests verify the same newline behavior at their
  rendering boundaries. The toast test covers both title and detail and verifies
  hit-test geometry.
- The tests verify visible control pictures and fixed footer display width.
- Existing narrow-width, Unicode-width, head-priority, and command-help tests
  continue to pass.

## Consequences

Multiline and control-bearing failures can no longer change cursor position or
error-view geometry. Users still see where control characters occurred, and
existing footer truncation rules continue to preserve the current head before
transient detail. Explicit toast title/detail structure continues to provide
intentional multiline presentation.
