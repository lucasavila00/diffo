# ADR 0112: Add an AI review activity

Refines [ADR 0034](0034-stage-and-continue-review.md),
[ADR 0039](0039-independent-app-modes.md), and
[ADR 0107](0107-create-ai-commits-with-codex.md).

## Decision

### UX and developer experience

Add **Review** after Diff and Explorer in the `Tab` cycle. Opening Review never
calls Codex. `Enter` or the visible Generate button starts it; explanatory text
is not clickable. While generation runs, the shared command queue provides the
pulsating border, progress, and cancellation. `Esc` cancels; `Enter` does not.

Before generation, explain the whole workflow. A completed review shows a short
summary and up to eight ordered suggestions. Selecting one with `n`, `p`, or a
click immediately opens its change in the normal diff renderer. `Space` stages
or unstages that suggestion's whole file through the shared queue, and `i` uses
the existing guarded AI-commit flow.

A review describes the changes at generation time. If content or HEAD changes,
keep the review and current diff visible, mark it **Out of date**, allow
navigation and committing staged work, and pause staging until regeneration.
Pure staging changes keep the review when the underlying patch is unchanged.

Resolve Codex once at startup from the inherited `PATH` or the user's login
shell. When unavailable, dim Review and let users open it to read the reason and
setup action. Other activities remain available.

### Prompt and response handling

Run one bounded Review request with the shared Codex runner, `gpt-5.6-luna`, a
read-only sandbox, structured output, and a 120-second deadline. One request
lets Codex produce a coherent repository-wide summary and order while avoiding
repeated process startup.

Send staged and unstaged patches through stdin as untrusted data. Give each
navigable changed region an opaque target ID and diff row. Expose at most 32
targets per file projection, keeping candidates from both ends, and bound the
whole context at 256 KiB with explicit omission markers.

Require one to three overview lines and one to eight suggestions. Each
suggestion has a bounded title, reason, fixed attention category, and one target
ID. Reject malformed output, invalid bounds or categories, and unknown or
repeated IDs. Accept results only for the matching queued command and repository
snapshot.

`diffo-ai-config` owns the model, prompt, schema, executable, and limits;
`diffo-app` owns Review state and navigation; `diffo` owns the Codex process.
End-to-end and stress tests use `codex-mock`, which validates the full CLI and
request contract without invoking Codex or the network.
