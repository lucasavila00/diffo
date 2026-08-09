# diffo-app

`diffo-app` contains Diffo's long-lived activities and the workbench that
composes them.

The Diff activity owns its pure model, terminal input mapping, background buffer
preparation, and rendering. Explorer owns its tree, file viewer, and
file-loading worker. Review owns explicit AI generation, its ordered review
steps, and navigation while reusing the Diff renderer and the shared staging and
AI-commit command paths. The workbench owns activity selection, global command
lifecycle (including serialized application updates), the shared repository
footer, persistent update results, and the single active modal slot. Searchable
checkout and merge, missing-upstream remote selection, protected-branch push
confirmation, and activity modals use that slot without activities coordinating
with one another. Passive update discovery uses a persistent toast and never
takes focus.

State, input, preparation, rendering, and external work remain separate modules.
State transitions stay independent of terminal rendering and repository I/O so
they can be tested deterministically. The executable owns repository workers and
the terminal event loop.
