# ADR 0074: Reserve bold for mouse targets

Refines the structural and control-style contract in
[ADR 0118](0118-use-terminal-defaults-for-ui-surfaces.md) and the footer wording
in [ADR 0069](0069-show-commit-hash-in-footer.md).

## Context

Bold appears on both mouse controls and static content, so it cannot reliably
tell a user what can be clicked. Removing an action can also leave its old
emphasis behind, as happened when the footer branch target became informational
text.

## Decision

Reserve bold for a persistent visible affordance inside geometry that accepts a
mouse action. Mouse targets may use other established affordances, such as
scrollbar tracks, without bold. Static content, keyboard-only hints, selection,
focus, progress, and semantic state use text, color, backgrounds, cursors, or
animation instead.

Display named and unborn heads without the `branch` prefix and without bold.
Keep detached HEAD wording unchanged. All footer head information is read-only
text; branch checkout remains available through `Git: Checkout to...`. A clipped
footer control is regular and inert unless its complete mouse affordance is
visible.

## Consequences

Bold now has one interaction meaning across Diffo. Shared styling names that
meaning, and an architecture test rejects direct bold styling in renderers.
Render and input tests pair footer emphasis with hit testing so removing an
action cannot leave a false affordance behind.
