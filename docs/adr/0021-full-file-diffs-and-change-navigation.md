# ADR 0021: Full-file diffs

Status: Proposed

## Problem

Diffo only shows Git hunks. The user cannot see the rest of the file. This makes a
change hard to understand.

Showing the full file creates another problem. Changes can be far apart and hard to
find.

## Decision

Show the full old and new files, like VS Code.

Keep hunks as navigation targets:

- Add next-change and previous-change actions.
- Mark every change on the vertical scrollbar.
- Keep the viewport visible on the same scrollbar.
- Let users click a change marker to jump to it.

The scrollbar markers are Diffo's minimap. Do not render miniature code. A terminal
does not have enough space for that.

Use these file versions:

- Staged: `HEAD` and index.
- Unstaged: index and working tree.
- New file: empty file and new file.
- Deleted file: old file and empty file.

## Why

The full file gives context. Hunk jumps make navigation fast. Scrollbar markers show
where changes are.

## Rejected

- Keep hunk-only view: not enough context.
- Use a huge Git context value: not a real full-file model.
- Render a code minimap: too little terminal space.

## Cost

Diffo must load both full file versions. Large files need background diff and syntax
work so input and scrolling stay responsive.
