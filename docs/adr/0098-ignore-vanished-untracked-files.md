# ADR 0098: Ignore vanished untracked files

Status: Accepted

Refines [ADR 0012](0012-live-repository-refresh.md) and
[ADR 0077](0077-visible-and-bounded-repository-startup.md).

## Problem

Diffo asks Git for status, then reads every changed file to build its snapshot.
Those steps cannot be atomic.

Generators such as easyjson create and remove temporary files quickly. Git can report
one of those files as untracked, only for it to be gone when Diffo tries to read it.
Diffo treated that normal race as a failed repository refresh and opened an error
dialog.

This is not a Git lock, and there is nothing useful for the user to fix.

## Decision

If an untracked file disappears while Diffo is inspecting or reading it, leave that
stale entry out of the snapshot and continue the refresh.

Do this silently. Do not retry the whole snapshot and do not show a warning. During
heavy generator activity, a retry could simply find a different temporary file. The
watcher will request another refresh for files that survive.

Keep the exception narrow. Missing conflicted files, permission failures, other I/O
failures, and Git failures remain errors.

## Result

Temporary-file churn no longer interrupts the user or prevents unrelated changes
from refreshing. Tests cover both single-worker and parallel collection, and prove
that real failures still propagate. `make all` must pass.
