# ADR 0022: Large hunk navigation targets

## Problem

Scrollbar change markers are small. They show where changes are, but they are
hard to click.

## Decision

Put one large hunk button at each edge of the diff view:

```text
┌───────────── ↑ Previous change (p) ───────────┐
│                                               │
│                  full file                    │
│                                               │
├────────────── ↓ Next change (n) ──────────────┤
```

The whole top row jumps to the previous hunk. The whole bottom row jumps to the
next hunk.

- Show the top button when a hunk exists above the viewport.
- Show the bottom button when a hunk exists below the viewport.
- Hide a button when there is no hunk in that direction.
- Keep the buttons fixed while the file scrolls.
- Highlight a button on mouse hover.
- Keep keyboard next-change and previous-change actions too.
- Show the keyboard shortcut in each button label. The buttons are wide, so use
  that space to teach `p` and `n` during normal review.

Do not wrap when clicking these buttons. A hidden button makes the start and end
of the file clear.

## Why

The buttons are easy to see and easy to click. They also tell the user which
direction contains another change.

## Cost

The buttons use up to two rows of diff space. They must not cover file content
or the horizontal scrollbar.

## Implementation

Derive both button targets from the committed diff projection and the effective
content viewport. Reserve fixed rows inside the diff border before rendering
file content. Reserve the horizontal scrollbar independently below the bottom
button, so none of the three controls overlap.

Click hit-testing uses only the currently rendered button rectangles and their
non-wrapping targets. Mouse movement updates hover state without changing the
diff scroll position.
