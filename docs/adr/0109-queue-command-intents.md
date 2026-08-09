# ADR 0109: Queue command intents

Status: Accepted

Refines [ADR 0055](0055-command-queue.md) and
[ADR 0056](0056-own-deferred-execution-dependencies.md). Builds on the AI commit
flow in [PR #2](https://github.com/lucasavila00/diffo/pull/2).

## Decision

Let people enter the next few commands while one is still running. We expect these
queues to be short, usually two or three commands. For example, `a`, `i`, `9` means:

1. Stage everything.
2. Write a commit message with AI and commit.
3. Sync.

The queue remembers what the user asked for. It turns that request into a Git action
only when its turn arrives, using the snapshot left by the previous command. A running
command must not make an otherwise valid key press disappear.

If any command fails, cancel everything still waiting in the queue. Later commands
probably depend on the failed one, so continuing would be surprising and unsafe. Do
the same when the active command is cancelled. Cancelling a waiting command removes
that command and everything behind it. There is no pause, resume, or retry state.

Wait for a running cancellation to finish and install its final snapshot before the
queue becomes idle. Never start another command while cancellation is still in progress.

Show the queue immediately in a compact overlay. Show the active command, up to three
waiting commands, their order and state, `+N more` when needed, a cancel target on each
row, and `Cancel all`. Keep it visible when changing activities. Do not add hover
behavior.

AI message generation and the guarded commit are one queue item. Keep the same command
ID and cancellation handle, and change its label from generating to committing.

Use this queue for all user-started asynchronous repository, AI, and update commands.
Keep read-only background preparation and picker queries on their existing schedulers.

## Verification

Test `a`, `i`, `9` arriving together and prove that each intent uses the snapshot
installed by its predecessor. Cover repeated toggles, stale targets, preparation and
execution failures, active and queued cancellation, cancel-all, and the rule that no
discarded command starts.

Rendering tests cover queue order, state labels, overflow, cancellation hit targets,
small terminals, modals, and activity changes. End-to-end tests cover the complete AI
commit workflow without sleeps.
