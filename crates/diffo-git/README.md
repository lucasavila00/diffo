# diffo-git

`diffo-git` implements Diffo's repository interfaces with the system `git` command.

It collects repository snapshots, discovers local and remote branches, walks the
worktree and loads files for Explorer, and performs staging, safe branch checkout,
commit, fetch, and sync operations.
Sync always fetches, then fast-forwards, pushes, or rebases local-only commits and
pushes according to the fixed repository state. Conflicting rebases are aborted and
sync never creates a merge commit or force-pushes. A sync that would push to `main` or
`master` pauses for explicit confirmation and rejects repository changes made while
that confirmation is open. The resulting state and errors use
the transport-neutral types from `diffo-core`. Network operations expose only typed,
validated askpass prompts through an operation-scoped Unix-socket bridge; Git and SSH
never receive Diffo's terminal input. Git and SSH re-enter the running Diffo image
through its Linux procfs path, so replacing or unlinking the launched pathname cannot
break or redirect a later prompt and no executable copy is required. Command execution
observes operation cancellation and does not acknowledge it until the Git process group
and askpass bridge have stopped.
