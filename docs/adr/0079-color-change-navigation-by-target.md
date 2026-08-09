# ADR 0079: Color change navigation by its target

Refines [ADR 0008](0008-diff-colors.md),
[ADR 0038](0038-remove-button-hover-changes.md),
[ADR 0054](0054-readable-tree-labels-and-controls.md), and
[ADR 0067](0067-viewport-aware-change-navigation.md).

## Context

The large `Previous change (p)` and `Next change (n)` controls use the same bold
white style as every other enabled control. Their fixed positions and labels
make them usable, but the neutral style does not connect them visually to the
offscreen changed content they reveal.

A change does not always have one color. Inline replacements contain red removed
rows followed by green added rows, side-by-side replacements expose both kinds
in one projected row, and conflict rows use the conflict color. Assigning one
fixed "hunk color" would discard that meaning. Coloring the controls on hover
would also restore the passive pointer redraws removed by ADR 0038.

## Decision

Color each available change-navigation control from the changed content nearest
to the viewport in that direction:

- `Next change (n)` uses the semantic diff background of the first hidden
  changed content below the viewport.
- `Previous change (p)` uses the semantic diff background of the last hidden
  changed content above the viewport.
- Added content uses the existing xterm-256 green background, removed content
  uses the existing xterm-256 red background, and conflict content uses the
  existing conflict background.
- A side-by-side replacement represents removed and added content in the same
  row. For that row, the previous control uses the removed background and the
  next control uses the added background. Do not collapse a replacement into a
  new third color. A conflict on the relevant side takes precedence and uses the
  conflict background.

The chosen style comes from the committed projection and the same directional
target used by navigation. A control must never preview a pending or stale
projection. When a viewport-spanning change moves one screen at a time, use the
changed content at the edge being revealed by that specific move.

Fill the complete button row with the semantic background, including the padding
around the centered label. Keep the arrow and label bold with the primary white
text foreground. The control has one stable style while visible. Pointer
movement, focus, and the input method do not change it. Hide the control when
its directional action is unavailable, as before.

The semantic background communicates the kind of destination diff content rather
than control state. The white foreground continues to satisfy ADR 0054's
enabled- control rule. This decision does not introduce a general facility for
caller-styled controls, a new palette role, configuration, animation, or hover
handling.

## Consequences

The edge controls become a preview of the changed content just outside the
viewport. Users can distinguish an upcoming addition, removal, replacement, or
conflict before navigating, while the labels and bold emphasis preserve the
persistent affordance.

Button preparation must retain enough committed projection information to derive
the style and target atomically. Inline and side-by-side modes may render
different styles for the same Git hunk because their visible projections differ;
this matches their existing navigation bounds and diff presentation.

## Verification

- Render next and previous targets for additions, removals, replacements, and
  conflicts in inline and side-by-side modes and assert their semantic
  backgrounds.
- Verify a mixed inline replacement uses the changed row nearest the viewport in
  the direction of travel.
- Verify a side-by-side replacement uses the removed background for previous,
  the added background for next, and the conflict background when the relevant
  side is a conflict.
- Verify the full button row uses the background while its arrow and label
  remain white and bold.
- Verify the displayed style and navigation target always come from the same
  committed projection across file opens, view-mode changes, and
  viewport-spanning changes.
- Verify passive pointer movement does not change the frame or request a redraw.
- Verify unavailable controls remain hidden and keyboard and mouse actions
  retain identical targets.
- Run `make all` when implementing the decision.
