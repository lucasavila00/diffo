# ADR 0040: Keep keyboard navigation complete and paired

## Problem

Diffo has `n` for the next change, but no keyboard shortcut for the previous
change. Going forward is fast; going back requires the mouse. This breaks
keyboard-only review and makes the controls feel unfinished.

Keyboard navigation is core product behavior. Every important navigation path
must be usable and discoverable without a mouse.

## Options

### Use Shift+N

This visually pairs with `n`, but adds modifier friction and is easy to miss in
help. Diffo does not assign uppercase character shortcuts.

### Use an arrow key

Arrow keys already scroll the diff. Overloading them would make movement depend
on hidden context.

### Use `p`

`p` is lowercase, mnemonic for previous, and free of conflicts. Together, `n`
and `p` form a simple next/previous pair. Chosen.

## Decision

- `n` jumps to the next change.
- `p` jumps to the previous change.
- Keep both in the binding registry so the help popup is generated from the real
  controls.
- Label the overlaid change links `Previous change (p)` and `Next change (n)` so
  the pointer affordance also teaches the keyboard path.
- Before adding a shortcut, check the complete registry for conflicts.
- When a keyboard navigation action has a natural opposite, provide and test
  both directions.
- Do not make mouse controls the only way to reach an important navigation
  target.
