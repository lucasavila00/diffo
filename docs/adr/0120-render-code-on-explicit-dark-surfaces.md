# ADR 0120: Render code on explicit dark surfaces

Refines the dark-background assumption in [ADR 0008](0008-diff-colors.md) and
preserves the Monokai syntax contract in
[ADR 0057](0057-consistent-code-view-syntax-style.md). Defines the code-view
exception to [ADR 0118](0118-use-terminal-defaults-for-ui-surfaces.md).

## Context

WT's terminal-default UI improvements do not solve Diffo's syntax-highlighting
limitation. `diffo-highlight` uses Monokai Extended RGB foregrounds, which
assume a dark background. `diffo-ui::plain_syntax_spans` currently emits those
colors on an unspecified background, so nearly white source text can disappear
when the terminal uses a light theme.

Replacing Monokai with terminal-palette syntax would sacrifice its colors and
syntax distinctions. That is a separate product change, not a prerequisite for
upstreaming WT's chrome improvements. Diffo should make its existing limitation
explicit and keep the source readable without changing the highlighter.

## Decision

Keep Monokai Extended and the existing foreground-only syntax contract. Render
source and diff content on an explicit fixed dark background in Explorer and
every Diff and History projection, including full-screen views. This applies
even when the surrounding terminal and application chrome use a light theme.

The code surface owns its background and fallback foreground as a pair. Use a
fixed dark RGB or extended xterm color compatible with Monokai, not the
terminal-default background or the configurable ANSI black slot. Use Monokai's
ordinary light foreground for unhighlighted text instead of inheriting the
terminal's foreground. Unsupported languages and files above the syntax
eligibility limit use the same readable surface.

Fill the complete code viewport, including empty rows, trailing row padding, and
both side-by-side cells. Code gutters, line numbers, and inline metadata on that
surface need explicit readable foregrounds as well. Keep surrounding pane
borders, controls, and overlays governed by the terminal-default chrome
contract; apply content styles within the code area without allowing them to
leak into adjacent UI. Scrollbars and change-marker rails retain their separate
geometry and readable styles on their own surfaces.

Preserve the existing added, removed, and conflict backgrounds and their
full-row or side-cell coverage. They override the base dark code surface.
Preserve the existing 4.5:1 contrast correction for syntax on those backgrounds;
do not recolor Monokai tokens on ordinary context rows. Fallback text and gutter
content on changed rows also need explicit contrast-safe foregrounds rather than
a light terminal's potentially dark default text.

Own the common code-surface styles at the shared rendering boundary. Explorer
and the shared Diff/History review renderer consume the same base surface;
`diffo-highlight` continues to own syntax definitions and Monokai foregrounds.
This does not introduce syntax-role classification, another theme, terminal
palette queries, user configuration, or a theme-switching lifecycle.

Preserve the bounded, atomic preparation contract: visible syntax coverage,
parser look-behind and byte budget, the strict 10,000-line eligibility boundary,
the sub-100 ms 9,999-line reference benchmark, and the four-entry prepared
file-and-mode cache. Painting a background does not require additional parsing
or another content transition.

## Alternatives

Leaving the code background unspecified preserves the current light-terminal
readability bug. Replacing Monokai with terminal-palette colors avoids a fixed
dark surface but loses the syntax appearance we want to retain.

Automatic light/dark syntax themes would require a separate decision about
colors, terminal detection, fallback, and runtime transitions. Defer that work
instead of bundling it with WT's UI improvements.

## Consequences

Diffo's chrome can follow a light terminal, but its code views remain dark. This
is partial light-mode support and an acknowledged limitation of the current
syntax-highlighting design. The main reading surface will not match a light
terminal until a separate syntax-theme change addresses it.

Monokai colors and the existing diff semantics remain intact. Explicit paired
content colors make the current design readable in either terminal theme without
pretending that Diffo has a light syntax theme.
