# ADR 0111: Add an AI review activity

Status: Accepted

Refines [ADR 0034](0034-stage-and-continue-review.md),
[ADR 0039](0039-independent-app-modes.md), and
[ADR 0107](0107-create-ai-commits-with-codex.md).

## Context

Diff shows every change, but it does not tell the reviewer where to start or which
hunks deserve attention.

## Decision

### UX and developer experience

Add **Review** after Diff and Explorer in the `Tab` cycle. Opening it does not call
Codex. `Enter` generates a review only when the user asks.

Review teaches the complete flow before generation: `Enter` starts, `j` and `k` move
between review steps, `Space` stages or unstages the current file, and `i` commits staged
changes. User-facing text says change, file, and review step; protocol terms never appear.

The left pane shows a short overview, an ordered list of up to eight steps, the selected
file and staging state, why the step matters, and persistent controls. The right pane
reuses Diffo's diff renderer. Starting a review opens the first step. `j`, `k`, and mouse
selection immediately open their step; `Enter` recenters it after scrolling.

`Space` stages or unstages the whole selected file through the existing command queue.
Successful staging advances one step in review order, including when the next step is in
the same now-staged file. Unstaging and failures stay on the current step. Keep the review
when its patch can be rebound unchanged to the new projection; other content or HEAD
changes require a fresh review.

`i` uses the existing guarded AI-commit command. It commits only staged changes and keeps
the same progress, cancellation, error, and stale-index behavior as Diff. Review does not
own another staging or commit implementation.

If Codex is unavailable at startup, disable Review and explain that installation and a
Diffo restart are required. Generation is a normal command-queue item. After the shared
delay it uses the pulsating application border, progress panel, and cancel control.
`Enter` also cancels it.

### Prompt and response handling

Use the shared Codex runner with `gpt-5.6-luna`, a read-only sandbox, structured output,
the fixed 120-second deadline, and the existing failure handling. Resolve Codex from
the inherited `PATH` or login shell once at startup and keep that result for the process
lifetime.

Send staged and unstaged changes through stdin as untrusted data. Give each hunk a stable
opaque ID. The response contains one to three overview lines and one to eight ordered
stops. Each stop contains a title, a fixed attention category, a reason, and one hunk ID.
Reject malformed output, invalid bounds or categories, and unknown or repeated IDs.

Process at most two changed file projections per Codex call, in stable order, within one
120-second queued command. Install each validated batch immediately so the user can
navigate ready steps while later batches continue. Keep staging and committing disabled
until generation finishes. Show the active part, change range, current files, and ready
step count; do not invent a percentage that Codex does not report. Limit each batch to
256 KiB and mark omitted content instead of rejecting a large change.

One worker serves AI commits and Review, with one request active at a time. Results are
accepted only for the matching request and repository snapshot. Tests use `codex-mock`;
they never invoke Codex or the network.

## Ownership

- `diffo-ai-config` owns the model, prompt, schema, executable, and limits.
- `diffo-app` owns Review state, navigation, staging intent, hunk IDs, and validation.
- `diffo` owns the Codex process.

## Verification

Test explicit generation, response validation, stale and staging-only repository
changes, stage-and-continue selection, oversized input, activity switching, and the mock
CLI contract. The E2E path generates a review, stages with `Space`, and commits with `i`.
`make all` must pass.
