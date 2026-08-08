# ADR 0104: Skip local hooks when creating commits

Status: Accepted

Refines [ADR 0017](0017-commit-composer-and-primary-action.md).

## Context

Diffo currently creates a commit with `git commit -m <message>`. Git therefore runs
repository-local commit hooks, including `pre-commit` and `commit-msg`.

An explicit commit through Diffo is a human decision to record the currently staged
work, even when validation or continuous integration may fail. A local hook must not
override that decision. In contrast, hooks remain useful guardrails for agents and
other automated processes that invoke Git directly.

## Decision

Create commits from Diffo with `git commit --no-verify -m <message>`. Apply
`--no-verify` unconditionally inside the Git operation implementation; do not expose
a toggle, CLI option, environment variable, or confirmation.

This changes only the Git subprocess launched by Diffo. It does not change repository
hooks or Git configuration, so `git commit` run from a shell, agent, script, or other
client continues to run the hooks normally.

This decision covers `RepositoryAction::Commit`, which is the Commit control defined
by ADR 0017. It does not change Amend, Revert, merge, rebase, or commits created by
Git as part of another operation. Those paths have separate behavior and require a
separate decision if they are to bypass hooks.

Remote hooks remain authoritative. Push and Sync continue to report a rejection
from a remote hook as a push failure.

## Consequences

Commit completion is determined by the user's explicit action and the staged state,
not by whether local validation predicts that continuous integration will pass.
Local policy checks that run in `pre-commit` or `commit-msg` no longer protect commits
created through Diffo. They continue to protect ordinary CLI, agent, and automation
commits, while remote checks and continuous integration may still reject or report
problems with Diffo-created commits.

Diffo continues to pass the message as a typed process argument and keeps the
existing success, failure, cancellation, draft-preservation, and atomic snapshot
behavior.

## Code change proposal

- Add `--no-verify` to the `RepositoryAction::Commit` command constructed in
  `crates/diffo-git/src/operation.rs`.
- Replace the end-to-end regression that expects a local `pre-commit` hook to reject
  the composer commit with a regression that installs a failing hook and proves the
  commit succeeds without running it.
- Add a focused Git operation test that records hook execution and verifies both the
  resulting commit message and the absence of the hook side effect.

## Verification

- A failing executable `pre-commit` hook does not prevent a Commit action.
- A failing executable `commit-msg` hook does not prevent a Commit action.
- The requested message and staged tree are present in the created commit.
- A subsequent ordinary `git commit` in the same repository still runs its hooks.
- Remote hook rejection classification and presentation remain covered.
- `make all` passes.
