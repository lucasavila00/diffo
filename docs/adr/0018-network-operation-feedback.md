# ADR 0018: Network operation feedback

## Decision

Fetch, Pull, and Push use one pending network-operation state.

While one runs:

- animate the whole app border with a slow xterm-256 color gradient;
- show a spinner and the operation name in the footer;
- redraw every 16 ms;
- disable other network and primary actions;
- keep keyboard input and Ctrl+C responsive.

Clear the pending state after either a refreshed snapshot or an error. Show the
error after failed operations.

Use indexed colors so the animation works through SSH and `xterm-256color`.
