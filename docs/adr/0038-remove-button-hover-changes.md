# ADR 0038: Remove button hover changes

Status: Accepted

## Problem

Diffo is intended to be used over SSH. In a terminal session, passive mouse movement
can travel from the local terminal to the remote process, and a hover-style change
can cause the remote process to render changed cells back across the connection.
That input, state tracking, rendering work, and terminal output consume network and
CPU resources even though hovering does not perform an action.

The large previous-change and next-change buttons currently change color when the
pointer moves over them. This gives little value: their labels and fixed placement
already make them discoverable, and a click provides the interaction users need.
The cosmetic feedback is not worth its recurring cost, especially on slow,
high-latency, or metered SSH connections.

## Decision

Buttons keep one stable visual style while the pointer moves over them. Do not add
hover-driven color, emphasis, border, label, or other visual changes to any Diffo
button or control.

- Keep mouse clicks, drags, and wheel actions where they are otherwise supported.
- Do not store hover-only application or renderer state.
- Do not request or process passive mouse-movement events solely to implement hover
  feedback.
- Do not redraw solely because the pointer entered, moved within, or left a control.
- Prefer stable labels and clear control geometry for discoverability.

This supersedes the hover styling, hover state, and hover test requirements in
[ADR 0022](0022-large-hunk-navigation-targets.md). Its hunk-button placement,
visibility, click behavior, and keyboard navigation decisions remain in force.

## Consequences

Buttons no longer acknowledge pointer proximity before a click. In exchange, Diffo
avoids a cosmetic interaction that can generate remote input and terminal output,
keeps rendering behavior stable across local and SSH sessions, and reduces work
whose product value is negligible.

Network-conscious terminal rendering remains a product constraint. New visual
feedback that causes redraws must provide functional state or progress information;
pointer location alone is not sufficient justification.

## Acceptance

- Moving the pointer onto, within, and away from a button does not change the
  rendered frame.
- Button clicks and their keyboard equivalents continue to work.
- Renderer and input tests do not depend on hover state or passive mouse movement.
- A regression test proves that a passive mouse-movement event does not change
  renderer state or request an application action.
