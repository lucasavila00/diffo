# ADR 0023: Keep file content separate from Git metadata

Status: Accepted

## Problem

Source files can contain text such as `GIT binary patch`. Searching the whole patch
for that text made Diffo report a normal source file as binary.

## Decision

Classify Git metadata only where the Git format allows it. Once the parser enters a
hunk, added, removed, and context lines are file content.

Do not infer file state from source text. Conflict markers receive conflict styling
only when Git status says the file is conflicted.

Every new Git sentinel must have tests showing that the same text is safe in added,
removed, and context lines. Keep a real TUI test with metadata-looking source text.

## Cost

Parser state and trusted repository state must be passed into diff projection and
included in renderer cache keys.
