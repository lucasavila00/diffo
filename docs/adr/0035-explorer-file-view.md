# ADR 0035: Explorer file tree and viewer

Status: Accepted

## Problem

Explorer needs the full repository tree, including unchanged files. The shared Git
snapshot only contains changes. Loading every file and highlighting it up front
would make startup slow and couple Explorer to Diff.

## Decision

Keep Explorer as its own tool with its own tree, selection, expansion, scroll, file
viewer, and background requests.

Load the tree from tracked and non-ignored untracked repository paths. Merge Git
status onto those paths. Keep deleted paths as missing tree entries so they remain
visible. Do not add unchanged paths or file contents to the shared Diff snapshot.

Use a left tree and a right read-only file viewer. Match the Diff activity's text,
spacing, borders, and change colors. Changed tree entries use the existing Git status
colors. Unchanged files and directories use white and gray. The terminal owns the
font and font size; Explorer uses the same terminal cell styles as Diff.

Read only the selected file. Also request its HEAD-to-worktree patch. Project that
patch onto file line numbers and draw a one-cell change gutter:

- green for added lines;
- yellow for modified lines;
- red at deletion points;
- the existing conflict color for conflicts.

Treat an untracked file as an addition from an empty base.

Keep syntax color on the text. Do not paint the whole changed line with a diff
background. Deleted files show the HEAD version with red markers. Binary or invalid
UTF-8 files show a plain message.

Reuse the Diff highlighter and its 10,000-line eligibility limit, viewport window,
look-behind, and byte budget. File selection is an atomic viewer commit: keep the old
viewer until content, line numbers, change markers, scroll bounds, and visible syntax
are ready together. Stale background results cannot commit.

## Alternatives

- Put every path and file body in the shared snapshot. Rejected. Startup and refresh
  become heavy, and the tools become coupled.
- Reuse the Diff model. Rejected. Tree and viewer state are independent.
- Highlight the whole selected file before display. Rejected. Large files block open.
- Render a unified diff in the viewer. Rejected. Explorer must show the real file.

## Tests

Cover tree building, status colors, unchanged colors, expansion, selection, gutter
mapping, deleted and binary files, stale results, and atomic syntax-ready opens.
