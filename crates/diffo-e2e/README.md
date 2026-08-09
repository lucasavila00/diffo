# diffo-e2e

`diffo-e2e` is the end-to-end test support crate for Diffo.

It drives the compiled terminal application through a pseudo-terminal and
exposes screen selectors and input helpers to the real-Git tests.
Network-operation tests use a loopback OpenSSH server with generated keys and
temporary bare repositories; they never use a public network. The test
environment requires `git`, `ssh`, `sshd`, and `ssh-keygen`. This package is not
published.

The E2E boundary is black-box wherever the behavior permits it. Tests launch the
compiled Diffo executable, drive its terminal UI, use real temporary files and
Git repositories, and assert rendered output, exit status, filesystem content,
Git refs, and remote state. They do not inject repository snapshots or assert
private socket, temporary-directory, or helper-image details.

Tests that need deterministic command ordering may put a one-shot proxy first on
the test process's `PATH`. The proxy blocks one selected Git subcommand on an
explicit FIFO gate and delegates every invocation to the real Git executable;
production code contains no timing hooks. Developer trace output remains
available for atomic-frame and secret-non-leakage contracts that cannot be
observed reliably from the final screen alone. Protocol/unit tests that
substitute collaborators are not part of the focused `make e2e` target; the full
`make all` command still runs them through the workspace test suite. `make all`
does not invoke `make e2e` separately because `cargo test --workspace` already
includes both black-box suites.

When `DIFFO_E2E_BINARY` is set to an absolute production executable, every
black-box suite launches those exact bytes and the standalone E2E package skips
its normal local Diffo build. This developer-only hook supports focused tests of
a prebuilt binary.
