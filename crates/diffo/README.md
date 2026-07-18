# diffo

`diffo` is a terminal UI for browsing and changing the current Git repository's
state.

This binary owns the terminal lifecycle and event loop. It connects the Git or
fixture repository source, background refresh work, and the Diffo workbench while
ensuring that the terminal is restored on exit. The same binary has a private,
environment-marked askpass startup path used only by Git and SSH network operations.

Diffo has no command-line arguments. Developer and test hooks are provided through
environment variables; they are not user configuration.
