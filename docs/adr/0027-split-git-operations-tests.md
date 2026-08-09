# ADR 0027: Split `diffo/tests/git_operations.rs`

## Decision

Keep `git_operations.rs` as module declarations only.

- `git_operations/content.rs`: rename, conflict, metadata-looking content.
- `git_operations/staging.rs`: file and group stage actions.
- `git_operations/commit.rs`: composer and primary action.
- `git_operations/network.rs`: fetch, pull, push, toasts.
- `git_operations/overlays.rs`: palette, help, context UI.
- `git_operations/navigation.rs`: selection, panes, exit.
- `git_operations/scrolling.rs`: wheel, bars, hunk jumps.
- `git_operations/async_diff.rs`: delayed open and refresh regressions.
- `git_operations/support.rs`: repository, Git, wait, and data helpers.

Keep each test unchanged. Share setup through `support` only.
