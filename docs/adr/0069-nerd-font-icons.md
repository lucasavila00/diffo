# ADR 0069: Use Nerd Font glyphs for all interface icons

Builds on [ADR 0063](0063-terminal-native-file-type-badges.md).

## Context

Diffo already uses Nerd Font glyphs for file types and several controls, but
other icon roles use unrelated Unicode symbols. Mixing symbol sets gives the
activity rail, tree disclosure, progress, and navigation controls inconsistent
weight and alignment.

## Decision

Every icon in the Diffo interface uses a fixed, one-cell Nerd Font glyph.
`diffo-ui` owns the shared icon vocabulary, including activity, disclosure,
action, progress, change-navigation, and change-marker icons. Product behavior
and icon mappings remain fixed in code; Diffo does not detect fonts, provide
fallbacks, or make icons configurable.

This requirement applies to glyphs that pictorially identify an activity,
control, state, or action. It does not apply to text punctuation, key names,
diff content and Git-status letters, password masking, separators, borders, or
structural rails and gutters. Those characters retain their textual or layout
meaning.

Diffo therefore requires a terminal font with Nerd Font symbols. A terminal
without them may render missing-glyph boxes. This is preferable to maintaining
parallel icon sets with different widths and visual behavior.

## Consequences

- Activity, disclosure, edit, dismiss, resize, progress, navigation, and
  change-marker icons share one visual vocabulary.
- Icon literals are centralized in `diffo-ui`; file-type mappings remain in its
  dedicated file-icon table.
- Users must select a Nerd Font or use a terminal that supplies Nerd Font
  symbols.
- Textual arrows used to name arrow keys remain text and are not part of the
  icon set.
