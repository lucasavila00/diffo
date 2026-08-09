# ADR 0080: Navigate between whole change blocks

Refines [ADR 0021](0021-full-file-diffs-and-change-navigation.md),
[ADR 0022](0022-large-hunk-navigation-targets.md), and
[ADR 0067](0067-viewport-aware-change-navigation.md), and
[ADR 0079](0079-color-change-navigation-by-target.md). Supersedes the rules in
ADRs 0067 and 0079 for moving and coloring one viewport at a time within a
viewport-spanning change.

## Context

Diffo asks Git for enough context to present a changed file as one full-file
diff. This effectively merges Git's ordinary hunks into one large hunk while the
projection keeps each contiguous run of changed rows as a separate change block.

The `p` and `n` actions currently use the viewport edge as their navigation
boundary. When a change block is taller than the viewport, another action moves
one viewport through that same block. The control therefore says “next change”
while keeping the user inside the current change. This exposes projection size
as navigation structure and makes a large replacement require repeated actions
before the next independent change can be reached.

## Decision

Treat one contiguous run of changed rows as the atomic unit for directional
change navigation. Git hunk boundaries do not create additional navigation stops
inside the full-file diff.

- `n` moves to the first change block after every block intersecting the current
  content viewport.
- `p` moves to the first row of the nearest change block before every block
  intersecting the current content viewport.
- A partially visible or viewport-spanning block is the current block and is
  skipped as a whole. Directional navigation never pages within it.
- When no change block intersects the viewport, `n` and `p` retain their
  existing directional targets below and above the viewport.
- The actions do not wrap. A direction is unavailable when no other block exists
  in that direction.

The rule applies independently to the committed inline and side-by-side
projections, whose row bounds may differ. All input paths use the same targets:
the `p` and `n` keys and the large previous/next controls must agree on
availability and destination. Opening a file still starts at its first change
block, and clicking a marker still jumps directly to that block.

Manual scrolling, page scrolling, and scrollbar movement remain available for
reading the hidden portion of a large block. No active-block selection state,
configurable behavior, new key binding, or new application mode is introduced.

Target preparation remains atomic. The displayed projection, syntax coverage,
change-block bounds, button availability, button color, and destination must
commit together. Stale or pending projections cannot contribute navigation
state.

## Consequences

Each `p` or `n` action advances to a distinct change block, regardless of how
tall the current block is. Large blocks may contain unseen changed rows when
navigation leaves them; reviewing every row is a scrolling concern rather than a
change-jump concern.

The hunk-marker rail continues to represent change blocks rather than Git hunk
headers. A large block has one marker and one directional stop. The
navigation-control color continues to come from the destination block under
ADR 0079.
