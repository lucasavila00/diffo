# ADR 0065: Propagate Git colors through Explorer folders

Status: Accepted

Refines [ADR 0050](0050-file-picker-status-colors.md) and
[ADR 0064](0064-separate-diff-status-and-tree-disclosure-columns.md).

## Context

Explorer omits Git-status letters to preserve horizontal space, but a completely
neutral tree hides which paths contain repository changes. This is especially
unhelpful when a changed file is inside a collapsed folder.

VS Code file decorations can propagate from descendants to parent resources. Diffo
can provide the same navigation signal without adding badges, columns, configuration,
or work to the rendering loop.

## Decision

Color changed files in the Explorer picker with the existing fixed Git styles. File
rows retain the conflict-specific bold modifier. Deleted paths are absent from
Explorer as required by ADR 0035.

Propagate status recursively to every ancestor directory while building the complete
Explorer tree. A directory stores the strongest descendant status using this fixed
precedence:

1. conflicted;
2. modified;
3. renamed or copied;
4. added or untracked.

Directories inherit only the foreground color. They are never struck out or made
bold because of a descendant. The folder icon and name receive the color; disclosure
carets retain their enabled-control style. Equal-priority pairs share a foreground,
so their internal tie order has no visual effect.

`TreeEntry::status` means direct Git status for files and strongest descendant status
for directories. Aggregation happens in `TreeBuilder::finish`, after child entries
are complete. It therefore covers every nesting level and does not depend on which
folders the picker currently exposes.

Explorer viewer titles remain neutral. Git letters remain exclusive to Diff, and
viewer gutter markers remain unchanged. No themes, settings, runtime detection, or
background tasks are added.

This replaces ADR 0064's prohibition on Git-status colors in Explorer while keeping
its compact disclosure layout and its ban on Explorer status letters.

## Consequences

- Collapsed folders reveal that their subtree contains changes.
- Mixed-status folders have one deterministic color.
- Explorer colors are a navigation aid; Diff remains the complete textual status
  view.
- Tree construction performs one bounded status fold over entries it already builds.

## Verification

- Test recursive propagation and mixed-status precedence.
- Test that repository refresh removes obsolete directory status.
- Render a collapsed changed folder and verify its caret stays neutral while its icon
  and name are colored.
- Verify the file-only conflict modifier.
- Verify viewer titles remain neutral and letter-free.
- Run `make all` when implementing the ADR.

## References

- [VS Code file decoration propagation](https://github.com/microsoft/vscode/blob/main/src/vscode-dts/vscode.d.ts)
- [VS Code Git decoration provider](https://github.com/microsoft/vscode/blob/main/extensions/git/src/repository.ts)
