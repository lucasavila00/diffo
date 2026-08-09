# Architecture decision records

This directory is Diffo's architecture decision log. Each record preserves one
consequential choice, its context, and its tradeoffs. For the current system
design, start with the [living architecture documentation](../architecture/).

## Adding a decision

1. Use a four-digit number higher than every existing record, followed by a
   short kebab-case title: `0112-example-decision.md`.
2. Record at least the context, decision, and consequences. Include considered
   alternatives when they clarify the tradeoff.
3. Preserve merged records as history. Record a changed choice in a new ADR and
   link the related records.
4. Update the relevant page under [`docs/architecture/`](../architecture/) when
   the implemented system changes.

Existing identifiers and filenames remain stable even when old merge history
produced duplicate numbers. New records always use a number above the current
maximum, avoiding new collisions without breaking old links.
