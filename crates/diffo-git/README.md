# diffo-git

`diffo-git` implements Diffo's repository interfaces with the system `git` command.

It collects repository snapshots, loads Explorer files, and performs staging,
commit, branch, and network operations. The resulting state and errors use the
transport-neutral types from `diffo-core`. Network operations expose only typed,
validated askpass prompts through an operation-scoped Unix-socket bridge; Git and SSH
never receive Diffo's terminal input. Command execution observes operation cancellation
and does not acknowledge it until the Git process group and askpass bridge have stopped.
