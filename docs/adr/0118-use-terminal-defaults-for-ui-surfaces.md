# ADR 0118: Use terminal defaults for UI surfaces

Changes the dark-only color decision in
[ADR 0052](0052-semantic-chrome-colors.md) and refines
[ADR 0054](0054-readable-tree-labels-and-controls.md). Changes selected-label
styling from [ADR 0050](0050-file-picker-status-colors.md) and
[ADR 0065](0065-propagate-explorer-git-colors.md).

## Context

WT evolved a similar terminal interface after its initial shell and control UI
commits, `791999af` and `60118ffe`. This review compares Diffo at `689a1e8` with
WT at `734053ab825335a50d9948614da144e1dea4cd38`. The transferable changes are
design decisions, not a compatible crate patch to cherry-pick: WT owns its
styles inside `crates/products/wt/client/src/`, while Diffo centralizes shared
components in `diffo-ui`.

WT's
[ADR 0038](https://github.com/lucasavila00/wt/blob/734053ab825335a50d9948614da144e1dea4cd38/docs/adr/0038-make-wt-shell-terminal-theme-safe.md)
and
[commit 85001ae8](https://github.com/lucasavila00/wt/commit/85001ae89c9b40b8eedca2e770443046cae271ba)
replaced black surfaces, white text, dark-gray chrome, and dark selection fills
with terminal defaults, dim secondary text, and reversed selections. ANSI color
names identify palette slots; they do not guarantee physical brightness. A
terminal's light palette can make a supposedly dark surface light, while fixed
white text becomes difficult to read.

Diffo retains `TEXT = White`, `CHROME = DarkGray`, and
`SELECTION_BACKGROUND = CHROME` in `diffo-ui/src/lib.rs`. These roles reach file
lists, command and search pickers, prompts, help, borders, dividers, and
scrollbars. Replacing a few local colors would leave the shared contract wrong.

## Decision

Use terminal-default foreground and background for ordinary UI text and
structural surfaces throughout Diffo. Code-view content uses the explicit dark
surfaces defined in [ADR 0120](0120-render-code-on-explicit-dark-surfaces.md).
Express shared roles as styles, including modifiers, rather than requiring every
role to be a color constant. Keep their ownership in `diffo-ui` alongside the
existing layout tokens.

Primary text uses the default foreground. Enabled controls use that foreground
with the existing bold emphasis and persistent affordance. Secondary text and
disabled controls may use `DIM`; essential labels, enabled controls, scrollbar
thumbs, and resize affordances must not become dim merely because they are
unfocused. Structural lines use default colors, with dim emphasis only where
their geometry remains understandable. Glyphs distinguish scrollbar tracks and
thumbs without depending on gray levels.

Text, list, picker, and form selections reverse terminal defaults. Explicitly
reset both colors and remove inherited dim styling on the selected label before
applying `REVERSED`; adding reversal to a colored foreground or a diff
background does not reverse terminal defaults. Keep selection markers visible.
Selection applies to the complete row, including its persistent action, and
resets semantic foregrounds before reversing the terminal defaults. File names
retain deletion strikethrough. Diff retains its status symbols, so selected-row
meaning does not depend on color. Explorer keeps its existing compact layout
without status letters; selected Explorer rows rely on their existing file and
folder identity plus repository context, while Diff remains the complete textual
status view. Unselected file labels retain Git-status coloring.

Framed content uses a navigation accent on its boundary or marker, not reversal
across the content. This preserves embedded syntax and status styles, following
WT's
[border-selection correction](https://github.com/lucasavila00/wt/commit/c93c8702fbeae836755d8944bc80364874454384).
[ADR 0119](0119-separate-navigation-and-status-colors.md) defines that accent.

Overlays clear their complete bounds to terminal defaults before drawing their
contents. A default background must not mean retaining underlying characters or
styles. WT's
[modal-background fix](https://github.com/lucasavila00/wt/commit/b3d6805e8a8dca9c59c95beba94eb6c7d02a2dc5)
demonstrates this distinction; Diffo already uses `Clear` in shared pickers and
prompts, and that behavior remains part of the surface contract.

The terminal owns light/dark switching. Diffo adds no theme setting,
command-line option, environment configuration, palette query, theme polling, or
redraw trigger for these styles. Already displayed default and reversed cells
follow the terminal's defaults. Code views retain Monokai colors on explicit
dark backgrounds under
[ADR 0120](0120-render-code-on-explicit-dark-surfaces.md). Light-mode syntax
remains a separate, deferred limitation.

## Related WT improvements

The shell history also contains improvements that do not require a second Diffo
implementation:

| WT change                                                         | Disposition in Diffo                                                                                                                                                                                                |
| ----------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `20575646`, distinguish navbar states                             | Carry over the distinction between passive structure and actionable controls through shared styles; do not copy WT's dim playback bar onto Diffo's enabled controls. ADR 0054 already requires visible affordances. |
| `6935c700`, enlarge navigation targets                            | ADR 0054 already ties visible labels to exact hit geometry; retain it rather than importing world navigation controls.                                                                                              |
| `834d19cd`, modal mouse and keyboard fixes                        | The commit explicitly aligned WT with Diffo conventions. Diffo already owns modal input and shared prompt geometry; this is not evidence for replacing its input model.                                             |
| `1b8dcd79` and `7640b087`, card scrollbars and viewport scrolling | Diffo already has shared scrollbars and independent viewport state. WT's card grid is not a replacement for file and text navigation.                                                                               |
| `86888e05`, remove activity frames                                | A WT dashboard layout choice. Diffo's pane seams carry resizing and navigation geometry, so removing them does not follow from theme safety.                                                                        |
| `3c19c918` and WT ADR 0075, contextual world actions              | Diffo already has contextual file/folder actions. Preserve the separate menu hit target and confirmation boundaries; do not introduce world-card UI.                                                                |
| `f37540b7`, remove buffer debug snapshots                         | Not adopted. Diffo's style snapshots and theme-contract tests expose precisely the hardcoded colors being changed.                                                                                                  |

WT's
[ADR 0003](https://github.com/lucasavila00/wt/blob/734053ab825335a50d9948614da144e1dea4cd38/docs/adr/0003-refresh-terminal-theme-through-byobu.md)
addresses stale terminal color reports by owning a newer tmux in its guest
images. Diffo neither owns a multiplexer installation nor needs those reports
for default-color UI. That infrastructure change does not belong upstream here.

## Alternatives

Maintaining light and dark chrome palettes requires detection, fallback, and
runtime transitions for a problem terminal defaults already solve. Replacing
ANSI dark gray with fixed RGB gray still assumes a background. Both alternatives
retain unnecessary application ownership of terminal appearance.

## Consequences

The shared style contract replaces ADR 0052's intentional lack of light-theme
support for chrome. Layout, fixed controls, SSH input costs, and content
ownership remain as before. Existing color-only call sites must consume whole
styles so dim and reversed behavior cannot drift between activities.

Appearance follows the user's terminal palette, including its limitations. `DIM`
and accent contrast vary between terminals; text, symbols, and geometry
therefore continue to carry essential meaning. Light-mode support is limited to
the surrounding UI; code views remain dark to preserve Monokai readability.
