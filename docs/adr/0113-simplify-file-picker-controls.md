# ADR 0113: Simplify file-picker controls

Refines [ADR 0040](0040-paired-keyboard-navigation.md),
[ADR 0049](0049-shared-file-picker.md), and
[ADR 0071](0071-separate-commit-and-sync-controls.md).

## Problem

The shared file picker exposes `Home` and lowercase `g` as two bindings for a
First file action, and `End` as a Last file action. These long-distance actions
add three keys and two Help rows beside the primary previous and next controls,
even though files remain reachable through sequential navigation, the scrollbar,
pointer input, and Quick Open.

Help also labels lowercase `c` as `Open path menu`. That key and right-click
open the same row context menu. Naming the control after the menu's current path
actions describes its contents rather than the interaction and unnecessarily
constrains how the shared menu can evolve.

## Decision

Remove the public First file and Last file actions from the shared file picker.
Plain `Home`, `End`, and lowercase `g` no longer change file selection and do
not appear in Help. Keep lowercase `j` for the previous file and lowercase `k`
or `l` for the next file.

Retain internal first-row and last-row selection operations where Diff needs
them to continue sequential navigation across its staged and unstaged pickers.
They are implementation details, not separately bound user actions.

Label lowercase `c` as `Open contextual menu`. It opens the same contextual menu
as right-click for the selected row. The label describes that shared interaction
without changing the menu's actions or eligibility.

## Alternatives

- Keep the actions but remove them from Help. Rejected because hidden fixed
  controls are not discoverable and would preserve the unnecessary bindings.
- Remove only lowercase `g` and keep `Home` and `End`. Rejected because the
  first-file and last-file actions themselves are being removed, not merely
  their character alias.
- Keep `Open path menu`. Rejected because it gives two ways of opening one
  contextual menu different names and couples the control label to its present
  contents.

## Consequences

File selection has one compact previous/next keyboard pair, and Help has two
fewer actions. `Home`, `End`, and lowercase `g` are unassigned in normal Diff
and Explorer file-picker input.

Keyboard and right-click users see one name for the same contextual menu. Its
existing path-copy behavior is unchanged.
