# diffo-workbench

`diffo-workbench` composes Diffo's activities and routes global input.

It owns activity selection, the FIFO application command queue, command progress and
result overlays, cross-activity tasks, and the shared workbench layout. The
executable supplies repository workers and the terminal event loop; individual
activities retain their own state and rendering.
It also owns operation prompt modals and binds them to the active command so prompt
input and cancellation share the queue lifecycle.
