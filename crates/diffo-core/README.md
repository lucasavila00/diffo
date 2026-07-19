# diffo-core

`diffo-core` defines Diffo's repository model and repository-source interfaces.

It contains snapshots, file, commit, and branch state, typed checkout targets,
repository actions and results, typed network prompts, query and command identifiers,
sequenced repository updates, and cancellation handles. It also provides the
deterministic fixture source used during development and tests. Real Git command
execution and prompt transport live outside this crate.
