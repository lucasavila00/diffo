# ADR 0014: Smooth scrolling

Status: Proposed

## Problem

Scrolling feels like two systems move the view at different times.

Possible causes:

- input is handled after drawing, so every event waits one loop;
- key repeat, mouse wheel, watcher refresh, and renderer completion arrive separately;
- a watcher snapshot can reset scroll while input changes it;
- prepared diff rows can change after scroll was already applied;
- there is no trace showing which event changed the visible row.

## Decision

Use one scroll owner and one frame transaction:

```text
drain input + refresh + renderer results
-> update model
-> clamp scroll against ready content
-> draw once
```

- `diffo-app::Model` is the only owner of scroll position.
- Renderer preparation never writes scroll state.
- Watch refresh preserves scroll when the selected file is the same. Content changes
  clamp the old position; they do not reset it to zero.
- File selection and view-mode changes may reset scroll.
- Drain all ready terminal events before drawing. Coalesce consecutive scroll events
  into one signed delta.
- Apply refresh results before the scroll delta. User input wins in that frame.
- Draw immediately after state changes. Do not wait for the next idle poll.
- Keep the previous prepared diff and its scroll position until the replacement is
  ready. Swap content and clamp position in one frame.

## Debug trace

Add developer-only `DIFFO_TRACE_FRAMES=<path>`.

Write one RON record per frame:

```text
frame, input events, refresh generation, selected file,
content revision, preparation state, scroll before, scroll after, first rendered row
```

Record monotonic timestamps for event read, update start, draw start, and draw end.
Do no file I/O on the UI thread. Send trace records to a bounded writer thread. Drop
records when full.

The trace must answer:

- Did one physical event become two messages?
- Did a refresh overwrite scroll?
- Did content swap after the scroll frame?
- How long was input-to-draw latency?

## Regression tests

- Feed one wheel event. Assert one scroll transition and one next-frame movement.
- Feed ten ready wheel events. Assert one coalesced transition and one draw.
- Interleave refresh and scroll. Assert the user scroll wins.
- Complete background preparation while scrolling. Assert no intermediate old/new
  frame and no jump to zero.
- Run the same cases with key repeat, Page Up, and scrollbar drag.
- Add a PTY test that sends timestamped wheel events and reads frame trace output.
  Require one visible transition per batch and bounded input-to-draw latency.

## Done when

- A trace contains no unexplained scroll writes.
- Watcher activity cannot reset an active scroll.
- Ready input is visible in the next draw.
- Small and large diffs scroll with the same event semantics.
