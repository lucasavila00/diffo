# ADR 0022: Large hunk navigation targets

Status: Proposed

## Problem

Scrollbar change markers are small. They show where changes are, but they are hard
to click.

## Decision

Put one large hunk button at each edge of the diff view:

```text
┌────────────── ↑ Previous change ──────────────┐
│                                               │
│                  full file                    │
│                                               │
├─────────────── ↓ Next change ─────────────────┤
```

The whole top row jumps to the previous hunk. The whole bottom row jumps to the next
hunk.

- Show the top button when a hunk exists above the viewport.
- Show the bottom button when a hunk exists below the viewport.
- Hide a button when there is no hunk in that direction.
- Keep the buttons fixed while the file scrolls.
- Highlight a button on mouse hover.
- Keep keyboard next-change and previous-change actions too.

Do not wrap when clicking these buttons. A hidden button makes the start and end of
the file clear.

## Why

The buttons are easy to see and easy to click. They also tell the user which
direction contains another change.

## Cost

The buttons use up to two rows of diff space. They must not cover file content or
the horizontal scrollbar.
