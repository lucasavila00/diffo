# CLI utilities

A Rust workspace for small command-line utilities.

## Utilities

- [`diffo`](crates/diffo): browse the current repository's Git state in a terminal UI.

## Install

Diffo supports Debian stable and Ubuntu 24.04 or newer on x86_64 GNU/Linux. Download
the stable release asset and its checksums from GitHub, verify both the digest and the
GitHub artifact attestation, then install the verified file at a path you choose:

```sh
version=v0.1.0
base=https://github.com/lucasavila00/diffo/releases/download/$version
curl --fail --location --remote-name "$base/diffo-x86_64-unknown-linux-gnu"
curl --fail --location --remote-name "$base/SHA256SUMS"
grep ' diffo-x86_64-unknown-linux-gnu$' SHA256SUMS | sha256sum --check
gh attestation verify diffo-x86_64-unknown-linux-gnu --repo lucasavila00/diffo
install -m 0755 diffo-x86_64-unknown-linux-gnu "$HOME/.local/bin/diffo"
```

Run `diffo update` to check and install a strictly newer signed stable release. The
same action is available as `Application: Update Diffo` in the F1 command palette.
Diffo uses the permissions of the current process and never invokes `sudo` itself.

## Development

```sh
make diffo
make diffo-mock
make all
```

`make diffo` reads the current Git repository. `make diffo-mock` loads a mutable,
in-memory repository from `crates/diffo-core/fixtures/repository-state.ron`. Stage,
unstage, and stage-all work for the life of the process without changing the fixture.
It also generates large changes on demand: a 20,000-line Rust file, 5,000 JSON
items, and a 25,000-byte line. These payloads are not stored in the repo.
It also includes generated 5k, 50k, 500k, and 5,000k-line stress patches. The fixture
covers staged and unstaged changes, untracked files, recent commits, and commits that
have not been pushed.

Set `DIFFO_MOCK_FILE` directly to preview another RON fixture without changing the normal
application behavior.

`make all` is the only repository validation command. Always run it before
considering a change complete; it is the single source of truth for CI checks.

### Release signing setup

The tag-driven release workflow requires an Ed25519 key pair. Store the base64 of the
unencrypted PKCS#8 PEM private key as the `DIFFO_UPDATE_SIGNING_KEY` repository secret,
and store the base64 of its raw 32-byte public key as the
`DIFFO_UPDATE_PUBLIC_KEY` repository variable. The workflow derives the public key
from the secret and fails before building if they do not match. Never commit the
private key. Stable tags must exactly match `v<workspace-version>`.

For debugging, `DIFFO_DUMP_PATH=state.ron make diffo` writes one repository snapshot
and exits without opening the TUI. The launcher accepts only the fixed `update`
maintenance argument; the application receives no command-line arguments.

## Crate documentation

Every package under `crates/` has its own `README.md`. That file is the source of
truth for the package overview and is included verbatim as the crate-level rustdoc,
so the repository and generated API documentation cannot drift. API-specific details
remain on the Rust items they describe.

Build the workspace documentation with:

```sh
cargo doc --workspace --no-deps
```

When a crate's purpose or boundaries change, update its README rather than adding a
duplicate crate overview to `lib.rs` or `main.rs`. See
[`ADR 0051`](docs/adr/0051-crate-documentation.md) for the decision and tradeoffs.

## Keyboard controls

Diffo's keyboard shortcuts are fixed and always use lowercase characters. Uppercase
characters are never assigned as shortcuts, so no action requires holding Shift.
Non-character keys such as arrows, function keys, Enter, and Escape may still be
used where they fit the interaction.

## TUI architecture invariants

Structural application chrome uses one fixed dark gray from `diffo-ui`; individual
renderers do not choose raw terminal colors. Every box border, divider, scrollbar,
and selection background shares that gray, while emphasis and markers show focus or
activity without changing hue. Widths, heights, panel/dialog insets, gaps, and overlay
bounds also use semantic tokens from `diffo-ui` instead of renderer-local literals.
Semantic content, diff rows, and syntax highlighting retain their meaning-specific
colors. See
[`ADR 0052`](docs/adr/0052-semantic-chrome-colors.md).

Diffo is designed for use over SSH, so terminal input and output must always be
treated as network traffic. Buttons and other controls keep a stable appearance as
the pointer moves over them: hover-only state and redraws consume network and CPU
resources for little value, particularly on slow, high-latency, or metered
connections. Mouse clicks, drags, and wheel actions remain supported. See
[`ADR 0038`](docs/adr/0038-remove-button-hover-changes.md).

Diff-buffer changes are atomic. While a selected file is being prepared, Diffo keeps
the last committed buffer and viewport unchanged. It commits the replacement's
content, projections, hunk targets, scroll bounds, and initial position together
before one draw. Rendering must never poll or install background preparation results,
and stale results must never become visible. See
[`ADR 0024`](docs/adr/0024-atomic-diff-buffer-transitions.md).

Syntax preparation is viewport-bounded but remains part of that atomic commit. A
9,999-line Rust fixture previously took about 3.45–3.56 seconds in debug and 640 ms
in release because both complete file versions were highlighted. The same cold open
now measures 72–98 ms in debug and 31 ms in release by highlighting the first visible
window with bounded parser context and parallel old/new sides. The full measurements
and tradeoffs are in [`ADR 0032`](docs/adr/0032-bounded-syntax-windows.md).

Only the active inline or side-by-side projection is built for a cold open. Switching
modes is itself an atomic prepared transition, and returning to a recently prepared
file or mode uses a four-entry cache.

Uncached vertical jumps also wait for their colored target window; the current
viewport remains unchanged until content, colors, targets, bounds, and position can
commit together. Syntax remains enabled below 10,000 file lines.

The vertical scrollbar and hunk overview are separate controls. The scrollbar owns
the inner track; hunk markers own the adjacent right-border rail. Neither control may
paint over or capture clicks intended for the other.
