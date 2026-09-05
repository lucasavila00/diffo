# ADR 0016: Key help and mouse targets

## Key registry

Each key binding stores:

- Actual `KeyChord` values.
- App message.
- Action description.
- Availability in the current committed state.

Generate shortcut labels from `KeyChord`. Do not parse help strings. The help
popup is a two-column Shortcut / Action table built from this registry.

## Mouse targets

Mouse actions use renderer-owned geometry.

- File row body selects the file.
- File row `[+]` or `[-]` runs that file action.
- Header `+` and `-` are one-cell buttons.
- Scrollbar final cells map to exact maximum offsets.
- The horizontal scrollbar owns the bottom-right scrollbar corner.

Coordinate tests verify action cells and nearby inert labels. PTY tests verify
the same behavior through the compiled binary.
