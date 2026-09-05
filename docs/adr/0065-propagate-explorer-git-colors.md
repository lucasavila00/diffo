# ADR 0065: Propagate Git colors through Explorer folders

Refines the status-color contract now consolidated in
[ADR 0119](0119-separate-navigation-and-status-colors.md).

## Context

Explorer omits Git-status letters to preserve horizontal space, but a completely
neutral tree hides which paths contain repository changes. This is especially
unhelpful when a changed file is inside a collapsed folder.

VS Code file decorations can propagate from descendants to parent resources.
Diffo can provide the same navigation signal without adding badges, columns,
configuration, or work to the rendering loop.

## Decision

Color changed files in the Explorer picker with the existing fixed Git styles.
Semantic state does not add bold. Deleted paths are absent from Explorer as
required by ADR 0035.

Diff flat rows reserve a two-cell Git-status column followed by the file icon
and path. Explorer rows reserve two indentation cells per depth and a two-cell
disclosure column: collapsed/expanded glyphs for folders and spaces for files.
Explorer shows no status letters; icons stay adjacent to names.

Propagate status recursively to every ancestor directory while building the
complete Explorer tree. A directory stores the strongest descendant status using
this fixed precedence:

1. conflicted;
2. modified;
3. renamed or copied;
4. added or untracked.

Directories inherit only the foreground color. They are never struck out or made
bold because of a descendant. The folder icon and name receive the color;
disclosure carets retain their enabled-control style. Equal-priority pairs share
a foreground, so their internal tie order has no visual effect.

`TreeEntry::status` means direct Git status for files and strongest descendant
status for directories. Aggregation happens in `TreeBuilder::finish`, after
child entries are complete. It therefore covers every nesting level and does not
depend on which folders the picker currently exposes.

Explorer viewer titles remain neutral. Git letters remain exclusive to Diff, and
viewer gutter markers remain unchanged. No themes, settings, runtime detection,
or background tasks are added.

This keeps compact disclosure and the ban on Explorer status letters while
restoring Git-status colors as navigation aids.

## Consequences

- Collapsed folders reveal that their subtree contains changes.
- Mixed-status folders have one deterministic color.
- Explorer colors are a navigation aid; Diff remains the complete textual status
  view.
- Tree construction performs one bounded status fold over entries it already
  builds.

## References

- [VS Code file decoration propagation](https://github.com/microsoft/vscode/blob/main/src/vscode-dts/vscode.d.ts)
- [VS Code Git decoration provider](https://github.com/microsoft/vscode/blob/main/extensions/git/src/repository.ts)
