# ADR 0007: File navigation and staging

Status: Proposed

## UI

Use two panes:

```text
Files                  Diff
├─ Changes             Selected file diff
│  ├─ README.md
│  └─ src/main.rs
└─ Staged Changes
   └─ Cargo.toml
```

Build the left pane now. The right pane can show a placeholder until file diff work.

## State

```rust
struct App {
    snapshot: RepositorySnapshot,
    selected: Option<FileKey>,
    focus: Focus,
}

struct FileKey {
    path: PathBuf,
    area: ChangeArea,
}

enum ChangeArea {
    Unstaged,
    Staged,
}
```

A file can appear in both groups. Selection includes path and group.

After refresh, keep the same selection when it still exists. Otherwise select the
nearest item.

## Actions

- Up/down selects a file.
- Enter opens the file diff later.
- Stage stages the selected unstaged file.
- Unstage unstages the selected staged file.
- Stage all stages all changes and untracked files.
- Mouse click selects a file when mouse support is added.

Git commands:

```text
stage file    git add -- <path>
unstage file  git reset -- <path>
stage all     git add --all
```

Run Git outside rendering code. Refresh the snapshot after every action.

## Rules

- UI sends actions. Git code performs them.
- Show action errors. Do not hide them.
- Disable actions that do not fit the selected group.
- No discard button yet.
- No line or hunk staging yet.

## Done when

- Left pane shows Changes and Staged Changes.
- Keyboard navigation works.
- Selected row is clear.
- Stage, unstage, and stage-all work.
- Snapshot refresh keeps selection when possible.
- Mock mode can show and test both groups.
