# ADR 0052: Render in-app errors as inert terminal text

## Context

Repository watcher, file-loading, and operation errors cross system boundaries
as arbitrary strings. They render in Explorer or in the shared acknowledgement
modal. Multiline content can otherwise be mistaken for renderer-owned layout. A
line feed can therefore turn a one-line status into multiline content or inject
an extra toast row. Terminal controls such as escape, carriage return, and
backspace can also move the cursor or alter unrelated cells. Width calculation
and truncation then operate on different content from what the terminal
interprets.

Diffo already uses `diffo_ui::terminal_safe_text` for repository file content,
paths, and Explorer errors. It expands tabs to a fixed width, renders C0
controls and DEL as visible control pictures, and replaces remaining control
characters. The resulting string contains no terminal control characters.

## Decision

Treat every error field as untrusted at the terminal rendering boundary. Before
creating an Explorer paragraph or modal title or detail, pass the complete field
through `terminal_safe_text`. Perform display-width measurement, wrapping,
sizing, and truncation only after that conversion.

An embedded line feed inside a single modeled error field is shown as `␊`; it
never creates another rendered line. The modal may put a title and a separately
modeled detail on different rows; that separator is renderer-owned rather than
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

## Consequences

Multiline and control-bearing failures can no longer change cursor position or
error-view geometry. Users still see where control characters occurred, and the
shared acknowledgement modal retains intentional title/detail presentation.
