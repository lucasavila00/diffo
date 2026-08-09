# ADR 0048: Put the last command first

Refines [ADR 0013](0013-command-and-file-actions.md). Activity ownership remains
as defined by [ADR 0039](0039-independent-app-modes.md).

## Context

Opening the F1 command palette with an empty query always shows commands in
catalog order. Repeating the command that was just used therefore requires
searching for it again or navigating back to its row. Command palettes such as
VS Code's make this common repetition faster by presenting recent commands
first.

Diffo has no configuration or user-state file. Its Diff, Explorer, and Search
activities each keep a separate, long-lived command palette so switching
activities does not copy or reset tool state.

## Decision

Each command palette remembers the last command it emitted for execution during
the current Diffo process. When that palette is opened with an empty query and
the remembered command is present in its current catalog, put that command
first. Keep the remaining commands in catalog order and select the first row.

Once the user types a query, use the existing fuzzy score and catalog-order
tie-breaker without a history boost. Clearing the query restores the
last-command ordering. Keyboard and mouse execution both update the remembered
command. Closing the palette, quitting, or attempting to execute an empty result
does not update it.

History stays in application memory and remains local to each activity's
palette. Do not add a history file, configuration, environment variable, CLI
option, full most-recently-used list, or cross-activity synchronization.

## Consequences

Repeating the last command in an activity requires only F1 and Enter. Search
results remain stable and continue to reflect relevance rather than prior use.
History is lost when Diffo exits, and a command used in one activity does not
reorder another activity's independent palette.
