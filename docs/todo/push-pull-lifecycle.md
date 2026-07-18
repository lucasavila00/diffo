# TODO: Push and pull lifecycle

Current rule: when the branch is both ahead and behind, show disabled `Push + Pull`.
Do not run Git.

Before enabling sync, decide and test:

- Fetch timing. Ahead/behind data may be stale.
- Merge versus rebase versus fast-forward-only pull.
- What happens with staged, unstaged, and untracked changes.
- Autostash policy.
- Conflict flow and abort/continue actions.
- Upstream missing, renamed, deleted, or not writable.
- Detached HEAD and unborn branches.
- Push rejection and non-fast-forward recovery.
- Force push and force-with-lease policy. Never force by default.
- Authentication, credentials, prompts, cancellation, and timeouts.
- Progress UI while network work runs.
- Retry rules and exact error messages.
- Whether Push + Pull means pull then push, fetch then rebase then push, or a menu.

Add real local-remote E2E cases for every chosen path. Keep all Git work outside the
input and render threads.
