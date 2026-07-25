# ADR 0087: Share syntax assets between highlighters

Status: Accepted

## Problem

Constructing a `SyntaxHighlighter` loaded and decoded the same bundled language
definitions and theme every time. The data is read-only after loading, so separate
copies add setup time and memory without changing behavior.

## Result

The assets now load once, when the first highlighter needs them. Later highlighters
borrow the shared copy.

`initialize/syntax-highlighter` improved from about 1.26 ms to 5.2 ns. Highlighting
tests and syntax snapshots remained unchanged.

This benchmark measures repeated construction. The first highlighter still pays the
cost of loading the assets, so this result is not evidence of faster cold startup.

## Decision

Keep the shared, read-only syntax assets. The change removes repeated work and keeps
the highlighter API unchanged.

Add a separate cold-load benchmark before attempting to optimize first-use startup.
