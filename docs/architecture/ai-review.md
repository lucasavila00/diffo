# AI Review

Review is the third Diffo activity. It asks Codex for a short overview and an
ordered path through the staged and unstaged changes. Entering the activity does
not run AI; `Enter` or the visible Generate button starts a review.

## User flow

The initial screen teaches the complete path: generate, choose suggestions with
`n`, `p`, or a click, stage or unstage the selected file with `Space`, and
commit staged work with `i`. Explanatory text is inert. Only rendered buttons
have click targets.

The left pane keeps the summary and review path above the selected suggestion's
details. Every suggestion points to one concrete change in one file. Selecting
it immediately opens that change in Diffo's normal diff renderer; the
explanation may still connect it to related work elsewhere.

Review uses the existing command queue for generation, staging, and committing.
The queue owns scheduling and cancellation and supplies the standard pulsating
border, progress overlay, and cancel target. `Esc` cancels generation. `Enter`
only generates, retries, or regenerates a review.

The review is a description of the repository snapshot used to create it. A
content or HEAD change marks it **Out of date** without replacing the interface.
The old review stays readable and navigable, and current staged work can still
be committed. Staging from old guidance is paused until regeneration. A pure
stage/unstage projection change rebinds the review when its patch is unchanged.

## Request and response

`ReviewRequest` walks staged and unstaged file projections in stable order and
maps contiguous changed regions to opaque target IDs and exact diff rows. It
keeps at most 32 targets per projection, preserving candidates from the start
and end when there are more. The complete XML-shaped context is capped at 256
KiB; fair prefix/suffix patch samples and explicit omission markers replace
oversized content.

Diffo starts one ephemeral, read-only `codex exec` request with `gpt-5.6-luna`,
the fixed prompt, and a temporary output schema. The one request sees the
bounded repository context together, so its overview and order are coherent and
only one CLI process is started. Repository data goes through stdin and is
explicitly untrusted.

The schema allows one to three overview lines and one to eight suggestions. Each
suggestion contains a title, attention category, reason, and target ID. Diffo
independently validates lengths, control characters, categories, known IDs, and
uniqueness before installing the result atomically. Late, malformed, cancelled,
or snapshot-mismatched results cannot alter the active review.

## Availability, failures, and tests

At startup, Diffo resolves Codex first from its inherited `PATH`, then from the
user's login shell, and stores the result for the process lifetime. A missing or
non-executable CLI disables Review and explains the setup action inside the
activity. Authentication, account/model access, rate limits, network failures,
timeouts, crashes, malformed output, and cancellation use the shared bounded
Codex failure handling.

The fixed model, prompt, schema, executable policy, and limits live in
`diffo-ai-config`. End-to-end and stress builds select `codex-mock` at compile
time. The mock validates the exact CLI arguments, schema, prompt, and stdin
shape before returning deterministic JSON, so tests never invoke Codex or the
network.

See [ADR 0111](../adr/0111-ai-review-activity.md).
