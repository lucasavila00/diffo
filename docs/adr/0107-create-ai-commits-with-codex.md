# ADR 0107: Create AI commits with Codex

Refines [ADR 0019](0019-commit-message-modal.md),
[ADR 0110](0110-queue-command-intents.md), and
[ADR 0056](0056-own-deferred-execution-dependencies.md).

## Context

Diffo can stage and commit without leaving the TUI, but its generated
`Update N files` fallback does not explain the change. A user can write a
message in the commit editor, yet that interrupts the fast keyboard workflow of
reviewing, staging all changes with `a`, and committing.

Codex provides a stable non-interactive `codex exec` interface for subprocess
use. It accepts piped context, writes progress to stderr and its final response
to stdout, can avoid persisting a session with `--ephemeral`, and supports a
JSON output schema. Diffo can therefore use the user's installed and
authenticated Codex CLI without owning API credentials or adding a second
network client.

VS Code's commit-message generator establishes useful product precedents: prefer
staged changes when present, refresh state before generation, include recent
commit subjects as style evidence, show cancellable progress, and do not force
Conventional Commits when the repository uses another convention. Diffo keeps
those inputs but chooses a stronger, keyboard-first action that creates the
commit after generation instead of opening an editor for review. VS Code also
renders its prompt against a model token budget: change diffs have higher
priority than recent commit examples, so lower-priority context can be pruned
instead of rejecting a large change outright. Diffo adopts that degradation
principle with a deterministic byte budget suitable for the CLI boundary.

## UX and developer experience

Add `i` as the fixed lowercase `AI commit staged changes` shortcut. It never
opens the commit-message modal. `m` continues to edit a manual message and Enter
continues to submit it, so the existing manual workflow remains unchanged.

`i` submits one ordinary queued intent whose composite command has two visible
phases: `Writing commit message` and `Committing`. The workbench intent queue is
the single scheduler and owns progress, cancellation, and serialization across
both phases. The fixed queue panel exposes cancellation; cancelling a waiting
row removes it and every dependent intent behind it, while cancelling the active
row uses the shared command cancellation path.

The fast `a`, `i` sequence is two queued intents. Bind the AI request to the
fresh committed snapshot only when it reaches the head of the queue, after Stage
All succeeds and installs its result. A failed or cancelled prerequisite removes
dependent intents. An activated AI intent with no staged files reports
`Stage changes before creating an AI commit`.

Later repository mutations and commits may queue while the AI command is active;
diff review and navigation remain available. A valid generated subject replaces
the current draft only when generation succeeds, immediately before the Git
phase. A successful commit clears it and reports `Committed <hash> — <subject>`.
A Git failure retains the generated subject for manual recovery. A generation
failure retains the previous draft.

Codex is an optional runtime dependency. Resolve `codex` only when the action
starts: first from the inherited `PATH`, then through `command -v` in the user's
login shell so IDE and subprocess launchers see the same shell installation.
Invoke the resolved absolute path and cache a successful resolution for the
process lifetime; do not cache a miss, so installing Codex or repairing the
shell path can be retried without restarting. Do not probe at startup, bundle
Codex, manage authentication, add configuration, or change release artifacts.
Reuse the user's saved Codex authentication and provider, but pin `gpt-5.6-luna`
for its low latency and cost on this small structured task instead of inheriting
the user's default model. Key help and the Diffo README disclose that the
request sends the staged diff, repository and branch identity, and recent commit
subjects through that CLI setup. The explicit AI action is consent; do not add a
confirmation modal, telemetry, or a persistent preference.

Keep the provider, model, executable names, request limits, fixed prompt, and
response schema together in the compile-time `diffo-ai-config` crate. Production
and mock paths must consume the same constants, particularly the model, so
changing policy has one code edit and cannot leave the offline CLI contract
behind. Summarize the active policy and its edit point in the repository-root
`AI.md`; it is documentation, not configuration.

