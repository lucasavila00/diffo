# diffo-repository-service

`diffo-repository-service` provides Diffo's serialized background repository
lane.

It classifies optional filesystem notifications into repository refresh requests
and independent worktree invalidations, collects full snapshots, serves
identified branch and merge-ref queries, and executes the one identified command
dispatched by the workbench. One worker serializes queries, refreshes, and
mutations so repository reads cannot race commands. Typed operation prompts
carry the active command identifier and answers bypass the blocked worker lane.
Cancelled commands return a post-operation snapshot. Snapshots, refresh
failures, worktree invalidations, and command terminal results leave the lane as
typed events. Queue lifecycle, presentation, and committed repository state stay
outside this crate.
