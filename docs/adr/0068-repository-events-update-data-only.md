# ADR 0068: Repository events update data only

## Decision

Repository events update repository data. They never directly reset user
interaction or presentation state.

The workbench accepts every sequenced repository update and rejects stale
generations. Branch discovery remains scoped by query ID, and prompts remain
scoped by command and prompt IDs; neither belongs to the repository generation.
The event loop drains all available updates before frame preparation, so
rendering sees only the final committed state.

Picker refresh reconciles query, selection, and scroll by stable identity while
taking labels, enabled state, ordering, and payload from the newest data.
Checkout identity is branch kind plus full ref; object ID is payload. Initial
load may choose a default. Later refreshes keep a selection when its stable
identity still exists, is enabled, and matches the query; otherwise they choose
the first enabled match or clear selection. Update items, selection, and scroll
in one commit. This applies to every open control receiving background data.

Command progress belongs to workbench presentation state. It appears only when a
command is still active 150 ms after dispatch, remains visible until that
command's terminal update, and never delays repository data. Watcher snapshots
cannot finish commands, hide progress, or clear toasts. Stage and Unstage
success changes only the lists; other successes retain their three-second toast,
and failures use the shared acknowledgement modal.
