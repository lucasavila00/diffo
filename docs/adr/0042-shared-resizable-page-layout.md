# ADR 0042: Share the resizable page layout across activities

## Problem

Diffo's Diff activity has a left file pane and a right diff pane separated by a
vertical seam. The seam can be dragged to change the left pane's width. Explorer
has the same basic tree-and-viewer shape, but uses a fixed 32/68 split and
cannot be resized. New activities can repeat either implementation and make this
basic interaction inconsistent again.

The inconsistency exists because the Diff model and renderer currently own the
split percentage, drag state, layout calculation, hit target, and mouse-event
mapping. Those are page-shell concerns rather than diff behavior. Copying them
into Explorer would fix one screen while preserving the cause of the drift.

## Decision

Make a resizable two-pane layout part of the workbench page shell used by every
activity.

The workbench owns one `PaneSplit` state containing the leading-pane percentage,
the last non-zero percentage, and whether a drag is active. The same percentage
is used when switching activities, so the seam stays in the same column and
resizing on one page applies to every page. Activity models do not store copies
of this state.

Put the reusable split primitive in `diffo-ui`. It owns the pure behavior shared
by all pages:

- calculate leading pane, seam, and trailing pane rectangles from an area;
- identify the seam's fixed mouse hit target;
- convert a mouse column to a bounded pane percentage;
- begin, update, and end a drag;
- collapse the leading pane and restore its last non-zero width; and
- provide the seam's normal and active visual styles.

Keep the existing product behavior as the initial contract: the leading pane may
be collapsed to zero, its non-zero width is capped at 80 percent, and dragging
continues until the left button is released. Small terminal areas must use
saturating geometry and must not panic or produce coordinates outside the
supplied page area.

The workbench divides its content into the shared panes and passes the committed
pane rectangles to the active tool for frame preparation, rendering, and local
hit testing. A tool decides what appears in each pane, but it does not recompute
the horizontal split. Diff renders files and the diff in those rectangles;
Explorer renders its tree and viewer in them; History and future activities use
the same contract even when one pane is temporarily empty. Status rows and
overlays remain outside the pane primitive.

The workbench handles seam press, drag, and release before dispatching ordinary
events to the active tool. An open modal or palette keeps input priority and
prevents a resize from starting through it. Once a resize has started, the
workbench captures the drag and release even if the pointer leaves the seam.
Pointer movement without a held button remains ignored and must not trigger
redraws.

Remove the Diff-specific pane percentage, resize messages, geometry helpers, and
border-style helpers after the workbench path is in use. Do not retain adapters
that allow activities to create private page splits.

## Alternatives

- Add resize handling to Explorer. Rejected. It duplicates the Diff
  implementation and allows behavior and bounds to diverge again.
- Give each activity an instance of a shared split component. Rejected. It
  shares code, but widths still jump during activity changes and the page-shell
  behavior remains optional.
- Keep the state in Diff and let other activities read it. Rejected. It makes a
  product-wide layout depend on one activity and violates activity independence.
- Make arbitrary nested panels resizable. Rejected. This ADR standardizes the
  one page-level vertical seam; a general docking or layout framework is not
  needed.

## Consequences

All activities have the same panel-width interaction and retain that width while
the user moves between them. Geometry used for drawing and hit-testing has one
owner. Activities remain independent in their content, selection, scrolling,
preparation, and background work, consistent with ADR 0039.

Changing the `Tool` boundary and migrating Diff is more work than adding an
Explorer mouse handler, but it removes existing duplicated layout decisions
instead of adding another one. The shared split is intentionally limited to the
page-level horizontal layout; vertical layouts, scrollbars, and
activity-specific panes remain local.
