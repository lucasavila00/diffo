# ADR 0079: Confirm pushes to main and master

Refines the confirmation policy in
[ADR 0070](0070-rebase-unpushed-work-when-syncing.md). Its sync algorithm
remains unchanged outside the confirmation boundary defined here.

## Context

Diffo's normal workflow is to commit on a branch and merge through a pull
request. Pushing directly to a repository's primary branch bypasses that review
path and is usually a mistake.

Sync currently fetches, selects a plan, and performs that plan without
confirmation. When the selected plan includes a push, it can therefore advance
`main` or `master` immediately. The existing branch indicator makes the current
branch visible, but visibility alone does not prevent an accidental direct push.

## Decision

After fetch selects a sync plan, require confirmation when both conditions are
true:

- the plan includes a push; and
- the configured upstream destination's short branch name is exactly `main` or
  `master`.

Match the destination branch, not merely the checked-out local branch. For
example, confirm a local `work` branch configured to push to `origin/main`, but
do not confirm a local `main` branch configured to push to `origin/archive`. The
match is case-sensitive and does not include names such as `main-next` or
`masterpiece`.

Do not open the modal for a sync that only fetches, fast-forwards, or finds no
work. Do not ask before fetch: the refreshed tips are required to know whether
the selected plan will push.

## Interaction

Show a workbench confirmation modal over the current activity. Name the
destination and the number of commits that the selected plan will push. Explain
that the normal workflow is to use a branch and pull request.

For example:

```text
Push 2 commits directly to origin/main?

This bypasses the branch and pull-request workflow.
```

Offer `Cancel` and `Push`, with `Cancel` selected first. Use the existing
confirmation picker controls: arrows, Enter, Esc, and mouse. Add no character
shortcut. Esc and an outside click cancel. `Ctrl+C` keeps its global quit
behavior.

The modal owns input while open. Global Sync controls and activity actions
cannot bypass it. Keep the sync pending and other repository actions disabled
until the user answers.

Confirming continues the already selected plan. It does not fetch again or
recompute the plan. Cancelling stops before rebase, fast-forward, or push. The
completed fetch may have updated the upstream remote-tracking ref, but
cancellation leaves the local branch and remote branch unchanged. Refresh the
displayed snapshot so the fetched upstream state is visible.

If repository state relevant to the selected plan becomes stale while the modal
is open, stop instead of executing the stale plan. The user must start Sync
again; a previous confirmation is never carried into another sync.

## Boundaries

This is a fixed safeguard, not configurable branch-protection policy. Do not add
a configuration file, environment variable, CLI option, remembered approval, or
a list of additional protected names.

The modal does not replace server-side branch protection. It does not block the
push permanently, create a branch, open a pull request, or change the
normal-push-only rule from ADR 0070. Force-pushes and other non-fast-forward
remote updates remain unsupported.

## Consequences

The common branch-and-pull-request workflow remains uninterrupted. Direct pushes
to `main` and `master` require one deliberate choice per sync.

A repository whose primary branch uses another name receives no confirmation.
Adding more fixed protected names or discovering a remote's primary branch
requires a separate decision.

## Verification

- Pure state-transition tests cover opening, cancel-first selection,
  confirmation, Esc, outside-click cancellation, modal input priority, and
  stale-plan rejection.
- Plan tests cover push and no-push rows for `main`, `master`, similarly
  prefixed names, case variants, and local names that differ from their upstream
  destination.
- Real-Git tests prove cancellation may update the remote-tracking ref but
  changes neither the local nor remote branch, and confirmation performs the
  selected normal push exactly once.
- A compiled PTY regression traces the fetched plan, modal frame, cancellation,
  and confirmed push without sleeps or delay environment hooks.
- `make all` passes.
