# diffo-e2e

`diffo-e2e` is the end-to-end test support crate for Diffo.

It drives the compiled terminal application through a pseudo-terminal and exposes
screen selectors and input helpers to the real-Git tests. Network-operation tests use
a loopback OpenSSH server with generated keys and temporary bare repositories; they
never use a public network. The test environment requires `git`, `ssh`, `sshd`, and
`ssh-keygen`. This package is not published.

The E2E boundary is black-box wherever the behavior permits it. Tests launch the
compiled Diffo executable, drive its terminal UI, use real temporary files and Git
repositories, and assert rendered output, exit status, filesystem content, Git refs,
and remote state. They do not inject repository snapshots, replace Git or SSH, or
assert private socket, temporary-directory, or helper-image details.

Developer trace output and deterministic preparation delays are reserved for timing,
atomic-frame, and secret-non-leakage contracts that cannot be observed reliably from
the final screen alone. Those tests still use the compiled executable and real Git
state, and their functional assertions remain at the public UI and repository
boundary. Protocol/unit tests that substitute collaborators are not part of the
focused `make e2e` target; the full `make all` command still runs them through the
workspace test suite. `make all` does not invoke `make e2e` separately because
`cargo test --workspace` already includes both black-box suites.
