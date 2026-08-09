# ADR 0036: Show the current Git branch and state

## Problem

Diffo already collects the current branch, upstream divergence, and changed
files, but the normal footer shows only command help. A user can review, stage,
commit, pull, or push without a persistent reminder of which branch those
actions affect.

`BranchState { name: Option<String> }` also treats an unborn branch and detached
HEAD as the same value. That is not enough for an accurate branch control or for
a checkout result.

VS Code keeps the current ref in its status bar and changes the branch indicator
to reflect conflicts, staged changes, and working-tree changes. Diffo should use
the same information hierarchy in a terminal-safe form.

## Decision

Reserve the left side of the footer for a persistent Git status segment. Keep
transient operation and error text in the middle and command help on the right.

Examples:

```text
branch main · clean                         1/f1: commands  2/f2: help
branch feature/search · changes · ↓1 ↑2    Pulling…
detached a1b2c3d · conflicts               Checkout failed: local changes
```

The segment contains:

1. the current named branch, unborn branch, or seven-character detached commit;
2. one repository-state label;
3. upstream divergence when either count is non-zero.

Use this fixed state priority, matching the useful part of VS Code's
branch-status indicator:

```text
conflicts > staged > changes > clean
```

`conflicts` means at least one conflicted file. `staged` means there are staged
changes and no conflicts. `changes` means there are unstaged or untracked
changes and neither higher-priority state applies. The staged and working-tree
file groups continue to show the full detail; the footer is only a compact
warning about the state in which the next Git action will run.

Do not communicate state by color alone. Use the existing conflict, staged, and
change colors in addition to the text labels. Render divergence as `↓N ↑N`,
where down is behind and up is ahead. Omit both counts when both are zero or no
upstream exists.

Replace the optional branch name with an explicit head state:

```text
HeadState
  Named { name, commit }
  Unborn { name }
  Detached { commit }
```

Parse `branch.head` and `branch.oid` from the existing
`git status --porcelain=v2 --branch -z` result. Derive the compact
repository-state label and file counts from the same immutable
`RepositorySnapshot`; do not run Git from the renderer and do not maintain a
second status cache.

The branch segment is a mouse target. Clicking it opens the same checkout picker
as the `Git: Checkout to...` palette command defined by
[ADR 0037](0037-git-checkout-to.md). It does not add a keyboard shortcut.

## Rendering and refresh

Branch, divergence, and file state are one snapshot commit. A refresh must not
show a new branch with the previous branch's files or counts. While checkout
runs, keep the last committed segment visible and show `Checking out <target>…`
as transient text. Replace the segment only when the checkout result and its
complete snapshot are installed together.

At narrow widths, preserve the head label first. Truncate a long branch name
with a single ellipsis, then omit divergence, the state label, command help, and
transient detail in that order. Errors may replace command help but must not
replace the head label. Keep all clipping inside the footer; it must not change
the pane layout.

## Alternatives

- Put the branch in the file-pane title. Rejected because it disappears with
  that activity and competes with file-list information.
- Show only color or a compact symbol. Rejected because terminal fonts and color
  perception vary, and the state would be ambiguous.
- Re-read Git during rendering. Rejected because rendering consumes committed
  state only and may not block on external commands.
