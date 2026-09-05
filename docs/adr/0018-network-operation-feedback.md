# ADR 0018: Long-running command feedback

## Decision

The workbench command queue in [ADR 0110](0110-queue-command-intents.md) owns
long-running repository-operation state.

While a command runs:

- animate the whole app border with a slow xterm-256 color gradient;
- show its goal and current phase in the command queue;
- redraw only for meaningful state changes or the bounded progress animation;
- queue later repository intents;
- keep keyboard input and Ctrl+C responsive.

Clear the active state only after the final snapshot or failure is installed.
Successful and informational results use the bounded toast queue; failures use
the shared acknowledgement modal.

Use indexed colors so the animation works through SSH and `xterm-256color`.
