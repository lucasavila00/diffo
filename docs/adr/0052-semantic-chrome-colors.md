# ADR 0052: Centralize structural design tokens

## Context

Diffo's shared shell grew across activity, picker, command, overlay, status, and
text-view crates. Each renderer selected terminal colors locally. Equivalent
boxes, dividers, scrollbars, and selection surfaces therefore used cyan, light
cyan, gray, dark gray, blue, or the terminal default. The File Diff and Commit
message boxes could look unrelated even though both are ordinary panels in the
same layout.

Structural geometry drifted for the same reason. Panel and dialog insets,
overlay width rules, status heights, activity-rail dimensions, gaps, and
scrollbar widths were repeated as unexplained literals in their renderers.
Equivalent spacing could change independently, and a reviewer could not tell
whether a number was a shared design rule or content-specific math.

Raw color choices make this drift easy to reintroduce because reviewing one
renderer does not reveal the visual contract used by the others. Diffo still
needs distinct colors for content visualization: Git states, diff rows, conflict
markers, and syntax tokens communicate information rather than decorate the
application shell.

## Decision

`diffo-ui` owns the fixed structural design system. `diffo-ui::theme` contains
color roles and `diffo-ui::design` contains semantic geometry. Renderers consume
these roles instead of choosing raw values.

### Color

All structural chrome uses one color: dark gray. This includes:

- every box border, including focused, active, dragging, and modal boxes;
- panel and activity dividers;
- vertical and horizontal scrollbars;
- muted labels, inactive controls, gutters, and structural markers; and
- selected-row and primary-control backgrounds.

Primary text is white. Focus, activity, and dragging may add bold text or a
fixed marker, but must not change the structural color. The File Diff and Commit
message boxes receive the exact same explicit border style rather than relying
on terminal defaults.

Meaningful content may still use color: informational status is light cyan,
success is light green, warning is yellow, danger is light red, and conflicts
are light yellow on xterm-256 color 58. These colors decorate the text or
content that carries the meaning; they never tint a surrounding box, margin,
divider, or scrollbar. Selection keeps a content-supplied foreground and adds
the one gray background and bold modifier.

### Geometry

Layout values are named for their role rather than their magnitude. The shared
geometry includes:

- one-cell borders, single-line rows, panel insets, dialog insets, and inline
  gaps;
- status, file-composer, commit-field, primary-action, activity-rail, and
  activity- control dimensions;
- responsive width rules and height bounds for Command Palette, Help, and Commit
  editor overlays;
- toast and path-menu bounds;
- picker header-action geometry; and
- scrollbar, diff rail, and side-by-side divider geometry.

Responsive widths use a shared `ResponsiveWidth` rule with a percentage,
minimum, and maximum. Values that format content rather than structure chrome
remain local. Examples include the Explorer line-number gutter, diff line-number
columns, syntax look-behind, and viewport byte budgets.

The palette is fixed in code. Do not add theme configuration, environment hooks,
or alternate palettes. A workspace architecture test scans the production
portions of the shared chrome renderers and rejects direct `Color::` choices,
local `Margin` values, numeric layout constraints, and local chrome dimension
constants. Add a semantic role to `diffo-ui` when a genuinely new structural
meaning is required; do not add a role merely to preserve a component-specific
value.

ADR 0008 continues to govern diff visualization. Dynamic syntax RGB values and
diff row colors are outside this chrome rule. Git-state colors remain
centralized by `diffo-ui::change_kind_style` and use the matching semantic
status roles.

This ADR supersedes ADR 0018's changing border-gradient decision and its
border-color test. A network operation continues to animate its footer spinner
and name the operation, while its outer border remains the same gray as every
other border.

## Consequences

Equivalent structures now render consistently across activities, and future
local raw color or geometry additions fail a deterministic test. Active state
depends on emphasis and markers rather than a new hue. Colored feedback remains
available where the color communicates domain meaning instead of component
identity.

Changing shared density, overlay bounds, or structural spacing now has one
reviewable source. The design module has more named constants, but each one
records intent and prevents unrelated renderers from inventing local dimensions.

The fixed palette deliberately does not adapt to arbitrary light terminal
themes, matching the existing dark-background assumption in ADR 0008. Adding a
new semantic role requires a design-system decision rather than an isolated
renderer edit.

## Verification

- Assert the semantic role constants retain their accepted terminal colors.
- Assert panel/dialog insets and responsive overlay widths retain their accepted
  geometry.
- Scan shared chrome production sources and reject direct raw color and layout
  choices.
- Render focused, selected, muted, success, warning, danger, and conflict states
  in their existing component tests.
- Render File Diff and Commit message together and assert both borders use the
  one chrome gray.
- Verify network progress changes its spinner without changing the outer border.
- Run workspace formatting, tests, clippy, and crate documentation.
