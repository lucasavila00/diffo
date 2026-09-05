# ADR 0055: Application command queue

Refines [ADR 0018](0018-network-operation-feedback.md),
[ADR 0051](0051-workbench-operation-toasts.md), and the structural style
contract now consolidated in
[ADR 0118](0118-use-terminal-defaults-for-ui-surfaces.md).

## Decision

Add one workbench-owned `CommandQueue` for asynchronous application commands. A
command has a stable identifier, display label, cancellation handle, and
result-to-toast projection. The queue runs one command at a time in FIFO order
and exposes only explicit states: `Queued`, `Running`, `Cancelling`, and a
terminal result. Synchronous UI actions do not enter the queue.

The workbench queue is the only command scheduler. `diffo-repository-service`
owns the single background repository lane and executes only the active command
dispatched by the workbench. It serializes that execution with watcher refreshes
but does not own, copy, or infer command queue state.

Askpass prompts are scoped to the active command ID. The repository service
brokers their one-shot answers outside its blocked worker lane, but the
workbench owns the modal and cancellation state. Cancelling a prompt cancels the
whole queued command through the same cancellation handle. The next command
waits for process and prompt cleanup to produce the active command's terminal
event.

The queue is the only source of command progress. While an item is running or
cancelling, the workbench automatically:

- restores the slow xterm-256 pulsating gradient across the application border;
- renders the command label and spinner as a non-expiring progress toast; and
- renders a persistent, high-contrast `×` inside that toast's cancel hit target.

The progress toast is a projection of queue state, not a second stored
lifecycle. Clicking `×` requests cancellation. A queued item is removed
immediately; a running item remains `Cancelling` until its worker or child
process acknowledges termination, and the next item cannot start before that
acknowledgement.

Success or failure removes the progress projection and automatically adds the
command's result to the existing workbench toast queue. Success follows normal
expiry; failure stays until dismissed. A successful cancellation creates no
result toast. Queue state survives activity changes, and its overlay is rendered
and hit-tested by the workbench. Failure and cancellation retain the last
committed repository snapshot; only successful command completion installs a
replacement snapshot.

This restores ADR 0018's border gradient as the default loading presentation for
every queued command. Idle structural chrome remains terminal-default and dim as
defined by ADR 0118.
