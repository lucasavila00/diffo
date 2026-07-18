# diffo-core

`diffo-core` defines Diffo's repository model and repository-source interfaces.

It contains snapshots, file and commit state, repository actions and results, typed
network prompts, plus the deterministic fixture source used during development and
tests. Real Git command execution and prompt transport live outside this crate.
