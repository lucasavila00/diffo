# ADR 0110: Add an AI review activity

Status: Accepted

Refines [ADR 0039](0039-independent-app-modes.md),
[ADR 0043](0043-shared-text-buffer-view.md),
[ADR 0055](0055-command-queue.md), and
[ADR 0107](0107-create-ai-commits-with-codex.md).

## Context

Diff shows every change, but it does not explain how a large change fits together or
where a review should start. A reviewer must find the important hunks, choose an order,
and identify code that needs closer inspection.

Diffo will combine three related capabilities:

- a short overview and an ordered path through important hunks;
- attention markers for areas such as behavior, security, concurrency, error handling,
  public APIs, performance, and test coverage; and
- questions about the change whose answers link back to exact hunks.

These capabilities need their own activity. Putting them in Diff would mix generation,
errors, stale results, and question state into the normal diff workflow.

GitHub Copilot and VS Code place AI summaries, review comments, and questions beside the
diff. Diffo follows that pattern but presents one keyboard-driven review map. The map
helps the reviewer navigate; it does not approve the change or claim that a hunk is
correct or incorrect.

## UX and developer experience

Add **Review** as a third long-lived workbench activity. `Tab` cycles:

```text
Diff -> Explorer -> Review -> Diff
```

The activity rail also opens Review directly. Switching activities preserves each
activity's selection, viewport, and prepared work.

Opening Review does not run Codex. The initial view explains that the staged and
unstaged diff will be sent through the user's Codex CLI and shows a `Generate review`
action. `Enter` starts generation. The request is cancellable and runs outside the input
and rendering loop.

The completed view has two panes:

- The left pane shows an overview of at most three short lines and an ordered map of at
  most eight review stops. Each stop has a title, a staged or unstaged label, one
  attention category, and one sentence explaining why it matters.
- The right pane shows Diffo's diff at the selected hunk. It uses the existing diff
  projection, syntax highlighting, scrolling, and hunk navigation components.

`j` and `k` select a stop. `Enter` opens it. A stop becomes visited when its hunk is
ready and visible, and the map shows `visited / total` progress. `n` and `p` move between
attention-marked stops. Mouse selection performs the same actions and adds no hover-only
behavior.

`/` opens **Ask the diff**. The user enters one question about the captured change. The
answer is at most three short sentences and can contain up to five hunk links. Opening a
link moves the right pane to that hunk. A new question replaces the previous question
and answer. Diffo does not keep or resend a chat transcript. `Esc` returns to the map.

Attention categories are neutral navigation labels:

```text
behavior     correctness     security       concurrency
error-path   public-api      performance    test-coverage
```

The interface does not show severity, confidence, bug counts, approval, or
request-changes language. Review cannot edit files, stage changes, create commits, or
mark a change safe.

Review has five visible states: Empty, Generating, Ready, Stale, and Failed. A failed
request keeps the ordinary Diff and Explorer activities usable and offers a retry.

## Snapshot and navigation

Review analyzes staged and unstaged changes together, while keeping them clearly
labelled. This lets the user understand the whole working tree before deciding what to
stage.

At request creation, capture HEAD and stable staged and unstaged patch projections.
Assign each hunk an opaque ID in projection, path, and hunk order. Staged and unstaged
hunks always have different IDs, including when they belong to the same file. Codex may
only refer to IDs supplied in the request. Paths and line numbers returned by the model
are never used as navigation targets.

Fingerprint HEAD and both projections. Cache a ready review in memory for that exact
fingerprint. Returning to Review reuses it without another request.

A repository change marks the review Stale. Keep the old overview and map visible with
a stale label, but disable hunk links, progress changes, and Ask the diff until the user
regenerates. If the repository changes while a request is running, cancel the child when
possible and discard its result.

Install a result only during frame preparation and only when its request ID and
fingerprint still match. Commit the map, selected target, diff projection, scroll bounds,
syntax coverage, and initial viewport together. Rendering reads committed state only.

## Prompt execution and response parsing

Use the Codex CLI with `gpt-5.6-luna`. Keep the Review model, prompts, schemas, category
list, and limits in `diffo-ai-config`. Reuse ADR 0107's executable lookup, process-wide
path cache, authentication behavior, private schema files, bounded output capture,
read-only sandbox, and terminal-safe errors.

Run review generation as:

```text
codex exec --ephemeral --model gpt-5.6-luna \
  --sandbox read-only \
  --output-schema <private-temporary-review-schema> <fixed-review-prompt>
```

