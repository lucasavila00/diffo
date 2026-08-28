# ADR 0117: Share one atomic review pipeline

Changes [ADR 0115](0115-review-checkout-history.md), refines
[ADR 0116](0116-share-diff-and-history-renderer.md), and extends the atomic
transition contract in [ADR 0024](0024-atomic-diff-buffer-transitions.md).

## Context

ADR 0115 gave History a local hunk-only renderer and preparation path. ADR 0116
introduced the same three review modes in Diff and History, but did not define
which state and behavior had to be shared. Separate right-side implementations
could still drift in rendering, shortcuts, scrolling, syntax preparation,
full-screen behavior, and asynchronous transitions.

Hunk mode also combines changes from several files. Flattening those patches
without retaining file identity loses the information needed to focus a selected
file and to apply that file's syntax. Moving to a distant file before its syntax
window is ready would violate ADR 0024 by showing a plain target and coloring it
in a later frame.

## Decision

Diff and History keep separate left-side state and repository requests. Each
activity owns an instance of the same right-side renderer and review state. The
shared infrastructure owns:

- Inline, side-by-side, and Hunk projection and rendering.
- Vertical and horizontal viewport state, bounds, scrolling, and full-screen
  rendering.
- Fixed review input, including lowercase `r` cycling through all three modes.
- Background preparation, prepared-buffer caching, syntax coverage, navigation
  targets, and atomic viewport transitions.

Both activities describe right-side content with the same review document and
selection types. Diff supplies working-tree and index patches. History supplies
the selected commit and its files. History loads full-file context lazily when
Inline or side-by-side mode needs it, and identifies cached file content by both
commit ID and path.

Hunk mode is one compact aggregate projection containing every changed file.
Selecting a file changes only the focus within that projection; it never filters
the projection to one file. Every aggregate row retains its source segment, and
every file has a bounded target range. Focus moves to the first change inside
that range, or to the range start for a metadata-only change. Inline and
side-by-side modes continue to show the selected full file.

Preserve each aggregate segment's parsed document and syntax identity. Prepare
syntax only for segments intersecting the bounded visible window, using the
existing parser look-behind, byte budget, reusable coverage, and strict
10,000-line eligibility boundary. Compute a file-focus target before preparing
the frame. If its syntax is not covered, keep the previous selection and
viewport committed until the target window is ready, then commit the focus,
coverage, scroll bounds, and viewport together.

Install background results only during frame preparation. Superseded commit,
file, mode, and syntax requests cannot change displayed content or leave an
activity permanently preparing. Full-screen row metrics must describe the same
projection that full-screen rendering consumes.

## Alternatives

- Keep a History-specific renderer and copy Diff behavior into it. Rejected
  because matching behavior would depend on maintaining two implementations.
- Flatten aggregate hunks without file segments. Rejected because file focus and
  language-specific syntax would become guesses.
- Filter Hunk mode to the selected file. Rejected because Hunk mode represents
  the complete change and file selection is navigation within that change.
- Load every historical full file with the commit patch. Rejected because Hunk
  review does not need that content and commit size is unbounded.
- Move immediately and prepare target syntax afterward. Rejected because it
  exposes a partial frame and violates the atomic review contract.

## Consequences

The Diff and History screens differ on the left side and in their data sources,
while right-side behavior stays aligned by construction. A renderer change to
review modes, controls, scrolling, syntax, or full-screen presentation applies
to both activities.

Aggregate Hunk preparation retains more segment metadata, and History may issue
an additional file request after a mode change. Both costs stay bounded: syntax
work remains viewport-limited, full files load only on demand, and prepared
review content uses the existing fixed cache boundary.
