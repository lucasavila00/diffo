# ADR 0055: Application command queue

Status: Proposed

Refines [ADR 0018](0018-network-operation-feedback.md),
[ADR 0051](0051-workbench-operation-toasts.md), and
[ADR 0052](0052-semantic-chrome-colors.md).

## Decision

Add one workbench-owned `CommandQueue` for asynchronous application commands. A command
has a stable identifier, display label, cancellation handle, and result-to-toast
projection. The queue runs one command at a time in FIFO order and exposes only explicit
states: `Queued`, `Running`, `Cancelling`, and a terminal result. Synchronous UI actions do
not enter the queue.

The queue is the only source of command progress. While an item is running or cancelling,
the workbench automatically:

- restores the slow xterm-256 pulsating gradient across the application border;
- renders the command label and spinner as a non-expiring progress toast; and
- renders a persistent, high-contrast `×` inside that toast's cancel hit target.

The progress toast is a projection of queue state, not a second stored lifecycle. Clicking
`×` requests cancellation. A queued item is removed immediately; a running item remains
`Cancelling` until its worker or child process acknowledges termination, and the next item
cannot start before that acknowledgement.

Success or failure removes the progress projection and automatically adds the command's
result to the existing workbench toast queue. Success follows normal expiry; failure stays
until dismissed. A successful cancellation creates no result toast. Queue state survives
activity changes, and its overlay is rendered and hit-tested by the workbench.

This restores ADR 0018's border gradient as the default loading presentation for every
queued command and replaces ADR 0052's prohibition on that transient progress color.
Idle structural chrome remains fixed dark gray.

## Verification

Test FIFO execution, single-command exclusivity, cancellation acknowledgement, and
activity switching. Rendering tests cover the animated gradient, progress toast, and `×`
hit target. End-to-end tests cancel a delayed Git command and verify that the next queued
command starts only after cancellation completes.
