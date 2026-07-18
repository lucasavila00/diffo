# diffo-tui

`diffo-tui` renders Diffo's Diff activity and maps terminal events into application
messages.

It owns diff-buffer preparation, viewport transitions, file-panel presentation, and
activity-local overlays. It also provides the shared rendering and hit geometry for
workbench command progress. Rendering consumes committed state; repository I/O and
top-level terminal lifecycle are handled by other crates.
