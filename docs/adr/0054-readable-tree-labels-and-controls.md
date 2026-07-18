# ADR 0054: Keep tree labels and controls readable

Status: Accepted

Refines [ADR 0035](0035-explorer-file-view.md),
[ADR 0050](0050-file-picker-status-colors.md), and
[ADR 0052](0052-semantic-chrome-colors.md).

## Context

The Explorer tree renders unchanged directory names in the dark-gray structural
chrome color while unchanged file names use the white primary-text color. A
directory is already identified by its disclosure marker, indentation, and place in
the tree. Giving its name lower contrast adds no information and makes repository
paths harder to scan.

Enabled controls can have the same problem. Compact actions such as `[+]` and `[-]`
sit close to panel borders, titles, and other chrome. When their foreground is the
same dark gray as those structural elements, they look decorative or disabled
instead of actionable. This is especially harmful for small symbolic controls,
whose meaning and hit target are less obvious than a full text label.

ADR 0052 intentionally gives borders and other structural chrome one subdued color,
but its inclusion of inactive controls in that role does not distinguish an enabled,
stable control from a disabled control. ADR 0038 prohibits hover-only feedback, so
enabled controls must remain discoverable without relying on pointer movement.

The same rule must cover the whole interaction model, not only widgets named
"button." Selectable rows, dismissible surfaces, draggable seams, scrollbar rails,
and marker rails all accept mouse input. If the rendered frame contains no stable
affordance for a hit target, users must discover it by guessing. Conversely, a blank
layout cell must not trigger an action that has no rendered label or marker.

## Decision

Use the primary text foreground for unchanged directory names, exactly as for
unchanged file names. Do not lower the contrast of a path label merely to communicate
that it is a directory. Disclosure markers, indentation, expansion state, and tree
position continue to communicate directory structure. Existing semantic Git-status
styles continue to take precedence for changed file labels.

Render every enabled button or text control with a stable, high-contrast foreground
that is distinct from structural chrome. In the fixed palette, enabled control labels
use the white primary-text color and bold emphasis. This includes compact symbolic
actions such as `[+]` and `[-]`, panel-title actions, row actions, navigation buttons,
and dialog actions. Selection may add its established background, but it must preserve
the high-contrast control foreground.

Reserve the dark-gray chrome color for non-interactive structure and genuinely muted
or disabled content: borders, dividers, scrollbars, gutters, and controls that cannot
currently be activated. Do not use the chrome color for an enabled control merely
because it is not focused, selected, or hovered.

Every mouse hit target that performs a discrete action must contain a persistent
visible affordance:

- selectable flat rows use the same high-contrast leading marker before and after
  selection; tree rows rely on their disclosure/indentation structure and do not
  add a selection caret;
- draggable pane seams use a high-contrast resize marker while retaining their
  structural border;
- dismissible toasts and menus show a high-contrast close marker, while modal
  backdrops name their outside-click behavior in high-contrast help;
- editable panels show an edit marker that remains visible at the narrow supported
  layout; and
- scrollbars and diff marker rails retain their explicit tracks, thumbs, and marker
  glyphs.

The marker, label, track, or thumb must be inside the same geometry accepted by hit
testing. Blank rows and clipped labels do not create implicit actions. Large click
targets may surround a compact marker, but the marker must remain visible whenever
the action is available. The design system owns the shared interaction styles and
marker vocabulary so renderers do not invent local affordances.

The shared file picker owns row-action styling. Activities supply the action label
and behavior, not an arbitrary action style. This replaces ADR 0050's allowance for
caller-styled row actions; Git-status styling remains caller-supplied content and is
still preserved independently from the control marker and action.

File labels truncate with literal `...` when a row is narrower than its content.
Row layout reserves a visible right-side action such as `[+]` or `[-]` before
allocating width to the label, so truncation can never push an available action out
of view. Tree rows do not show the generic flat-list dot.

The style is constant while the pointer moves. This decision does not add hover
state, passive mouse handling, redraws, new key bindings, configuration, or an
alternate palette. Semantic status and diff colors remain governed by ADRs 0050 and
0008.

## Consequences

Files and directories have equal baseline readability, while the tree's existing
shape continues to distinguish them. Enabled controls remain visually separate from
the borders around them and do not appear disabled. Users can find actions without
generating hover-driven terminal work, which preserves the SSH behavior established
by ADR 0038.

Chrome can no longer double as the default style for enabled controls. Renderers must
classify text as content, an enabled control, disabled content, or structure before
choosing its style. The palette gains no new color; primary text and bold emphasis
provide the required contrast.

Persistent markers add a small amount of visual density to selectable rows and pane
seams. In exchange, the mouse interaction model can be understood from a static frame
and remains consistent across activities. Hit testing becomes stricter because empty
layout space no longer aliases the nearest rendered action.

## Verification

- Render an Explorer tree containing unchanged files and directories and assert that
  both label types use the primary-text foreground.
- Verify disclosure markers and indentation still identify directories and their
  expansion state.
- Render enabled `[+]` and `[-]` row and panel-title actions and assert that their
  foreground differs from the surrounding border and is bold.
- Render other enabled navigation and dialog actions with the same control contract.
- Verify disabled controls may use chrome styling but cannot be activated.
- Verify flat rows retain the same interaction marker after selection, while tree
  rows add no selection caret and retain their disclosure structure.
- Verify long flat and tree labels render `...`, and that a right-side row action
  remains fully visible after truncation.
- Verify the pane resize marker lies inside the seam hit target.
- Verify every dismissible toast renders a close marker inside its hit target.
- Verify blank command-palette rows do not execute a command.
- Verify the edit marker remains visible in a narrow file pane.
- Verify pointer movement does not change any control style or request a redraw.
- Run workspace formatting, tests, and clippy when implementing the decision.
