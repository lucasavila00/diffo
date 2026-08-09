# ADR 0107: Publish only validated main commits

Refines [ADR 0075](0075-continuous-main-releases.md).

## Context

CI and release currently start independently on every push to `main`. This means
we can publish a commit before `make all` or the stress tests finish, and a
later failure does not undo that release.

## Decision

Make CI responsible for starting publication.

Keep two required CI jobs:

- `checks`, which runs `make all`; and
- `stress`, which runs the repeated tests under scheduler contention.

The `publish` job runs only for pushes to `main` and depends on both jobs
succeeding. A failure, timeout, cancellation, pull request, or non-main push
must not publish.

Turn the release workflow into a reusable `workflow_call` workflow instead of
giving it its own push or manual trigger. Pass it the exact commit SHA validated
by CI. Keep the workflow read-only until the final publication job, which alone
receives `contents: write`.

Serialize release-branch updates and skip an older validated commit if a newer
one is already published. The tagless `release` branch must never move backwards
when CI runs finish out of order.

## Consequences

Only commits that pass both normal checks and stress tests can reach the release
branch. Failed validation leaves the previous release unchanged. Adding another
release-blocking check means adding it to the publish job's dependencies.
