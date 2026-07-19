# ADR 0018: Network operation feedback

Status: Accepted

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

## Tests

- Pure state tests cover Fetch, Pull, and Push pending state.
- Success and error both stop the pending state.
- Renderer tests cover the operation label and changing frame color.
- Compiled PTY tests hold real local-remote operations briefly and observe Fetching,
  Pulling, and Pushing before success.
- Compiled PTY tests verify disabled Commit and Push + Pull buttons cannot mutate Git.

End-to-end tests coordinate real Git processes with test-owned gates. Production code
does not delay Git to make operations observable.
