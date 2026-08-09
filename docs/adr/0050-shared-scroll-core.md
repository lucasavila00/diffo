# ADR 0050: One scroll core

## Problem

Scrollable panels duplicate wheel speed, bounds, clamping, and scrollbar
mapping. Rendering is not always the same.

## Decision

All scrollable panels use `diffo-ui` for:

- fixed mouse-wheel distance;
- maximum offset;
- bounded offset changes;
- scrollbar position count; and
- track-position to offset mapping.

No panel keeps private copies of this math or wheel speed.

Consumers are the Diff buffer, Explorer buffer, Staged picker, Changes picker,
and Explorer tree.

Rendering stays local:

- file pickers render plain vertical scrollbars;
- text buffers may render vertical and horizontal scrollbars; and
- Diff keeps its separate hunk-marker rail.

Wheel friction stays in raw input before routing, per ADR 0047. The shared core
changes scroll state only. It does not own rendering, hunk annotations,
selection, syntax work, or loading.

Refines ADRs 0033, 0043, 0047, and 0049.
