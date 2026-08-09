# ADR 0078: Remove passive update checks and notices

Supersedes the passive-discovery and persistent availability-notice portions of
[ADR 0070](0070-publish-and-self-update-diffo.md). The release protocol and
explicit update workflow from that ADR remain accepted.

## Context

After the first application frame, Diffo started an unsolicited network check
for a new release. When one existed, it displayed a persistent informational
toast until the user dismissed it. The notice competed with repository and
command feedback, could obscure diff controls, introduced a second dismiss
target into tests and automation, and made an otherwise idle startup change
after it appeared complete.

The notice was especially disruptive while evaluating startup UX: it looked like
additional startup flashing and made unrelated interactions depend on network
timing. Users who want to update already have the fixed `update` launcher path
and the `Application: Update Diffo` command in every activity's command palette.

## Decision

Do not perform a passive update check when the application starts. Do not create
an availability toast or retain workbench state for one.

Keep explicit updates unchanged:

- the executable accepts the fixed `update` maintenance argument;
- every activity exposes `Application: Update Diffo` through the command
  palette;
- an explicit update runs outside the input loop and remains cancellable; and
- its success or failure result remains visible until dismissed.

Removing passive discovery also removes its silent background network request.
No new configuration, environment hook, CLI argument, or replacement
notification is added.

## Consequences

Diffo no longer tells users automatically that a release is available. Updating
is a deliberate action. Startup and idle behavior are quieter, deterministic
with respect to the release endpoint, and free of an unsolicited overlay.

The update task channel now carries only results of explicit update commands.
The updater client continues to perform the same metadata verification and
installation when explicitly invoked.

## Verification

- Black-box application tests can remain open without an update-availability
  toast or passive release-endpoint request.
- Command-palette tests continue to find and enqueue the explicit update command
  in every activity.
- Explicit update success and failure results remain persistent until dismissed.
- `make all` passes.
