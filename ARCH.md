# Architecture

## What Diffo is

Diffo is one terminal program. It shows the current Git repository. User can read
changes, explore files, and run Git actions. It has no command-line options or user
configuration.

## Main flow

1. `diffo` starts the terminal and event loop.
2. `diffo-git` talks to the system `git` command. Tests can use a fake repository
   from `diffo-core` instead.
3. `diffo-repository-service` runs repository reads and writes on one background
   lane. This stops refreshes and Git actions from racing.
4. `diffo-workbench` receives repository state. It owns the active screen, global
   input, command queue, progress, prompts, and results.
5. Each screen updates its own state and draws itself.
6. The event loop sends terminal input in and draws committed state out.
7. On exit, `diffo` restores the terminal.

## Main parts

- `diffo-app`: state and rules for the Diff screen. No terminal or Git work.
- `diffo-tui`: draws the Diff screen and prepares visible diff buffers.
- `diffo-explorer`: repository tree and file viewer.
- `diffo-command`: command palette.
- `diffo-file-picker`: shared file list and tree picker.
- `diffo-diff`: turns Git patches into inline or side-by-side rows.
- `diffo-highlight`: colors only the visible part of source files.
- `diffo-text-view`: shared read-only text view and scrolling.
- `diffo-ui`: shared layout, style, text, and scrollbar helpers.
- `diffo-core`: shared repository data and interfaces.
- `diffo-git`: real Git implementation.
- `diffo-repository-service`: background repository worker.
- `diffo-workbench`: joins screens and global behavior.
- `diffo-e2e` and `diffo-measure`: test and performance tools.

## Important rule

Slow work happens away from drawing. A new diff becomes visible only when its rows,
syntax colors, navigation targets, and scroll state are all ready. Until then, Diffo
keeps showing the old complete view. This avoids half-built frames and stale data.
