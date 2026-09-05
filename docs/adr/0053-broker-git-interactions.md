# ADR 0053: Show Git prompts inside Diffo

Depends on [ADR 0052](0052-terminal-safe-footer-errors.md) and refines
[ADR 0018](0018-network-operation-feedback.md) and
[ADR 0020](0020-operation-toasts.md).

## Problem

Fetch and Sync's remote Git phases run without terminal input. This keeps Git
from taking over Diffo's terminal. It also means these normal questions fail:

- HTTPS username or secret;
- SSH key passphrase; and
- whether to trust a new SSH host key.

Do not give Git the terminal. Do not guess questions from stderr. Stderr also
contains progress, hooks, remote text, and errors.

Git and OpenSSH already provide the right hook: askpass. They start a program,
give it a prompt, and read its answer from stdout. Diffo can serve as that
program. Askpass still starts a short-lived process, but we do not need to ship
another binary.

## Decision

### Re-enter the Diffo binary

Ship one binary. When Git or SSH invokes that binary through askpass, an
internal environment marker makes startup enter a small askpass path instead of
the TUI. This is not a command or user setting. Normal Diffo startup still
accepts no arguments.

For each Fetch or remote Sync phase:

- keep stdin closed and `GIT_TERMINAL_PROMPT=0`;
- point `GIT_ASKPASS` and `SSH_ASKPASS` at the running image's procfs path from
  ADR 0062;
- set `SSH_ASKPASS_REQUIRE=force`; and
- pass the internal askpass marker and a fresh Unix-socket path in the child
  environment.

The socket server runs inside Diffo. There is no broker daemon and no second
long-lived process.

```text
Git or SSH -> Diffo askpass mode -> Unix socket -> running Diffo UI
             Diffo askpass mode <- Unix socket <- user's answer
Git or SSH <- askpass stdout
```

Create the socket in a fresh mode-0700 temporary directory. Remove it when the
operation ends. The directory separates operations and blocks other users.
Processes running as the same OS user are trusted; a token in the environment
would not protect against them.

The askpass path handles one prompt, writes at most one answer, and exits. It
never reads the terminal, renders UI, logs data, or changes the repository.

On success:

- text answers go to stdout with exit status 0; and
- SSH host approval writes `yes` with exit status 0.

On cancel, bad input, lost IPC, or an unsupported prompt, write no answer and
exit nonzero.

### Parse only known askpass prompts

Askpass identifies a prompt, but its argument is still plain text. The adapter
may parse that argument. It must never parse stderr.

Use exact prompt forms and `SSH_ASKPASS_PROMPT` when it is present. Accept only
tested forms for:

```text
Username { host }
Secret { kind: HttpsSecret | SshKeyPassphrase, context }
ConfirmSshHost { host, fingerprint }
```

OpenSSH does not provide a reliable machine-readable tag for its first-contact
host-key question. Match the complete supported prompt shape and validate the
host and fingerprint fields. Do not classify a prompt from one word such as
`yes`, `no`, or `authenticity`. If the shape changes or a field is missing,
cancel the prompt.

Remove URL userinfo from hosts. Convert displayed host, context, and fingerprint
text with the terminal-safe boundary from ADR 0052. Raw prompt text must not
reach the UI, errors, traces, or logs.

Do not support arbitrary SSH password or keyboard-interactive questions. Do not
turn an unknown prompt into a generic secret box. Fail closed with a safe
operation error.

### Let the worker wait without blocking the UI

Git waits for the helper. The helper waits for the user. The repository worker
therefore waits too.

The in-process socket bridge sends a typed prompt event to the main loop. The
main loop sends the answer back on a dedicated one-shot channel. Do not put the
answer on the repository service request lane; that lane cannot run until Git
returns. Prompt events and answers carry the active application command ID as
well as the prompt ID.

Allow one open prompt per operation. Give each prompt an ID. Reject unknown,
duplicate, stale, concurrent, or command-mismatched IDs. More prompts may follow
one at a time if Git retries.

A prompt does not change the repository generation or committed snapshot. On
success, install the operation result and new snapshot using the existing atomic
commit rules. On failure or cancel, keep the last committed snapshot.

### Own the prompt in the workbench

Show the prompt as a modal over every activity. While it is open:

- modal input wins over all other input;
- the network operation stays pending; and
- later repository intents may queue, but no second command starts until the
  prompted command finishes.

Use normal text input for a username. Mask secrets. Keep a secret only in the
modal and the response being sent to the helper. Never clone, debug-print,
trace, persist, or cache it. Git's configured credential helpers and SSH agent
remain responsible for storage.

For a new SSH host, show the sanitized host and fingerprint. Show `Continue` and
`Cancel`, with `Cancel` selected first. Use the existing picker controls:
arrows, Enter, Esc, and mouse. Add no character shortcuts.

Accepting closes only that prompt. The operation remains pending until Git
finishes and the new snapshot is ready.

### Cancel the whole operation

Start Git in its own process group. Cancel, Ctrl+C, and shutdown must:

1. close the modal and socket;
2. terminate and reap the operation process group; and
3. report cancellation as cancellation, not authentication failure.

This prevents Git, SSH, or an askpass process from surviving after Diffo exits.
The workbench queue, repository operation context, prompt broker, socket bridge,
and process-group runner share one cancellation handle for the active command.

## Out of scope

Do not add a terminal emulator, editor, browser login, credential store, remote
setup, or generic prompt system. Do not answer hook prompts. Do not accept
unknown SSH hosts automatically.

## Cost

This adds a small private startup path, one short-lived in-process socket bridge
per network operation, modal state, and cancellation tests. It adds no binary,
daemon, terminal emulator, credential store, or user configuration.
