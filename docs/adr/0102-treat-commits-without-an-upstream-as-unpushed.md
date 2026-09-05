# ADR 0102: Treat commits without an upstream as unpushed

## Problem

The Unpushed panel says `No upstream` when a branch has no upstream. That is
true, but not useful: the panel hides the commits a first push would publish.

## Decision

When a named branch has no upstream, treat every commit reachable from `HEAD` as
unpushed. Show the three newest commits in the existing Unpushed panel. If there
are older commits, add `... and more`; the recent-commit snapshot is bounded, so
Diffo does not pretend it knows the total.

Detached and unborn heads still show `No upstream`.

The read-only panel lists commits newest first using seven-character IDs and
terminal-safe subjects. With an upstream, show the exact remaining count after
the three visible entries. Without an upstream, the snapshot stays bounded and
uses only `... and more`. Its adaptive height remains below the file groups.
Install the branch identity, ahead/behind state, and commit summary atomically
as one repository snapshot. An unfinished merge replaces this panel as defined
by ADR 0108.

## Consequences

A new branch shows what its first push will publish. Branches with an upstream
keep using the upstream comparison and exact remaining count.
