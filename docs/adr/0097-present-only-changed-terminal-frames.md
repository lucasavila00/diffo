# ADR 0097: Present only changed terminal frames

Refines [ADR 0012](0012-live-repository-refresh.md),
[ADR 0038](0038-remove-button-hover-changes.md),
[ADR 0041](0041-repeatable-cpu-measurement.md), and
[ADR 0047](0047-bound-wheel-momentum-for-ssh.md).

## Problem

Diffo polls for terminal input and background work, then calls
`ratatui::Terminal::draw` after every poll. An idle poll expires every 50 ms, so
an unchanged application still attempts about twenty terminal frames per second.

Ratatui's Crossterm backend writes style-reset and cursor-hide control sequences
even when its cell diff is empty. Those bytes do not change the visible screen,
but terminal multiplexers count them as output activity. Byobu therefore keeps
Diffo's window marked as active. Depending on the multiplexer alert settings,
that activity also becomes a BEL sent to the outer terminal, where terminals
such as Ghostty show a bell or attention indicator.

Repository state did not change and Diffo did not request the user's attention.
Polling, preparation, and terminal presentation are separate operations;
completing the first two must not imply the third.

## Decision

Make terminal presentation conditional on a committed render-visible state
change. The top-level event loop may continue to wake, drain services, advance
application state, and run frame preparation, but it must not call
`Terminal::draw` while the presented state is clean.

Track presentation eligibility explicitly at the state-transition boundaries:

- the initial application frame and every terminal resize require presentation;
- accepted input requires presentation only when it changes committed visible
  state;
- a repository, tool, or update result requires presentation only when accepting
  it changes committed visible state;
- background text or diff preparation requires presentation only when a complete
  replacement, viewport transition, or syntax-ready visible range commits;
- toast expiry, delayed command-progress reveal, and each visible animation step
  require presentation when their rendered value changes; and
- terminal restoration remains unconditional during shutdown.

Drain all ready inputs and results before making the presentation decision, then
coalesce their changes into at most one terminal frame. Keep the existing 16 ms
active and 50 ms idle poll bounds; a poll timeout is a wake-up bound, not a
redraw request.

The following do not make the terminal dirty by themselves:

- elapsed poll time;
- an empty input batch;
- rejected, filtered, or no-op input;
- an unchanged repository snapshot;
- a stale or irrelevant worker result;
- a preparation request that is still pending; or
- a preparation pass whose committed render state is identical to the presented
  state.

Do not rely on Ratatui's buffer diff to suppress this output: invoking the
current Crossterm draw path is itself observable terminal activity even when no
cells differ. Avoid the draw call for a clean iteration.

Do not special-case Byobu, tmux, GNU Screen, Ghostty, `TERM`, or another
terminal. Do not add a user option, environment hook, or configurable redraw
rate. An idle Diffo process is quiet on every terminal.

## Frame tracing

`DIFFO_TRACE_FRAMES` must not cause a terminal presentation. Count and record a
presented frame only when `Terminal::draw` runs.

When a clean iteration contains raw input that must remain observable for
diagnostics, including a wheel event filtered under ADR 0047, represent it as a
suppressed presentation in the trace or an equivalent non-frame record. Do not
manufacture an unchanged terminal frame merely to attach input to a frame
record. CPU measurements must distinguish loop wake-ups from frames actually
presented.

## Alternatives

- Disable activity monitoring in Byobu or disable bell features in Ghostty.
  Rejected because that hides a false signal for one environment and leaves
  Diffo producing needless terminal and SSH traffic.
- Increase the idle poll interval. Rejected because every timeout would still
  create false activity, only less often, and background result latency would
  increase.
- Suppress only the known reset and cursor sequences in a custom backend.
  Rejected because it couples the application to one dependency's current output
  pattern and still performs unchanged rendering work on every idle wake-up.
- Block indefinitely until terminal input arrives. Rejected because repository
  notifications, background preparation, command results, toast deadlines, and
  terminal restoration must remain responsive.

## Consequences

An unchanged idle Diffo writes no bytes to its PTY, consumes less CPU, produces
no SSH redraw traffic, and no longer creates false multiplexer activity or bell
alerts. Real repository changes, completed background work, input transitions,
progress animation, and resizes continue to appear promptly.

Redraw ownership becomes explicit. State-mutating paths must report whether they
changed committed render state, and tests must cover that contract so a missing
dirty signal cannot leave new state undisplayed. Preparation still owns atomic
commits; presentation suppression must never expose a partially prepared buffer
or delay a ready commit behind unrelated input.