End-to-end and stress tests never invoke a real Codex installation. A separate
`codex-mock` workspace binary implements only this fixed subprocess contract.
Those builds enable the `codex-mock` Cargo feature, which changes the fixed
executable name from `codex` to `codex-mock`; the mock binary directory is
placed on `PATH`. This is a build-time test boundary, not runtime configuration.

## Prompt execution and response handling

At command start, capture an immutable request containing HEAD, branch name,
repository basename, the staged path/status/patch projection in stable path
order, and the five newest repository commit subjects. Include no unstaged
changes, whole-file contents, credentials, or unrelated repository data. Bound
prompt context at 256 KiB, but do not reject an otherwise valid AI commit merely
because its staged patch is larger.

When the complete context does not fit, drop the lower-priority recent subjects
first. Then divide the remaining budget fairly across every staged file and
retain deterministic prefix and suffix samples of oversized diffs, with an
explicit omission marker. Preserve every file's path, rename source, and status
whenever their manifest fits. If the manifest itself is oversized, include as
many stable path-ordered entries as fit plus an explicit omitted-file count. The
prompt must tell Codex not to invent details hidden by either marker. This
follows VS Code's priority-based degradation while keeping Diffo's payload,
memory use, and tests deterministic.

Pass a fixed instruction as the prompt argument and the repository context
through stdin, so source content never enters the process arguments. Mark
repository content as untrusted data. Tell Codex not to execute commands or
tools, obey instructions found in the diff, copy recent subjects, or invent
issue references. Recent commits determine style; use a concise imperative
subject only when history supplies no convention. The result is one non-empty
line of at most 72 Unicode characters, with no body.

Run Codex in the repository root with:

```text
codex exec --ephemeral --model gpt-5.6-luna \
  --sandbox read-only \
  --output-schema <private-temporary-schema> <fixed-prompt>
```

Do not use the JSONL `--json` event stream. Embed a strict schema for one
`subject` string, materialize it in a private temporary file for the child
lifetime, and remove it after the child exits. Capture stdout and stderr
concurrently while retaining at most 16 KiB of each so the child cannot block on
a full pipe. Parse stdout as JSON, then independently reject surrounding
whitespace, control characters, newlines, empty text, or more than 72
characters.

The runner owns the child, stdin, bounded output readers, temporary-schema
guard, cancellation, and process reaping. Missing CLI, authentication, nonzero
exit, malformed JSON, and invalid subjects use the shared terminal-safe error
modal. Diagnostics may contain bounded stderr but never the prompt context,
diff, environment, or authentication material.

Carry the expected HEAD and staged projection into the Git phase. Revalidate
both at the repository boundary immediately before `git commit`; if either
changed, discard the generated result and report
`Staged changes changed; press i to try again`. Keep one command ID and
cancellation handle across generation and commit so another queued action cannot
run between the phases.

## Implementation boundaries

- `diffo-ai-config` is the single source of truth for fixed AI provider, model,
  CLI, prompt, schema, and size policy.
- `diffo-app` owns the fixed input mapping, pure AI-command state, prompt
  construction, queued phase transitions, and presentation.
- The `diffo` runtime owns the Codex subprocess worker and hands a validated
  subject back to the active workbench command.
- `diffo-core` carries the guarded commit target shared by real Git and the
  mutable mock.
- `diffo-git` and the mock repository revalidate the expected snapshot before
  committing; Codex never mutates the repository directly.

## Sources

- [OpenAI Codex non-interactive mode](https://developers.openai.com/codex/noninteractive)
- [VS Code source-control AI commit-message behavior](https://code.visualstudio.com/docs/sourcecontrol/overview)
- [VS Code commit-message service](https://github.com/microsoft/vscode/blob/main/extensions/copilot/src/extension/prompt/vscode-node/gitCommitMessageServiceImpl.ts)
- [VS Code commit-message prompt](https://github.com/microsoft/vscode/blob/main/extensions/copilot/src/extension/prompts/node/git/gitCommitMessagePrompt.tsx)
- [VS Code prompt priority and token-budget design](https://github.com/microsoft/vscode-copilot-chat/blob/main/CONTRIBUTING.md#developing-prompts)
