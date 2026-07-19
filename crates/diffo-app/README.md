# diffo-app

`diffo-app` contains Diffo's long-lived activities and the workbench that composes
them.

The Diff activity owns its pure model, terminal input mapping, background buffer
preparation, and rendering. Explorer owns its tree, file viewer, and file-loading
worker. The workbench owns activity selection and global command lifecycle.
It also owns the shared repository footer and the single active modal slot; activities
request their contextual modal without coordinating with other activities.

State, input, preparation, rendering, and external work remain separate modules.
State transitions stay independent of terminal rendering and repository I/O so they
can be tested deterministically. The executable owns repository workers and the
terminal event loop.
