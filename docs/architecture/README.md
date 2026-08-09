# Architecture

This directory is the source of truth for Diffo's current architecture:

- [AI commits](ai-commits.md) describes the Codex integration and its safety
  boundaries.
- [Sync](sync.md) describes the current branch synchronization algorithm.

Architecture decision records in the [decision log](../adr/) explain why
consequential choices were made. Living architecture pages describe the system
as it works now; decision records retain the context and tradeoffs from when a
choice was made. When implementation changes, update the relevant living page.
Add a decision record only when the rationale and alternatives will matter to
future maintainers.

## What Diffo is

Diffo is one terminal program. It shows the current Git repository. User can
read changes, explore files, and run Git actions. Its launcher accepts only the
fixed `update` maintenance argument; the application has no options or user
configuration.

## Main flow

1. `diffo` starts the terminal and event loop.
2. `diffo-git` talks to the system `git` command. Tests can use a fake
   repository from `diffo-core` instead.
3. `diffo-repository-service` runs repository reads and writes on one background
   lane. This stops refreshes and Git actions from racing.
4. `diffo-app::workbench` receives repository state. It owns the active screen,
   global input, command queue, progress, prompts, and results.
5. Each screen updates its own state and draws itself.
6. The event loop sends terminal input in and draws committed state out.
7. On exit, `diffo` restores the terminal.
8. `diffo-update` verifies and atomically installs signed GitHub release assets.
   The TUI checks in the background after its first frame and runs installation
   in a separate process through the shared command queue.

## Main parts

- `diffo-app::diff`: Diff state, input, buffer preparation, and drawing.
- `diffo-app::explorer`: repository tree and file viewer.
- `diffo-app::workbench`: joins screens and owns global behavior.
- `diffo-ui::command_palette`: command palette.
- `diffo-ui::file_picker`: shared file list and tree picker.
- `diffo-diff`: turns Git patches into inline or side-by-side rows.
- `diffo-highlight`: colors only the visible part of source files.
- `diffo-ui::text_view`: shared read-only text view and scrolling.
- `diffo-ui`: shared terminal components, layout, style, and scroll helpers.
- `diffo-core`: shared repository data and interfaces.
- `diffo-git`: real Git implementation.
- `diffo-repository-service`: background repository worker.
- `diffo-update`: signed discovery and atomic executable replacement.
- `diffo-e2e` and `diffo-measure`: test and performance tools.

## Important rule

Slow work happens away from drawing. A new diff becomes visible only when its
rows, syntax colors, navigation targets, and scroll state are all ready. Until
then, Diffo keeps showing the old complete view. This avoids half-built frames
and stale data.
