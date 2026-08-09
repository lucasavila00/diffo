# ADR 0110: Explain slow startup

Status: Accepted

Refines [ADR 0076](0076-repeatable-startup-measurement.md) and
[ADR 0077](0077-visible-and-bounded-repository-startup.md).

## Context

Startup sometimes waits on Git for several seconds. A blank terminal makes Diffo look
hung and gives no clue which work is slow.

## Decision

Keep fast startup silent. If startup reaches three seconds, print the current phase to
stderr and print another short line when the phase changes. Stop the reporter before
entering terminal mode.

Keep the single complete first TUI frame. This is plain startup output, not a loading
screen or spinner.

## Consequences

Slow startup becomes understandable with very little terminal traffic. The alternate
screen hides these lines while Diffo runs; restoring the terminal may show them again
after exit. For slow runs, startup measurement's first output is now a progress line,
not the ready frame.

## Verification

Tests cover silent fast startup, the fixed delay, latest-phase selection, and output.
`make all` passes.
