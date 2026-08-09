# ADR 0063: Use one-cell Unicode file icons

Refines [ADR 0049](0049-shared-file-picker.md),
[ADR 0054](0054-readable-tree-labels-and-controls.md), and
[ADR 0061](0061-align-explorer-tree-names.md).

## Goal

Show folder and file-type icons. Spend one cell per row.

## Assumption

Diffo targets the latest stock Ghostty.

Ghostty bundles JetBrains Mono and Nerd Font symbols. Since Ghostty 1.2, Nerd
Font symbols work without installing or selecting a patched font.

## Prior art

- VS Code matches full file names, compound extensions, extensions, languages,
  and folder state.
- `nvim-web-devicons` maps file names and extensions to one glyph.
- `eza`, `lsd`, and Yazi use the same pattern: one leading glyph selected from
  file type, file name, or extension.

## Options

- Text badges such as `rs`. Rejected. Three cells.
- Emoji. Rejected. Often two cells.
- Terminal images. Rejected. Too much machinery.
- One-cell Nerd Font glyphs. Chosen. Stock Ghostty supports them.

## Decision

Render one icon directly before the name:

```text
main.rs
package.json
src
```

No separator space.

- Files use a language or file-type icon.
- Unknown files use a generic file icon.
- Closed and open folders use different folder icons.
- The folder icon replaces `▸` and `▾`. It shows both entry kind and expansion
  state.
- Git-status letters stay. Icons do not replace status.
- Names at the same tree depth stay aligned.
- Icons inherit the row foreground. No language-specific colors.
- Diff and Explorer use the same icon lookup.

`diffo-ui` owns the fixed icon table. Match in this order:

1. complete file name;
2. longest compound extension;
3. final extension;
4. generic file icon.

Use fixed code points known to exist in stock Ghostty. Do not load themes,
inspect file contents, or detect fonts at runtime.

## Tradeoffs

- Compact. One cell.
- No setup on stock Ghostty.
- Other terminals may show missing glyphs.
- The fixed table covers fewer files than a configurable icon theme.

## Verification

- Assert every icon has terminal width one.
- Test file-name, compound-extension, extension, folder-state, and fallback
  matches.
- Test the same icons in Diff and Explorer.
- Test file and folder alignment.
- Test Git-status styling and narrow layouts.
- Run `make all` when implementing the ADR.

## References

- [Ghostty 1.2 built-in Nerd Font support](https://ghostty.org/docs/install/release-notes/1-2-0)
- [VS Code file icon matching](https://code.visualstudio.com/api/extension-guides/file-icon-theme)
- [nvim-web-devicons](https://github.com/nvim-tree/nvim-web-devicons)
- [eza icon table](https://github.com/eza-community/eza/blob/main/src/output/icons.rs)
