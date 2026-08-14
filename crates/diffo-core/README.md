# diffo-core

`diffo-core` defines Diffo's repository model and repository-source interfaces.

It contains snapshots, checkout history, file, commit, branch, and
active-operation state, typed checkout and merge targets, repository actions and
results, typed sync plans and progress, operation prompts, query and command
identifiers, sequenced repository updates, repository watch roots, and
cancellation handles. It also provides the deterministic fixture source used
during development and tests. Real Git command execution and prompt transport
live outside this crate.
