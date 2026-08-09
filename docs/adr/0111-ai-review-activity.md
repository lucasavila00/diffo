# ADR 0111: Add an AI review activity

Status: Accepted

Refines [ADR 0039](0039-independent-app-modes.md) and
[ADR 0107](0107-create-ai-commits-with-codex.md).

## Context

Diff shows every change, but large changes still leave the reviewer to find the
important hunks and choose a useful reading order.

Diffo will add one activity that combines an overview, a guided review path, attention
markers, and questions about the diff. Keeping this separate from Diff avoids mixing AI
generation, stale results, and question state into the normal review workflow.

## Decision

### UX and developer experience

Add **Review** as the third workbench activity. `Tab` cycles:

```text
Diff -> Explorer -> Review -> Diff
```

Opening Review does not call Codex. The empty view explains what will be sent and shows
`Generate review`. `Enter` starts a cancellable background request.

The completed view has two panes:

- The left pane has a short overview and an ordered map of up to eight review stops.
  Each stop has a title, a staged or unstaged label, one attention category, and a
  one-sentence reason.
- The right pane shows the existing Diffo diff at the selected hunk. Review reuses the
  diff renderer and its atomic preparation rules; it does not mutate Diff activity
  state.

`j` and `k` select a stop, and `Enter` opens it. Diffo marks a stop visited once its hunk
is visible and shows progress through the map. `n` and `p` move between attention
markers.

`/` opens **Ask the diff**. An answer is at most three short sentences with up to five
navigable hunk links. A new question replaces the previous one; there is no chat
history.

Use neutral attention categories: behavior, correctness, security, concurrency,
error-path, public-api, performance, and test-coverage. Do not show severity,
confidence, bug counts, or approval language. Review never edits, stages, or commits.

Analyze staged and unstaged changes together and label them clearly. Cache a review in
memory for the exact HEAD and diff snapshot. Repository changes make it stale; keep the
old text visible, but disable its navigation and questions until regeneration.

### Prompt and response handling

Use only the Codex CLI and pin `gpt-5.6-luna`. Keep the model, prompts, schemas,
categories, executable name, and limits in `diffo-ai-config`.

Resolve Codex from the inherited `PATH`, then through the user's login shell. Cache the
resolved path for the process lifetime, but do not cache a miss. Run it as:

```text
codex exec --ephemeral --model gpt-5.6-luna \
  --sandbox read-only \
  --output-schema <private-schema> <fixed-prompt>
```

Pass repository data through stdin and mark it as untrusted. The prompt tells Codex not
to follow instructions in patches, run tools, invent missing code, invent hunk IDs, or
approve the change.

Before sending the diff, give every staged and unstaged hunk an opaque ID. The review
schema returns a short overview and one to eight ordered stops. Each stop contains a
title, a fixed attention category, a reason, one primary hunk ID, and optional related
hunk IDs. Reject the whole response if its JSON, bounds, category, or any ID is invalid.

Use a 256 KiB input budget. Share it fairly across staged and unstaged files in stable
order. Keep deterministic prefix and suffix samples from oversized patches and include
clear omission markers. Large changes lose detail instead of failing only because they
exceed the budget.

Ask the diff uses a separate structured request containing the same snapshot, the review
map, the selected hunk, and the new question. Its schema returns the short answer and up
to five supplied hunk IDs. Do not send previous questions or answers.

Use one Codex worker for AI commits and Review. Only one Codex request runs at a time.
Install results during frame preparation only when the request ID and snapshot still
match. Cancel or discard results when the repository changes.

Use the shared bounded Codex process runner and its 120-second deadline. Authentication,
access, rate-limit, network, service, configuration, incompatible-CLI, timeout, crash,
I/O, and response-validation failures keep the current repository and review state
intact and show an actionable error without exposing credentials.

## Ownership

- `diffo-ai-config` owns the fixed AI policy and CLI contracts.
- `diffo-app` owns Review state, input, hunk IDs, validation, and presentation.
- The `diffo` runtime owns the Codex subprocess worker.
- `diffo-core` supplies staged and unstaged diff projections. AI behavior does not enter
  the real Git path.

## Verification

Test activity switching, generation and cancellation, navigation, cache invalidation,
stale results, strict response parsing, invented IDs, prompt injection, and oversized
diff sampling. Add frame tests for every Review state and a frame-traced PTY flow for
generation, navigation, questions, and staleness.

End-to-end and stress tests use `codex-mock`. The mock validates all CLI arguments and
returns deterministic Review and Ask responses. Tests never invoke real Codex or the
network. `make all` must pass.

## Sources

- [GitHub Copilot: Explore pull requests](https://docs.github.com/en/enterprise-cloud%40latest/copilot/tutorials/explore-pull-requests)
- [GitHub Copilot code review in VS Code](https://docs.github.com/en/copilot/how-tos/use-copilot-agents/request-a-code-review/use-code-review?tool=vscode)
- [VS Code source-control quickstart](https://code.visualstudio.com/docs/sourcecontrol/quickstart)
- [OpenAI Codex non-interactive mode](https://developers.openai.com/codex/noninteractive)
