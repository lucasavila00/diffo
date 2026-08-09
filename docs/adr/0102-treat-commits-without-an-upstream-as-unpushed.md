# ADR 0102: Treat commits without an upstream as unpushed

Refines [ADR 0082](0082-unpushed-commits-panel.md).

## Problem

The Unpushed panel says `No upstream` when a branch has no upstream. That is
true, but not useful: the panel hides the commits a first push would publish.

## Decision

When a named branch has no upstream, treat every commit reachable from `HEAD` as
unpushed. Show the three newest commits in the existing Unpushed panel. If there
are older commits, add `... and more`; the recent-commit snapshot is bounded, so
Diffo does not pretend it knows the total.

Detached and unborn heads still show `No upstream`.

## Consequences

A new branch shows what its first push will publish. Branches with an upstream
keep using the upstream comparison and exact remaining count.

## Verification

Rendering tests cover a named branch without an upstream. `make all` passes.
