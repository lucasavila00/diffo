# ADR 0047: Bound terminal wheel momentum for SSH

Refines ADR 0014. Syntax-readiness work is specified by ADRs 0045 and 0086.

## Context

Diffo does not implement inertial scrolling or animate a scroll position toward
a target. A trackpad or terminal emulator produces the smooth-scrolling effect
by sending a sequence of mouse-wheel events, including a decaying tail after the
gesture ends. ADR 0014 drains and coalesces events that are already available
together, but events arriving in successive polls still cause successive
viewport changes and terminal updates.

That behavior is responsive on a local terminal but is expensive over SSH. Every
accepted wheel event can change most visible rows, trigger syntax readiness
checks, draw a frame, and send terminal output. A long momentum tail spends
network and remote CPU after the user has already completed the gesture.

Changing the fixed one-line wheel distance does not solve this cost: it changes
how far the viewport moves but still redraws once per accepted event. Diffo
needs a fixed event filter that acts like higher scroll friction.

## Decision

Apply wheel friction to raw terminal input before routing it to Diff or
Explorer:

- accept same-direction events at full speed while consecutive events are no
  more than 48 ms apart;
- reject same-direction events in the slower momentum-tail interval between 48
  and 120 ms;
- begin a fresh burst after 120 ms without a wheel event;
- accept a direction reversal immediately and begin a fresh burst in that
  direction;
- never filter keyboard input, page movement, scrollbar clicks or drags, mouse
  button events, or non-wheel terminal events.

Use fixed constants in code. This is product behavior, not user configuration.
Keep all raw events in `DIFFO_TRACE_FRAMES`, including filtered wheel events, so
the trace explains the input received by the application and the smaller scroll
transition that was applied.

The filter runs before shared workbench routing, so Diff and Explorer have
identical wheel behavior. Accepted events still use ADR 0014's single scroll
owner, ready-event coalescing, and one frame transaction.

## Why these bounds

The 48 ms active interval preserves common 60 Hz and 30 Hz terminal wheel
streams without throttling their distance. As terminal-generated momentum decays
and the gaps grow, the filter stops applying same-direction movement instead of
stretching the tail across more frames. The 120 ms reset is long enough to
classify those sparse events as the old gesture but short enough that a later
discrete wheel action starts immediately.

This is deliberately event decay rather than time-based scroll animation. Diffo
schedules no passive scroll frames and performs no redraw merely because time
passed.

## Consequences

Trackpad momentum decays sooner without slowing the active part of the gesture.
Rapid local scrolling, short gestures, and direction corrections remain
immediate. Sparse same-direction events near the end of terminal-generated
momentum no longer move the viewport or change terminal cells over SSH.

The constants should be changed only with a deterministic input trace and an
SSH-oriented output measurement. Do not add an inertia timer, passive animation
loop, environment setting, CLI flag, or configurable scroll behavior.
