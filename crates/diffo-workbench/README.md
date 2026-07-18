# diffo-workbench

`diffo-workbench` composes Diffo's activities and routes global input.

It owns activity selection, command dispatch, cross-activity tasks, operation prompt
modals, and the shared workbench layout. The executable supplies repository work and
the terminal event loop; individual activities retain their own state and rendering.
