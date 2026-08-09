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

Review has two panes. The left pane shows a short overview and an ordered list of up to
eight stops. Each stop has a title, a neutral attention category, and one sentence
explaining why to inspect it. The right pane reuses Diffo's diff renderer and opens the
selected stop without changing Diff activity state.

Use `j` and `k` to select a stop, `Enter` to open it, and `Space` to stage or unstage
the reviewed file through the existing command queue. A staging-only change keeps the
review when its patch can be rebound unchanged to the new staged or unstaged projection.
Any content or HEAD change makes the review stale and requires regeneration.

If Codex is unavailable at startup, disable Review and explain that installation and a
Diffo restart are required. Generation runs in the background and `Enter` cancels it.

### Prompt and response handling

Use the shared Codex runner with `gpt-5.6-luna`, a read-only sandbox, structured output,
the fixed 120-second deadline, and the existing failure handling. Resolve Codex from
the inherited `PATH` or login shell once at startup and keep that result for the process
lifetime.

Send staged and unstaged changes through stdin as untrusted data. Give each hunk a stable
opaque ID. The response contains one to three overview lines and one to eight ordered
stops. Each stop contains a title, a fixed attention category, a reason, and one hunk ID.
Reject malformed output, invalid bounds or categories, and unknown or repeated IDs.

Limit input to 256 KiB. Share the budget across files in stable order and mark omitted
content instead of rejecting a large change.

One worker serves AI commits and Review, with one request active at a time. Results are
accepted only for the matching request and repository snapshot. Tests use `codex-mock`;
they never invoke Codex or the network.

## Ownership

- `diffo-ai-config` owns the model, prompt, schema, executable, and limits.
- `diffo-app` owns Review state, navigation, staging intent, hunk IDs, and validation.
- `diffo` owns the Codex process.

## Verification

Test explicit generation, response validation, stale and staging-only repository
changes, oversized input, activity switching, and the mock CLI contract. `make all`
must pass.
