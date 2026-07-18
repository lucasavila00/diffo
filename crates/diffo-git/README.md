# diffo-git

`diffo-git` implements Diffo's repository interfaces with the system `git` command.

It collects repository snapshots, loads Explorer files, and performs staging,
commit, branch, and network operations. The resulting state and errors use the
transport-neutral types from `diffo-core`.
