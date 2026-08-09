# diffo

`diffo` is a terminal UI for browsing and changing the current Git repository's
state.

This binary owns the terminal lifecycle and event loop. It connects the Git or
fixture repository source, serialized repository service, and the Diffo workbench while
ensuring that the terminal is restored on exit. The same binary has a private,
environment-marked askpass startup path reached through the running process's Linux
procfs entry and used only by Git and SSH network operations. Before repository or
terminal initialization, its launcher also dispatches the fixed `update` maintenance
entry path. Passive update checks start only after the first TUI frame.

Pressing `i` with staged changes asks the installed Codex CLI to generate a commit
subject with `gpt-5.6-luna`, then creates the commit. The request sends the staged diff,
repository and branch identity, and five recent commit subjects through the user's
existing Codex authentication. Codex runs ephemerally in a read-only sandbox; Diffo
does not manage credentials. Press `a` then `i` to stage everything and create the AI
commit as one keyboard flow. Manual commit-message editing remains available with `m`.

The application has no command-line arguments; the executable accepts only `update`.
Developer and test hooks are provided through environment variables; they are not
user configuration.
