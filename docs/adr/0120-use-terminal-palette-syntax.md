# ADR 0120: Use terminal-palette syntax on default surfaces

Changes the fixed Monokai foreground decision in [ADR 0008](0008-diff-colors.md)
and [ADR 0057](0057-consistent-code-view-syntax-style.md). Extends
[ADR 0118](0118-use-terminal-defaults-for-ui-surfaces.md).

## Context

WT's theme-safe shell renders its own labels using terminal defaults and
displays application content as terminal cells. It does not provide a source
syntax highlighter that Diffo can copy. Upstreaming WT's chrome changes
therefore leaves a Diffo-specific problem: `diffo-highlight/src/engine.rs` fixes
token foregrounds to Monokai Extended, and `diffo-ui::plain_syntax_spans` emits
their RGB values on an unspecified background. Monokai's nearly white ordinary
text can disappear against a light terminal surface.

Diff adds another boundary. `diffo-app/src/diff/view/style.rs` uses explicit
xterm backgrounds 52, 22, and 58 for removed, added, and conflict rows, and
lightens syntax foregrounds to meet a 4.5:1 contrast ratio against those known
backgrounds. Merely changing ordinary text to the terminal default can put a
light terminal's dark foreground on a dark diff row, especially for unsupported
languages, files above the syntax limit, and metadata without syntax spans.

This decision is a Diffo-specific extension of WT's terminal-ownership
principle, not an improvement already implemented in WT.

## Decision

Use a fixed semantic syntax palette expressed as terminal color roles on
terminal-default code surfaces. Keep the bundled syntax definitions and scope
parsing in `diffo-highlight`; classify syntax scopes into shared roles rather
than exposing only Monokai RGB values as the presentation contract. The scope
mapping belongs to the highlighter, not to individual renderers, and must not be
inferred by guessing the nearest color to an existing RGB value.

Ordinary identifiers, whitespace, punctuation, and fallback text use the default
foreground. Comments also use the default foreground, without dimming essential
source text. Keywords use magenta, strings green, numeric and literal constants
cyan, types yellow, and function names blue. These are syntax roles, distinct
from the application-status roles in ADR 0119. The mapping is fixed in code and
uses normal terminal palette slots. Preserve foreground-only syntax: no syntax
backgrounds, bold, italic, underline, or reversal.

Explorer and unchanged/context source rows in every Diff and History projection
resolve the same roles to the same terminal foregrounds. Terminal palette and
default-color changes recolor those cells without detection, reparsing, a second
frame, or an application theme setting. Unsupported syntax and ineligible files
remain readable default text.

Retain the existing explicit added, removed, and conflict backgrounds and their
full-row or side-cell coverage. On those surfaces, resolve the same syntax roles
through fixed foreground colors paired with the known backgrounds, then apply
the existing 4.5:1 contrast correction. Use fixed xterm/RGB foregrounds, not
terminal-default or ANSI semantic slots whose actual RGB value is unknown.
Ordinary and unhighlighted text, line numbers, change symbols, metadata, and
conflict text on a filled surface need that explicit contrast-safe foreground as
well; readability cannot depend on syntax eligibility. Empty row padding keeps
the semantic background without introducing text color assumptions.

The source role mapping is shared, while surface resolution belongs at the
shared rendering boundary. `diffo-highlight` remains independent of Ratatui.
Diff and History consume the same resolution through their shared review
renderer under [ADR 0117](0117-share-one-atomic-review-pipeline.md); Explorer
uses the default-surface resolver. A selection accent must not replace these
foreground/background pairs or reverse a whole code pane.

Preserve the existing bounded preparation architecture: visible viewport,
256-line parser look-behind, fixed byte budget, strict 10,000-line syntax
eligibility boundary, sub-100 ms 9,999-line reference benchmark, and four-entry
prepared file-and-mode cache. Syntax roles are part of prepared coverage and
commit atomically with the buffer and viewport. Palette ownership introduces no
new background-result installation point or full-file opening work.

## Alternatives

Keeping Monokai RGB on terminal-default surfaces leaves light mode broken.
Giving all code an explicit dark surface would make it readable but leave the
main reading area permanently dark in a light terminal.

Automatically selecting separate light and dark RGB themes requires terminal
reports, unsupported-terminal fallback, multiplexer behavior, and atomic runtime
theme/cache transitions. WT's ADR 0003 shows that even the intermediary can
cache stale reports. Terminal palette roles avoid that additional lifecycle.

Removing syntax entirely or removing all diff backgrounds would simplify color
handling but discard existing source and change distinctions. Keeping paired
colors only on the already explicit semantic surfaces preserves those benefits.

## Consequences

Light support covers source content as well as surrounding chrome. Source colors
follow the terminal palette, and Monokai's exact appearance and fine-grained
color distinctions are intentionally replaced with a smaller fixed vocabulary.
Comments remain readable at the cost of less visual de-emphasis. Changed rows
retain their existing dark semantic fills even in a light terminal; they are
bounded change indicators with explicitly paired text, not the default reading
surface.

Diffo can enforce numeric contrast for its fixed diff surfaces, but cannot
promise a numeric contrast ratio for an unknown user-defined terminal palette.
Default text and the terminal's own syntax accents provide the same ownership
model as WT's UI. This avoids adding user configuration while making the
appearance responsive to terminal light/dark changes.
