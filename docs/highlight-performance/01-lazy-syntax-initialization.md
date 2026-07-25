# Lazy syntax initialization

## Idea

Creating a highlighter currently loads bat's bundled language definitions into
memory. If we create more than one highlighter, we may be repeating that setup work.

Load those definitions the first time they are needed, then let every highlighter in
the process use the same read-only copy.

## What counts as a win

`initialize/syntax-highlighter` improves by at least 50%, with no regression above
5% in any `window/*` benchmark.
