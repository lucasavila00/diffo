# ADR 0105: Show Git failure diagnostics

Refines [ADR 0052](0052-terminal-safe-footer-errors.md) and
[ADR 0084](0084-acknowledge-errors-in-one-modal.md).

## Context

When a Git subprocess exits unsuccessfully, Diffo classifies the combined output
and usually keeps only a short fixed detail. An unrecognized error becomes
`Git operation failed`, even when Git wrote the cause and recovery information
to stderr. The shared acknowledgement modal cannot show information that was
discarded at the Git boundary, so the user must leave Diffo and reproduce the
failure.

Git supplies more than one useful failure field. The process has an exit status
and may write different information to stderr and stdout. Keeping the streams
distinct also makes their origin clear. However, authentication failures and
remote hooks may echo credentials, authenticated URLs, tokens, or arbitrary
server text.

## Decision

For every unsuccessful Git subprocess owned by a repository action, retain this
user-visible detail:

- the existing classified summary;
- the Git process exit status;
- non-empty stderr, labeled `stderr`; and
- non-empty stdout, labeled `stdout`.

Trim surrounding whitespace from each stream and omit a stream when it is
identical to the classified summary. Preserve stderr and stdout separately
through failure classification instead of concatenating them before
presentation. Apply this policy to single commands, Sync, merge, and the
everyday operation paths.

Authentication and remote-hook classifications keep their fixed, secret-safe
summary and exit status but discard both captured streams. Do not add command
arguments, environment values, prompt answers, or commit messages to the
diagnostic. Those values can contain secrets and are not required to explain
Git's own output.

Bound the complete stored detail to 16 KiB. Preserve valid UTF-8 boundaries and
end a shortened detail with `[Git diagnostic truncated]`. This is a fixed
product limit, not configuration.

Keep the existing shared acknowledgement modal. The rendering boundary continues
to pass the complete detail through `terminal_safe_text`, so newlines and
terminal controls are visible and inert. Do not add a Git-specific error dialog
or toast.

## Consequences

Most Git failures expose the cause, process result, and both available output
streams without requiring the user to rerun a command. Classified summaries
remain available for quick interpretation.

Sensitive failure classes deliberately show less metadata. Their raw output is
less valuable than preserving the existing guarantee that secrets do not enter
application state, debug output, modal text, or frame traces.

Long diagnostics are explicitly incomplete but cannot create unbounded
application state. On small terminals, the modal shows as much of the bounded
terminal-safe text as its fixed layout permits.
