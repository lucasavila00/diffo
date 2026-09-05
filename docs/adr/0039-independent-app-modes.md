# ADR 0039: Run activities as separate tools

## Problem

Diffo needs Explorer, Diff, and History activities. Each activity has different
state, input, rendering, and background work. One large model would couple them.
Rebuilding an activity on every switch would lose its state.

## Decision

Use one runtime, one workbench, and three long-lived tools.

The runtime owns the terminal, event loop, repository watcher, task execution,
and shutdown. It does not own screen state.

The workbench owns the active activity and one instance of each tool. It handles
the activity bar and global input. The bar order is Explorer, Search, Diff.
`Tab` follows that order and wraps. Quit is also global. All other input goes
only to the active tool.

Each tool owns its model, event handling, rendering, and task bookkeeping. The
workbench asks only the active tool to prepare and draw a frame. Switching
activity does not create, drop, reset, or copy tool state.

Keep the current diff model and renderer inside the Diff tool. Wrap them at the
tool boundary. Do not merge Explorer or Search state into them.

Tools return commands to the workbench. The runtime executes commands and
returns results to the tool that created them. Repository snapshots are shared
input and may be sent to every tool that needs them. Tools never hold references
to another tool and never mutate another tool's state.

Use the same small tool contract for every activity: handle an event, accept a
task result, prepare a frame, render a frame, and report pending work. The
workbench uses the active activity to dispatch to the matching concrete tool. No
shared mutable tool model is added.

The activity bar is a fixed rail on the full left edge. It selects activities.
It is not tab navigation. The workbench content, including the Diff status row,
starts to its right.

## Alternatives

- Add an activity field and all new state to the diff model. Rejected. It makes
  one model own unrelated products and risks current diff behavior.
- Keep one tool and replace its state on `Tab`. Rejected. Switching loses
  scroll, selection, query, and prepared work.
- Let tools call each other. Rejected. Ownership and task results become
  unclear.
- Run three event loops or terminals. Rejected. Terminal lifetime, input, and
  shutdown must have one owner.

## Result

Activities stay independent. Switching is only a workbench state change. Shared
behavior stays small and explicit.
