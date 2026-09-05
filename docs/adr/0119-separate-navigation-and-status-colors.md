# ADR 0119: Separate navigation and status colors

Extends [ADR 0118](0118-use-terminal-defaults-for-ui-surfaces.md) and refines
[ADR 0052](0052-semantic-chrome-colors.md).

## Context

WT first moved selected cards from full-content reversal to yellow borders in
`c93c8702`. Yellow then meant both selection and a state needing attention. WT's
[ADR 0072](https://github.com/lucasavila00/wt/blob/734053ab825335a50d9948614da144e1dea4cd38/docs/adr/0072-use-blue-as-the-ui-highlight-color.md)
and [commit bf6ec616](https://github.com/lucasavila00/wt/commit/bf6ec616)
resolved that ambiguity with blue selected borders and active activity icons and
markers.

Diffo's current activity rail applies the same white, bold mouse-target style to
active and inactive icons. Its marker identifies the active activity, but the
shared palette has no navigation-accent role. Yellow already means modified
files and attention; cyan already means information and renamed/copied files.
Neither should become the general navigation color.

## Decision

Give application UI colors fixed semantic roles owned by `diffo-ui`:

| Role                          | Terminal color     | Use                                                               |
| ----------------------------- | ------------------ | ----------------------------------------------------------------- |
| Navigation accent             | Blue               | Active activity icon and marker; selected framed-content boundary |
| Attention                     | Yellow             | Warnings, pending attention, modified-file status                 |
| Danger                        | Red                | Errors, conflicts, destructive action labels, deleted-file status |
| Success                       | Green              | Successful operations, added/untracked files, healthy states      |
| Information                   | Cyan               | Informational messages and renamed/copied-file status             |
| Ordinary content and controls | Default foreground | Labels and enabled actions, with emphasis from ADR 0118           |

Use the terminal's normal semantic slots rather than requiring the bright
variants currently used for success, danger, and information on default
surfaces. These assignments concern UI and Git metadata, not language syntax
categories or explicit diff-surface foreground/background pairs.

Apply blue to both the active activity icon and its existing leading marker.
Inactive enabled icons retain default foreground and their interactive emphasis.
Blue does not replace the reversed text/list/form selections in ADR 0118, and it
does not wash over code, status labels, or a complete content pane.

Keep meaning in labels and symbols as well as color. A warning remains a warning
in monochrome; an active activity still has its marker. Retain Diffo's existing
Git distinctions, including deletion strikethrough, conflict symbols, and status
columns. Do not turn every yellow or red span blue merely because it can be
clicked.

In particular, previous/next change links and hunk-marker rails remain colored
by their target's change kind under
[ADR 0079](0079-color-change-navigation-by-target.md) and
[ADR 0114](0114-clickable-change-navigation-links.md). Those colors describe the
destination's Git meaning. Their clickable styling comes from the shared control
affordance, not from replacing the semantic color with navigation blue.

## Alternatives

One highlight color for both selection and attention recreates WT's original
ambiguity. Coloring every enabled control blue would conflate availability with
the currently active location. Retaining white active icons would omit the
specific distinction WT added, even after terminal-default text is corrected.

## Consequences

Navigation, attention, and operation outcomes become distinct across activities
without introducing another selectable theme. The blue accent supplements the
existing fixed interaction model; it adds no hover behavior or input handling.
Arbitrary terminal palettes can still assign poor accent colors, so the
non-color indicators remain necessary.