Pass repository content through stdin, not process arguments. Mark it as untrusted data.
The prompt tells Codex not to follow instructions in patches, run tools, infer omitted
code, invent hunk IDs, or make approval claims.

The review response uses a strict JSON schema:

```text
review
  overview: 1..3 strings
  stops: 1..8 items
    title: string
    category: fixed attention category
    reason: string
    primary_hunk_id: supplied hunk ID
    related_hunk_ids: supplied hunk IDs
```

Reject the whole response when the JSON, field bounds, category, or any hunk ID is
invalid. Do not render a partial result.

Use a 256 KiB stdin budget. Include a manifest of changed paths, then distribute the
remaining patch budget fairly across staged and unstaged files in stable order. For an
oversized patch, keep deterministic prefix and suffix samples and add explicit omitted
byte and file counts. A large change loses detail instead of failing only because it
exceeds the budget. The prompt requires the overview to mention relevant omissions and
forbids conclusions about omitted code.

Ask the diff is a separate ephemeral request with a strict response schema:

```text
answer
  text: 1..3 strings
  hunk_ids: 0..5 supplied hunk IDs
```

Its input contains the same bounded snapshot, the review map, the selected hunk ID, and
the new question. It does not contain previous questions or answers. Apply the same ID
validation, output bounds, cancellation, and stale-result checks used for generation.

Use one Codex worker for AI commits, review generation, and Ask the diff. Only one Codex
request runs at a time. Review requests do not occupy the repository mutation service.
A repository mutation may proceed and invalidate the request.

## Implementation boundaries

- `diffo-ai-config` owns the provider, model, prompts, schemas, category list, executable
  name, and request and response limits.
- `diffo-app` owns Review state, input, snapshot fingerprints, hunk-ID validation,
  progress, and composition of shared diff views.
- The `diffo` runtime owns the shared Codex subprocess worker and routes typed results by
  request ID.
- `diffo-core` supplies staged and unstaged projections. AI behavior does not enter the
  real Git data path.
- Review reuses diff and text-view components. It does not own or mutate the live Diff
  activity model.

## Verification

- Workbench tests cover the three-activity `Tab` cycle, activity-rail selection,
  preserved state, and lowercase-only bindings.
- Review state tests cover explicit generation, cancellation, atomic installation,
  visited progress, retry, cache reuse, invalidation, stale navigation, and replacement
  of Ask results.
- Prompt tests cover staged and unstaged hunk IDs, fixed categories, untrusted patch
  content, invalid IDs, deterministic compaction, fair file coverage, and omission
  markers.
- Frame tests cover Empty, Generating, Ready, Stale, Failed, narrow-terminal, hunk-link,
  and Ask views while preserving diff syntax and viewport readiness.
- A frame-traced PTY test generates a review, follows its map, asks a question, and sees
  the review become stale after a repository change.
- End-to-end and stress tests use `codex-mock`. The mock parses and validates every CLI
  argument and returns deterministic Review and Ask responses. Tests never run a real
  Codex binary or use the network.
- `make all` passes.

## Consequences

Review gives the user one path from orientation to close inspection. Overview, attention
markers, and questions share one snapshot and navigate the same hunks. Explicit
generation controls when repository content leaves the process, and fingerprint checks
keep every result tied to the code it describes.

The workbench gains a third activity and a long-lived Review model. The runtime gains two
structured Codex request types. Diff and Explorer keep their current behavior and remain
usable when Codex is missing, unauthenticated, slow, cancelled, or returns invalid data.

## Alternatives

- Add Review controls to Diff. Rejected because generation and question state do not
  belong in the normal diff workflow.
- Split overview, attention markers, and questions into separate activities. Rejected
  because they use the same snapshot and review path.
- Generate whenever Review opens or the repository changes. Rejected because switching
  activities must not start an external request.
- Rank findings by severity. Rejected because the model provides navigation guidance,
  not review authority.
- Keep a chat transcript. Rejected because questions should stay short, current, and
  anchored to hunks.

## Sources

- [GitHub Copilot: Explore pull requests](https://docs.github.com/en/enterprise-cloud%40latest/copilot/tutorials/explore-pull-requests)
- [GitHub Copilot code review in VS Code](https://docs.github.com/en/copilot/how-tos/use-copilot-agents/request-a-code-review/use-code-review?tool=vscode)
- [VS Code source-control quickstart](https://code.visualstudio.com/docs/sourcecontrol/quickstart)
- [OpenAI Codex non-interactive mode](https://developers.openai.com/codex/noninteractive)
