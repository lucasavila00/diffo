# ADR 0110: Queue command intents

Refines [ADR 0055](0055-command-queue.md) and
[ADR 0056](0056-own-deferred-execution-dependencies.md). Builds on the AI commit
flow in [PR #2](https://github.com/lucasavila00/diffo/pull/2).

## Decision

Let people enter the next few commands while one is still running. We expect
these queues to be short, usually two or three commands. For example, `a`, `i`,
`9` means:

1. Stage everything.
2. Write a commit message with AI and commit.
3. Sync.

The queue remembers what the user asked for. It turns that request into a Git
action only when its turn arrives, using the snapshot left by the previous
command. A running command must not make an otherwise valid key press disappear.

If any command fails, cancel everything still waiting in the queue. Later
commands probably depend on the failed one, so continuing would be surprising
and unsafe. Do the same when the active command is cancelled. Cancelling a
waiting command removes that command and everything behind it. There is no
pause, resume, or retry state.

Wait for a running cancellation to finish and install its final snapshot before
starting again. Commands entered after cancellation starts form a fresh queue
and wait for that snapshot; they must not disappear when cancellation finishes.

Show the queue immediately in its own fixed-size panel. Reserve four command
rows so the panel does not jump as work starts and finishes. Show the active
command, up to three waiting commands, their order and state, and `+N more` in
the title when needed. Each visible row has one cancel target. There is no
separate `Cancel all`: cancelling the active row already clears its tail, and
cancelling a waiting row removes it and everything behind it. Keep the panel
above activities, full-screen views, and modals so the controls match what
people see. Do not add hover behavior.

Every row has a stable goal and, when useful, a changing phase. Never name a
command after only its first step. Show `Sync — Fetching`, then
`Sync — Pushing`; show `AI commit — Generating commit message`, then
`AI commit — Committing`. New multi-step commands must follow the same rule.

AI message generation and the guarded commit are one queue item. Keep the same
command ID and cancellation handle while its phase changes.

Use this queue for all user-started asynchronous repository, AI, and update
commands. Keep read-only background preparation and picker queries on their
existing schedulers.
