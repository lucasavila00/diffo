# ADR 0030: Split `diffo-tui/src/input.rs`

Status: Proposed

## Decision

Keep `input.rs` for `map_event` and modal precedence.

- `input/bindings.rs`: fixed key table, labels, help rows, access checks.
- `input/keyboard.rs`: normal, palette, help, and commit key mapping.
- `input/mouse.rs`: clicks, wheel, drags, splitters, action buttons.

Move tests beside their owner. Keep precedence tests in `input.rs`.
Do not change keys, help text, or emitted messages.
