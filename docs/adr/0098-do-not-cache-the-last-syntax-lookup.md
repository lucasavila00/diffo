# ADR 0098: Do not cache the last syntax lookup

Status: Rejected

Evaluates
[Cache the last syntax lookup](../highlight-performance/13-cache-last-syntax-lookup.md).

## Problem

Syntect resolves an extension with a linear scan of the bundled syntax list. We
tested a one-entry per-highlighter cache for repeated requests on the same file.

The cache stored only successful file-name lookups. First-line detection remained
uncached because its result can change with file content.

## Result

TypeScript top-window highlighting improved by 5.5%. Rust, JSON, and Markdown had no
statistically significant change. The geometric improvement across the four cases
was about 1.4%, below the required 5%.

First-line inference is not used for recognized extensions, so removing that
fallback would not improve the measured cases.

## Decision

Do not keep the mutex and last-syntax state for a small, language-specific gain.
Continue using syntect's direct lookup and preserve first-line detection for
extensionless files.
