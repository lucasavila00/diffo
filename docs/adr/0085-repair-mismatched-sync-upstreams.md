# ADR 0085: Repair mismatched sync upstreams

Refines the sync target in [ADR 0070](0070-rebase-unpushed-work-when-syncing.md)
and supersedes the mismatched-name behavior in
[ADR 0079](0079-confirm-protected-branch-pushes.md).

## Problem

A local branch can track a differently named remote branch. For example,
`git switch -c my-feature origin/master` configures `origin/master` as the
upstream.

Diffo used that upstream as the push destination and explicitly pushed
`HEAD:refs/heads/master`. This bypassed Git's default name-mismatch safeguard
and could push feature work directly to a protected branch.

## Decision

Sync always targets a remote branch with the same name as the local branch.

For a configured upstream:

1. Use its remote.
2. Derive `refs/heads/<local-branch>` as the sync and push target.
3. If the configured branch name differs, mark the upstream for repair.

Thus `my-feature` tracking `origin/master` syncs with `origin/my-feature` and
never pushes to `origin/master`.

If the same-named remote branch exists, plan against it using the normal sync
algorithm. Otherwise, create it through the existing first-publication path.
After every required Git operation succeeds, set the local upstream to the
same-named remote branch.

Failure, cancellation, or stale repository state leaves the original upstream
configuration unchanged. Protected-branch confirmation still applies to
same-named local `main` and `master` branches; it cannot approve a mismatched
push.

Diffo does not support separate integration and publication branches or Git's
configurable push-target policies.

## Consequences

Sync cannot silently advance a differently named remote branch. A deliberately
mismatched upstream is replaced after a successful sync.

The displayed plan, commit counts, protected-branch check, push refspec, and
resulting upstream all use one destination.
