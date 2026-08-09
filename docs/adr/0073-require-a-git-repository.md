# ADR 0073: Require a Git repository at startup

Builds on [ADR 0004](0004-real-git-state.md) and
[ADR 0070](0070-publish-and-self-update-diffo.md).

## Context

Diffo's application view represents the current Git worktree and has no useful
state to show outside one. Repository discovery currently lets a failed
`git rev-parse` reach the top-level error reporter. Launching from an ordinary
directory therefore prints Git's command, fatal diagnostic, and locale-dependent
implementation detail.

The launcher must still distinguish this expected usage error from failures such
as a missing Git executable, an unreadable directory, or a repository Git
refuses to use. The fixed `update` maintenance entry path must remain
independent of repository discovery.

## Decision

Resolve the worktree root before terminal initialization and store that
top-level path as the repository source root. Run subsequent Git commands and
resolve every root-relative status path from that discovered root, regardless of
the directory from which Diffo was launched. Repository discovery classifies
Git's fixed, C-locale "not a git repository" and "must be run in a work tree"
failures as a typed `NotRepository` error. Other discovery failures keep their
diagnostic context.

When the no-argument application entry path receives `NotRepository`, exit
unsuccessfully and print exactly:

```text
Diffo must be run inside a Git repository.
```

Do not initialize the terminal or construct application state. Directories
nested inside a worktree remain valid because Git resolves their enclosing
repository. Argument dispatch and the `update` entry path continue to run before
this check.

## Alternatives

- Show an empty application. Rejected because every application action and view
  is defined in terms of repository state.
- Print every repository-discovery failure as the same usage message. Rejected
  because installation, permission, ownership, and damaged-repository failures
  need their real diagnostics.
- Forward Git's fatal output. Rejected because it exposes command and locale
  details for an expected user mistake.

## Consequences

Launching Diffo outside a worktree produces one stable, actionable line without
entering terminal mode. Bare repositories are rejected because Diffo operates on
a worktree. Unexpected Git and filesystem failures remain distinguishable and
actionable.

## Verification

- A black-box launcher test runs the real Diffo binary in a temporary
  non-repository directory and asserts failure, empty standard output, and the
  exact message.
- A black-box launcher test runs the real binary from a nested worktree
  directory, dumps a repository snapshot, and verifies a root-relative nested
  file is present.
- Existing launcher tests continue to prove invalid arguments and `update`
  dispatch before repository discovery.
- Repository-backed integration tests continue to launch from valid worktrees.
- `make all` passes.
