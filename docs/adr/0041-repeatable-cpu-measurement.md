# ADR 0041: Measure whole-process CPU with deterministic PTY workloads

## Problem

Diffo has been observed using substantial CPU during interaction and measurable
CPU while idle. Component benchmarks can measure diff preparation or rendering
in isolation, but they do not show the cost of the real event loop, terminal
drawing, worker threads, or terminal output. Optimizing before those costs are
measured risks changing the wrong subsystem.

CPU measurements are sensitive to build profile, startup work, terminal
dimensions, input timing, fixture contents, and host load. The repository needs
one repeatable developer command whose workloads and reported quantities stay
fixed while the event loop is investigated.

## Decision

Provide `make measure-cpu`, backed by the `diffo-measure` workspace package.

- Support Linux only and read whole-process user and system CPU ticks from
  `/proc/<pid>/stat` using the host's `CLK_TCK` value.
- Build and run Diffo with the release profile. Debug measurements are not part
  of this contract.
- Launch the real Diffo binary in a 100-column by 30-row PTY with the
  deterministic mutable repository fixture.
- Measure two fixed five-second workloads: no input while idle, and one
  mouse-wheel event every 16 ms over the diff pane while active.
- Allow one second for startup and initial preparation before each sample.
- Perform three unreported warm-up runs followed by five reported runs for each
  workload, then report the median of each metric.
- Report wall-normalized CPU percentage, CPU milliseconds, traced frames and
  frame rate, cumulative draw time, and bytes written to the PTY during the
  sample.
- Use the existing developer-only `DIFFO_TRACE_FRAMES` hook to correlate CPU
  with event-loop and draw behavior. Exclude startup and the final quit event
  from the trace window.
- Do not enforce performance thresholds. This is a diagnostic measurement, and
  scheduler noise makes host-independent pass/fail limits unreliable.

The measurement package is developer tooling, not a Diffo user interface. It has
no runtime options; changing a workload is an intentional source change and ADR
update.

## Consequences

The command takes roughly two minutes plus release compilation because stable
samples are preferred over a quick single observation. Results from different
machines are not directly comparable, but idle and active results from the same
machine and before/after results on the same host are useful.

PTY byte counts expose terminal and SSH cost that CPU sampling alone would miss.
Frame count and cumulative draw time help determine whether CPU tracks wakeups,
rendering, input processing, or background work. A sampling profiler such as
`perf` remains a follow-up tool after these measurements identify the workload
that needs deeper attribution.

## Acceptance

- `make measure-cpu` builds and measures the release Diffo binary on Linux.
- Idle samples contain no traced input events; active samples contain scroll
  input.
- Each scenario prints five runs and a median with all documented metrics.
- Diffo exits successfully after every sample and the PTY reader terminates.
- The normal workspace formatting, tests, and lints continue to pass.
