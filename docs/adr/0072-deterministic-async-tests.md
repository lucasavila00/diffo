# ADR 0072: Test asynchronous transitions without timing delays

Builds on [ADR 0024](0024-atomic-diff-buffer-transitions.md),
[ADR 0044](0044-single-frame-position-changes.md), and
[ADR 0045](0045-measure-and-reduce-skeleton-frames.md).

This supersedes the developer delay hook and delayed-PTY-test requirements in
[ADR 0024](0024-atomic-diff-buffer-transitions.md) and
[ADR 0066](0066-direction-neutral-diff-syntax-prefetch.md).

## Problem

An asynchronous test cannot establish ordering by making a worker sleep. A sleep
changes how likely one interleaving is, but it does not create a causal boundary
between the input loop, preparation worker, frame renderer, PTY reader, and test
thread.

This is especially unreliable under the stress job, where all tests share one
CPU. After a test observes a selected file row, the PTY reader may already have
consumed later frames. The background result may therefore commit before the
test separately samples the screen. A fast machine can finish inside an assumed
pending interval; a contended machine can delay the UI until after the worker
wakes. Increasing the sleep only makes the suite slower and moves the race.

`DIFFO_E2E_DIFF_PREP_DELAY_MS` encoded this assumption in the production worker.
It made tests depend on elapsed time, exposed a test-only scheduling control in
runtime code, and still could not prove stale-result or atomic-commit behavior.

Bounded wait deadlines are different. They detect a process that stopped making
progress and keep a failed suite from hanging forever. A deadline must not
determine which state is correct or create the interval in which an assertion is
expected to run.

## Decision

Test asynchronous correctness through observable state transitions and explicit
outcome ordering. Never use a sleep, timer, or delay environment variable to
arrange the ordering being asserted.

- Test worker-result policy below the thread boundary. Supply stale and current
  outcomes directly to the state transition that accepts or rejects them, and
  assert the committed buffer after each outcome.
- In PTY tests, wait only for durable observable conditions needed to send the
  next input. Do not infer that an earlier screen frame is still current after a
  separate wait or screen sample.
- Record requested identity, displayed identity, readiness, and viewport
  transition for each completed frame. After process exit, assert relationships
  within a frame and across the ordered trace.
- Use architectural frame boundaries as synchronization. A request submitted
  during frame preparation cannot be installed by that same preparation pass
  because worker results are drained before the request is sent.
- Permit bounded condition waits only as liveness guards. Test success depends
  on the observed condition or recorded transition, never on how quickly it
  occurred.
- Keep performance limits and latency measurements in repeatable measurement
  tooling; do not turn scheduler-sensitive elapsed time into correctness
  assertions.

The diff preparation worker has no environment-controlled delay. Other developer
hooks must expose data or deterministic control points, not alter scheduling to
make a race more likely.

## Alternatives

- Increase the artificial delay. Rejected because no duration proves ordering on
  both fast and contended systems, and every increase slows the suite.
- Make the test timeout shorter or longer. Rejected because a timeout is a
  liveness ceiling, not synchronization.
- Use a larger fixture to keep the worker busy. Rejected because execution time
  still depends on hardware, caches, compiler settings, and scheduler
  contention.
- Assert transient screen contents immediately after observing selection.
  Rejected because the PTY stream may contain multiple completed frames by then.

## Consequences

Stale and out-of-order result handling is deterministic and fast to test. PTY
tests continue to cover real input, rendering, and frame commits without
requiring a particular worker speed. Stress runs may change how many pending
frames occur, but they cannot change which trace invariants must hold.

Tests may still fail at a bounded wait when the application makes no progress.
Such a failure reports a hang or missing observable state, not a missed timing
window.

## Verification

- No diff worker delay environment variable or worker sleep exists.
- A renderer state-transition test rejects a supplied stale outcome and accepts
  the supplied latest outcome in a fixed order.
- The rapid-open PTY test asserts pending and committed identities from frame
  traces, without sampling transient previous-buffer contents.
- Navigation and cold-open PTY tests pass without injected delays.
- The rapid-open regression passes repeatedly under single-CPU scheduler
  contention.
- `make all` passes.
