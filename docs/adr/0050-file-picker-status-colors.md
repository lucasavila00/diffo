# ADR 0050: Preserve status colors in file pickers

Refines [ADR 0049](0049-shared-file-picker.md).

## Context

Diff and Explorer identify each changed file with a letter and a filename. The
letter is the durable, non-color indication of its Git status, while foreground
color makes a list of changes faster to scan. Added and untracked files,
modified files, deleted files, renamed and copied files, and conflicts must
remain visually distinct without requiring the user to read every marker.

The shared file picker composes caller-provided labels with selection symbols,
tree indentation, disclosure markers, spacing, and row actions. During that
composition it retained the label spans but discarded the `Line` style. Diff and
Explorer still supplied the established status styles, but those foreground
colors and modifiers no longer reached the terminal.

## Decision

Keep the fixed status decoration defined by `diffo-ui::change_kind_style`:

- added and untracked: light green;
- modified: yellow;
- deleted: light red with strikeout;
- renamed and copied: light cyan; and
- conflicted: bold light red.

Apply the decoration to the status marker and filename in both Diff's flat
pickers and Explorer's tree picker. Keep the marker letters as the non-color
signal, so color is helpful but never the only way to determine status.

The activity projection remains responsible for choosing a label and its
semantic style. The shared picker must preserve the complete caller-provided
`Line` style when it adds picker-owned content. Explicit styles on individual
spans, such as a stage or unstage row action, continue to override the inherited
line style. Focused selection adds its dark background and bold modifier without
replacing the status foreground or the deleted-file strikeout.

Do not add a theme, configuration, environment variable, alternate palette, or
status-specific behavior to the generic picker.

## Consequences

Changed files regain the compact visual grouping used before the shared-picker
extraction, while marker letters preserve meaning in monochrome or limited-color
terminals. Preserving the label's full line style also prevents future picker
composition from silently dropping semantic modifiers supplied by an activity.

The picker now treats a label's line-level style as part of its public rendering
contract. Callers that want only part of a label decorated must use explicitly
styled spans.
