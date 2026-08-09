# ADR 0063: Exercise askpass through real loopback Git and OpenSSH

Refines [ADR 0053](0053-broker-git-interactions.md) and
[ADR 0062](0062-use-procfs-image-for-askpass.md).

## Context

The original askpass end-to-end tests used an executable shell script as
`GIT_SSH`. The script invoked the askpass programs with hand-written prompts and
then executed the Git server command locally. This covered the UI and socket
protocol, but not the behavior of an SSH client or server.

The scripted host-confirmation test also set `SSH_ASKPASS_PROMPT=confirm`
itself. Real OpenSSH on the supported Linux environment can omit that optional
hint while passing the complete host-confirmation prompt. Diffo rejected the
real prompt even though its shape, host, algorithm, and fingerprint were all
valid.

## Decision

Run network-operation end-to-end tests against a real unprivileged OpenSSH
server bound to a random loopback port. Each test creates temporary bare, seed,
and worktree Git repositories, generates a host key and client key with
`ssh-keygen`, uses an isolated client configuration and `known_hosts`, and lets
OpenSSH execute the real `git-upload-pack` or `git-receive-pack`. Tests use no
public network and no replacement Git or SSH executable.

Use an encrypted client key to exercise the passphrase path. Use a password-only
SSH configuration to make OpenSSH issue an unsupported password prompt and
verify that Diffo fails closed. Continue fault injection by unlinking and
replacing the launched Diffo pathname, but verify only externally visible
behavior: the original running image completes the operation and the replacement
does not run.

Treat `SSH_ASKPASS_PROMPT` as a hint. When it is absent and the prompt contains
multiple lines, accept only the same complete, strictly validated OpenSSH
host-confirmation form accepted when the hint is `confirm`. Reject malformed,
control-bearing, or unrelated multiline prompts. Keep unknown single-line
password and keyboard-interactive prompts unsupported.

Black-box tests assert terminal content, trace secrecy, `known_hosts`, Git refs,
worktree state, and process success or failure. They do not inspect private
temporary directory names, socket names, executable-copy strategies, or other
implementation details.

Do not load serialized repository fixtures or substitute Git/SSH commands in
this black-box suite. Construct rename and failure states with real Git commands
and real missing or rejected remotes. Keep deterministic delay and frame-trace
hooks only for contracts whose subject is cancellation ordering, atomic
presentation, timing, or secret non-leakage; these hooks control observation and
scheduling but do not replace Git, SSH, the repository, or the compiled
application.

Make `make e2e` run the `diffo-e2e` snapshot package and the compiled black-box
`git_operations` test. Make `make all` run the complete workspace test suite
once; `cargo test --workspace` already includes both black-box suites, so it
must not invoke the focused target again. Install `openssh-server` in the
devcontainer and CI.

## Consequences

The E2E suite now detects differences in real OpenSSH prompts, environment
behavior, host-key persistence, encrypted-key handling, remote command
execution, and child process lifetime. It requires an OpenSSH server binary but
no privileged daemon, fixed port, system host key, public service, or persistent
credential.

The parser remains fail-closed without depending on OpenSSH always setting its
optional prompt-kind environment variable.

## Verification

- Accepting a real first-contact host prompt completes a Fetch and writes the
  generated host key to the isolated `known_hosts` file.
- Cancelling host approval or an encrypted-key passphrase preserves refs and
  worktree state.
- A correct real key passphrase completes Fetch without appearing in terminal
  output or frame traces.
- A real SSH password prompt opens no modal, fails the operation, and changes no
  Git state.
- Replacing the launched Diffo pathname before a prompted Push cannot redirect
  askpass; the Push reaches the real bare remote through OpenSSH.
- `make e2e` and `make all` pass.
