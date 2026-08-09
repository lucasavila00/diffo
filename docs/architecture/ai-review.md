# AI Review

Review is a normal Diffo activity after Diff and Explorer in the `Tab` cycle. It
uses Codex to summarize staged and unstaged changes and build an ordered path
through the changes worth inspecting. Opening the activity does not contact
Codex. The explicit `[ Generate review (Enter) ]` control starts generation;
surrounding explanatory text is inert.

The initial screen explains that a review covers the changes as they are when
generation starts. If those changes are edited later, Diffo keeps the review and
current diff visible and labels the review **Review out of date**. The user can
still navigate it or commit current staged work. Staging from stale guidance is
paused until `[ Regenerate review (Enter) ]` refreshes the review.

## Review flow

The left pane shows a short summary, a stable review order, and details for the
selected change. Every step focuses on one concrete change in one file. Its
explanation may connect related work, but the selection is never a whole-file or
multi-file group. The right pane uses Diffo's normal diff renderer and centers
the selected change.

`n` and `p` move between changes, matching Diff. `Space` stages or unstages the
entire selected file through the shared command queue, then advances through the
review when staging succeeds. `i` uses the existing guarded AI-commit flow for
staged work. Review does not own separate staging or commit implementations.

## Codex request

Review uses the shared `gpt-5.6-luna` Codex runner in an ephemeral, read-only
sandbox. The request contains staged and unstaged patches with stable opaque
identifiers for each contiguous changed region. Repository content is untrusted
and is written through stdin. The fixed `AI_REVIEW_PROMPT`, schema, model,
executable policy, and limits live in `diffo-ai-config`.

Diffo sends at most two changed file projections per Codex call. Each batch is
bounded at 256 KiB and records omitted content instead of rejecting a large
change. One queued Review command owns all batches and has a 120-second
deadline.

## Response and progress

Each response contains one to three overview lines and one to eight ordered
steps. Every step has a bounded title and reason, a fixed attention category,
and one known change identifier. Diffo rejects malformed output, invalid bounds
or categories, and unknown or repeated identifiers.

Validated batches become available immediately, so the user can start reviewing
while later batches continue. The interface reports the active part, change
range, file paths, and number of ready steps. It does not invent a completion
percentage. The shared command progress border and cancel control remain active
until the complete Review command finishes. `Esc` cancels the active Review
command and its remaining batches. `Enter` only starts, refreshes, or recenters
a review.

## Availability and failure handling

At startup, Diffo resolves Codex from its inherited `PATH` or the user's login
shell and keeps that result for the process lifetime. If Codex is missing, the
Review activity is disabled and explains that installation and a restart are
required.

The shared runner handles cancellation, timeouts, process crashes, bounded
stdout and stderr, authentication and usage failures, malformed responses, and
terminal-safe diagnostics. Results are accepted only for the matching request
and repository snapshot. Content or HEAD changes mark the review out of date; a
pure staging projection change can retain it when the patch still matches. An
out-of-date review remains readable while regeneration runs. The first validated
new batch replaces it; cancellation or failure before that point leaves the old
review visible.

## Offline testing

End-to-end and stress builds select `codex-mock` at compile time. The mock
validates the complete CLI argument, schema, prompt, and stdin contract before
returning deterministic Review JSON. Tests never invoke Codex, credentials, or
an AI service.

The product decision is recorded in
[ADR 0111](../adr/0111-ai-review-activity.md).
