# AI in Diffo

Diffo uses AI for commit-message generation and guided diff review. Pressing `i`
generates a commit subject from the staged changes and, if the repository still matches,
creates the commit. The Review activity builds an overview and an ordered set of review
steps linked to the relevant changes. Review uses the same `Space` staging and `i` AI
commit commands as Diff. The manual `m` commit-message workflow does not use AI.

## Supported provider and model

Diffo supports **OpenAI Codex only**. It launches the locally installed `codex` CLI and
reuses that CLI's existing authentication. Diffo does not call an AI HTTP API directly,
store API keys, select another provider, or fall back to a different model.

The current model for every AI task is **`gpt-5.6-luna`**. It is pinned because Diffo's
AI interactions are focused, latency-sensitive tasks and should use a fast, inexpensive
model instead of whichever model happens to be the user's Codex default.

All compile-time AI policy lives in the
[`diffo-ai-config`](crates/diffo-ai-config/README.md) crate. To change the model, edit
only `AI_MODEL` in
[`crates/diffo-ai-config/src/lib.rs`](crates/diffo-ai-config/src/lib.rs). Production,
unit tests, and `codex-mock` all consume that constant, so the mock CLI contract cannot
silently keep expecting an old model. Update this document and ADR 0107 at the same time
so the user-facing documentation names the selected model accurately.

The same crate owns the provider name, real and mock executable names, sandbox policy,
context and output limits, fixed prompt, and JSON response schema. These are fixed
product/build policies, not user configuration. Diffo intentionally has no AI settings
file, CLI option, key-binding option, or model environment variable.

## Codex invocation

Production runs this fixed non-interactive command in the repository root:

```text
codex exec --ephemeral --model gpt-5.6-luna \
  --sandbox read-only \
  --output-schema <temporary-schema> <fixed-prompt>
```

The repository context is written to stdin, not placed in process arguments. Codex's
final response is read from stdout and progress or errors are read from stderr. The
session is ephemeral and the sandbox is read-only; Codex generates text and never
changes the repository. Diffo itself performs the guarded Git commit.

Codex must already be installed and authenticated. At startup, Diffo first resolves
`codex` from the `PATH` it inherited. If it is absent there, Diffo asks the user's login
shell from `SHELL` to resolve `codex`, then stores the result for the process lifetime.
This covers terminals and IDE launchers whose inherited environment has not incorporated
the user's shell setup. When Codex is missing, the Review activity is disabled and
explains that Diffo must be restarted after installation. Manual Git workflows remain
available.

## Data sent to Codex

An AI commit request contains only:

- the repository basename and current branch;
- the staged file paths, statuses, and staged patches;
- the five newest commit subjects as low-priority style examples.

It does not include unstaged patches, arbitrary whole-file contents, environment
variables, credentials, or unrelated repository data. Repository content is marked as
untrusted, and the fixed prompt tells Codex not to follow instructions found inside a
diff or invent details hidden by context omission.

Context is bounded at 256 KiB. Oversized changes are not rejected: Diffo first removes
the recent-subject examples, then fairly samples the beginning and end of every staged
patch while preserving file metadata and explicit omission markers. If even the file
manifest does not fit, it includes the path-ordered entries that fit and an omitted-file
count.

## Response and commit safety

Codex must return a strict JSON object containing one `subject`. Diffo independently
parses and validates that it is non-empty, has no padding, control characters, or line
breaks, and is at most 72 Unicode characters. Output from either process stream is
bounded before it is retained.

Every request has a fixed 120-second deadline. Diffo writes stdin and drains stdout and
stderr concurrently, owns the Codex process group, and reaps it on success,
cancellation, timeout, I/O failure, or crash. An early Codex failure wins over a broken
stdin pipe, so an expired login is reported as an authentication problem instead of a
generic write error.

Nonzero exits are classified conservatively from bounded stderr. Authentication,
account/model access, usage limits, network failures, service failures, incompatible CLI
arguments, and Codex configuration failures receive actionable messages. Signal exits
are reported as crashes. Unknown failures retain only a short terminal-safe final line;
diagnostics containing API keys, tokens, authorization headers, or similar credential
markers are never echoed. Since Codex does not document stable stderr wording, this
classification improves the message but never controls correctness or recovery.

The staged projection and HEAD captured for generation are checked again immediately
before `git commit`. If either changed, Diffo discards the generated action instead of
committing a message for stale content.

## Offline testing

End-to-end and stress builds enable the `codex-mock` Cargo feature. That compile-time
switch changes the executable from `codex` to `codex-mock`; there is no runtime override.
The mock parses and validates the complete CLI argument contract, schema, and stdin
shape before returning deterministic commit or Review JSON. Tests therefore never
invoke the real Codex CLI, use credentials, or access an AI service. Runtime failure
tests use local fake subprocesses rather than a real Codex installation.

The detailed product and implementation decision is recorded in
[ADR 0107](docs/adr/0107-create-ai-commits-with-codex.md). The living prompt structure,
context contract, and maintenance workflow are documented in
[AI commit prompt architecture](docs/arch/ai-commit-prompt.md).
