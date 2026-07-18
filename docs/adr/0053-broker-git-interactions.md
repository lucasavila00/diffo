# ADR 0053: Broker supported Git interactions through the workbench

Status: Proposed

Depends on [ADR 0052](0052-terminal-safe-footer-errors.md) and refines
[ADR 0018](0018-network-operation-feedback.md) and
[ADR 0020](0020-operation-toasts.md).

## Context

Network operations currently run Git with captured output, no interactive stdin, and
`GIT_TERMINAL_PROMPT=0`. This keeps the TUI responsive and prevents Git from taking over
Diffo's terminal, but it also turns every interaction into a final failure. On first
contact with an SSH server, for example, OpenSSH may ask whether to trust a host key and
offers affirmative and negative responses. HTTPS credentials and SSH key passphrases
have the same missing route back to the user.

Stderr is not a prompt protocol. It can contain progress, remote-controlled text,
localized wording, hooks, and multiple lines. Rendering it in the footer cannot safely
or reliably identify choices, and writing TUI keystrokes to a Git subprocess would let
the child compete with Diffo for terminal ownership.

VS Code solves the same boundary with `GIT_ASKPASS` and `SSH_ASKPASS` helpers connected
back to the application. Its Git integration presents credential and passphrase input
boxes and a two-option picker for SSH host authenticity instead of exposing the child
process terminal. Diffo will follow that interaction model, not VS Code's configuration
surface. See the upstream
[`askpass.ts`](https://github.com/microsoft/vscode/blob/main/extensions/git/src/askpass.ts)
implementation.

## Scope

Support only prompts delivered through the standard Git and SSH askpass mechanisms:

- HTTPS username and secret input after configured credential helpers do not answer;
- SSH private-key passphrases; and
- SSH host-authenticity confirmation with the host and fingerprint.

Do not implement a generic terminal emulator, parse arbitrary stderr questions, answer
hook prompts, launch an editor, perform browser authentication, configure remotes, or
store credentials. Unsupported prompts fail closed with a safe, actionable operation
failure.

## Decision

### Askpass boundary

Add a private askpass companion executable and a per-operation, local IPC broker. The
companion is implementation infrastructure, not a user-facing Diffo command. Git and SSH
invoke it using their required askpass argument protocol; this does not add arguments,
configuration files, or configurable behavior to the Diffo application.

For Fetch, Pull, and Push, continue to detach the child from terminal input and keep
`GIT_TERMINAL_PROMPT=0`. Set `GIT_ASKPASS`, `SSH_ASKPASS`, and
`SSH_ASKPASS_REQUIRE=force` to the private helper for that child only. Pass a random
operation capability and the broker endpoint in the child environment as internal
protocol data, never as user configuration. Create the endpoint in a permission-restricted
temporary directory and remove it when the operation ends.

The helper sends one request to the broker and writes only the selected response to its
stdout. It never renders, reads Diffo's terminal, logs a response, or mutates repository
state. The broker recognizes the bounded askpass forms and converts them into typed data:

```text
GitInteractionRequest { operation_id, prompt_id, kind }

GitInteractionKind
  Username { host }
  Secret { purpose: Password | SshPassphrase, context }
  ConfirmSshHost { host, fingerprint }

GitInteractionResponse
  Text(value)
  Choice(Continue | Cancel)
  Cancel
```

Parse protocol fields only inside the askpass adapter. Do not expose raw argv, raw stderr,
credential-bearing URLs, or arbitrary response strings to the UI. Remove URL userinfo from
displayed hosts. Apply the terminal-safe text boundary from ADR 0052 to the bounded host,
key, and fingerprint labels before rendering them.

### Worker protocol

Extend the repository-operation boundary with an interaction handler. A real Git source
may synchronously request one of the typed interactions while `apply` is running; fixture
sources continue without requesting one. The refresh service publishes the request with
the operation identifier but does not mark the operation complete.

Prompt responses use a dedicated broker channel, separate from the refresh worker command
queue. This is required because the worker is waiting for the Git child, which is waiting
for askpass, while the response is produced by the main input loop. A response must not be
queued behind the blocked operation. Allow one outstanding prompt per operation and reject
unknown, duplicate, stale, or concurrent prompt identifiers.

The lifecycle is:

```text
repository action starts
  -> Git/SSH invokes private askpass helper
  -> broker emits a typed interaction request
  -> workbench commits and renders the prompt modal
  -> user response returns on the broker channel
  -> helper returns the protocol response and Git resumes
  -> operation result and complete snapshot commit atomically, or failure is reported
```

An interaction request does not advance repository snapshot generations and cannot install
content, navigation targets, or scroll metrics. The final successful result retains the
existing atomic snapshot rules. Closing Diffo, pressing Ctrl+C, or choosing Cancel cancels
the broker request and terminates the child operation. Add a structured cancellation
failure so cancellation is not mislabeled as authentication failure.

### Workbench interaction

Make the prompt a workbench-owned modal so it remains visible over Diff, Explorer, and
Search. Modal input takes priority over activity, palette, toast, and ordinary global
input. While it is open, keep the network operation pending and keep other repository
actions disabled.

Render SSH authenticity as a fixed two-row choice list containing `Continue` and `Cancel`,
with `Cancel` selected initially. Show the sanitized host and fingerprint. Arrow keys,
Enter, Esc, and mouse selection use the established picker behavior. Do not add `y`, `n`,
uppercase, or configurable character shortcuts.

Render username as ordinary text input and passwords or passphrases as masked input. Keep
secret input only in the active modal until it is handed to the broker. Never copy it into
application errors, toasts, frame traces, debug formatting, clipboard output, or repository
state. Do not persist or cache credentials; existing Git credential helpers and SSH agents
remain the persistence mechanisms.

Cancellation closes the modal and leaves the last committed repository snapshot visible.
Acceptance closes only the current prompt; the operation spinner remains until Git and the
post-operation snapshot both complete.

## Alternatives

- Attach Git to Diffo's terminal. Rejected because the child would take over input,
  rendering, mouse modes, and terminal restoration.
- Parse stderr and infer questions. Rejected because the format is localized, mixed with
  progress and remote output, and not a safe request/response protocol.
- Retry after classifying a failed stderr message. Rejected because the first process has
  already failed and retrying can repeat remote side effects.
- Keep all network Git non-interactive. Rejected because recoverable first-contact and
  credential requests become dead ends.
- Accept every new SSH host automatically. Rejected because it removes host identity
  verification and changes OpenSSH trust state without user consent.
- Add credential or host-key configuration to Diffo. Rejected because controls and behavior
  are fixed, while established Git, SSH agent, and `known_hosts` mechanisms already own
  persistence.

## Verification

- Unit tests map each supported askpass form to typed, terminal-safe display data and reject
  malformed, credential-bearing, unknown, and concurrent requests.
- Worker tests prove a prompt response bypasses the blocked action queue, stale prompt IDs
  cannot resume another operation, and cancellation terminates the child.
- Workbench tests cover modal priority, masked secret input, cancel-by-default host choices,
  mouse and keyboard selection, activity switching suppression, and pending-operation state.
- Trace and error tests use sentinel credentials and prove they never appear in frames,
  toasts, errors, debug output, or serialized test artifacts.
- A delayed PTY regression performs Fetch against a local test SSH server with an unknown
  host key, observes the host and fingerprint choices, accepts once, and sees the operation
  complete without terminal corruption.
- A second PTY regression cancels the same prompt and verifies that repository refs, the
  committed snapshot, and the user's `known_hosts` fixture remain unchanged.
- Credential and passphrase tests use local helpers and isolated temporary homes; they never
  depend on a public network service or real user credentials.

## Consequences

Supported Git interactions become explicit application state instead of accidental stderr.
The main loop remains the sole terminal owner, the repository worker may pause without
blocking UI input, and SSH trust still requires an informed user choice.

This adds a private helper, authenticated local IPC, cancellation, and secret-handling paths
that need platform-specific packaging and tests. Until those pieces are implemented, current
network operations remain deliberately non-interactive.
