# ADR 0076: Measure startup through PTY readiness milestones

## Context

Diffo previously collected the complete repository snapshot and constructed the
workbench before initializing the terminal. A user therefore saw an unchanged or
blank terminal for the whole startup interval. Wall-clock timing around
`make diffo-mock` mixes Cargo's debug-launch overhead with application work,
while a timer that stops at process exit cannot distinguish startup from the
later interactive session.

Startup also scales along two different axes. The mock repository contains very
large generated diff bodies, while a real repository can contain many
independently changed files that require Git and filesystem work. One workload
cannot characterize both.

## Decision

Provide `make measure-startup`, backed by the Linux-only `diffo-measure`
package.

- Build the Diffo binary with the release profile before measurement. Exclude
  Cargo compilation and the measurement harness's repository setup from samples.
- Launch the real binary in a 100-column by 30-row pseudo-terminal.
- Perform three unreported warm-up launches and five reported launches for each
  fixed scenario, then report medians.
- Observe output from the PTY instead of adding sleeps or production timing
  output. Quit each process as soon as every required milestone has appeared.
- Apply a 30-second diagnostic timeout so a missing marker fails with its
  scenario name instead of hanging.

Report these elapsed times from immediately before process launch:

1. `first_output_ms`: the PTY reader receives its first byte;
2. `ready_ms`: a known repository path is visible in the committed application
   frame.

The ready marker measures a usable repository frame, not completion of every
deferred Explorer or diff-preparation job. The selected files in these workloads
are small, so their first view does not intentionally exercise the large-diff
background boundary. Text and syntax readiness remain the separate concern of
ADR 0045 and `make measure-text-readiness`.

Use these fixed scenarios:

- `mock-5.6m-lines`: the checked-in RON fixture plus its generated 5,000-,
  50,000-, 500,000-, and 5,000,000-line source patches, the existing large Rust,
  JSON, and long-line payloads, and 250 generated file-list entries;
- `git-1-change`: one small tracked file changed in a temporary real Git
  repository; and
- `git-500-changes`: 500 small tracked files changed in a temporary real Git
  repository.

The mock scenario isolates large in-memory content and cloning. Comparing the
two Git scenarios exposes changed-file scaling, including Git process and
filesystem costs. All repository setup happens once before warm-ups and is
outside the reported interval.

Do not enforce startup thresholds in CI. Scheduler state, filesystem cache
state, Git version, storage, and host load make portable pass/fail timing limits
unreliable. Compare medians before and after a change on the same host and
inspect every raw sample for instability.

## Consequences

The measurement is deliberately black-box: it includes repository discovery, Git
subprocesses, allocation, model construction, and terminal drawing performed by
the shipped binary. Diffo intentionally performs one initial application draw,
so `first_output_ms` and `ready_ms` should remain close. A large gap is evidence
of extra terminal commits or partial startup presentation and should be
investigated.

The 500-file workload starts many real Git commands and is diagnostic developer
work, not part of `make all`. Changing its sizes, milestone definitions,
terminal geometry, warm-up count, or sample count requires updating this ADR
with the harness.

## Verification

- `make measure-startup` prints five samples and a median for all three
  scenarios.
- Every sample observes first output and the repository marker.
- Every launched Diffo process exits successfully and its PTY reader terminates.
- `make all` continues to validate the workspace independently of timing noise.
