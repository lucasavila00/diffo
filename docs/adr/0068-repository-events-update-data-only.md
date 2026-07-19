# ADR 0068: Repository events update data only

Status: Accepted

Builds on [ADR 0067](0067-preserve-interaction-state-across-refresh.md).

## Decision

Repository events update repository data. They never directly reset user interaction
or presentation state.

The workbench accepts every sequenced repository update and rejects stale generations.
Branch discovery remains scoped by query ID, and prompts remain scoped by command and
prompt IDs; neither belongs to the repository generation. The event loop drains all
available updates before frame preparation, so rendering sees only the final committed
state.

Picker refresh reconciles query, selection, and scroll by stable identity while taking
labels, enabled state, ordering, and payload from the newest data. Checkout identity is
branch kind plus full ref; object ID is payload.

Command progress belongs to workbench presentation state. It appears only when a command
is still active 150 ms after dispatch, remains visible until that command's terminal
update, and never delays repository data. Watcher snapshots cannot finish commands, hide
progress, or clear toasts. Stage and Unstage success changes only the lists; other
successes retain their three-second toast, and failures retain their persistent error
toast.
