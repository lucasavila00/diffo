# ADR 0070: Publish and self-update Diffo

Builds on [ADR 0039](0039-independent-app-modes.md),
[ADR 0110](0110-queue-command-intents.md), and
[ADR 0062](0062-use-procfs-image-for-askpass.md).

## Context

Diffo needs public releases, update discovery, and recovery when the TUI cannot
start. It remains a single-binary, no-configuration application. Updating a
privileged executable must not replace it with partial, corrupt, or
wrong-platform bytes.

## Decision

### Releases

Use `https://github.com/lucasavila00/diffo` as the public canonical repository
and its tagless `release` branch as the fixed publication and update authority.
Publish every validated push to `main`; do not create or require release tags.
Derive the stable version as
`<workspace-major>.<workspace-minor>.<first-parent-main-commit-count>` and embed
both that version and the validated source SHA.

CI owns publication. Its `checks` (`make all`) and `stress` jobs must both pass
before the publish job invokes the reusable release workflow with that exact
SHA. Serialize publication and skip an older validated commit if a newer one is
already published.

Build one statically linked `x86_64-unknown-linux-musl` executable supporting
Ubuntu 22.04 and newer. Preserve the legacy `diffo-x86_64-unknown-linux-gnu`
asset name and `x86_64-unknown-linux-gnu` manifest target so existing clients
stay on the same compatibility channel. Publish only that executable, unsigned
schema-1 update metadata, and `SHA256SUMS` as the root of a parentless commit,
then atomically force-update `release`. Other architectures are unsupported.

`DIFFO_E2E_BINARY` is a developer and test hook, not user configuration. When
unset, local `make all` keeps its normal development-profile behavior.

### Launcher

Dispatch before repository discovery, terminal setup, or application
construction:

- no argument starts Diffo;
- exactly `update` starts the embedded updater;
- every other argument is rejected.

The application never receives or parses CLI options. The updater is an isolated
entry path in the same executable, not a helper program, and must remain usable
when TUI initialization fails.

### Update protocol

Every stable release publishes:

```text
update-v1.json
diffo-x86_64-unknown-linux-gnu
SHA256SUMS
```

Fetch metadata through the fixed raw-content URL for the `release` branch. The
manifest contains its schema, Diffo version, and each asset's name, length,
target, and SHA-256 digest. HTTPS protects the metadata and asset in transit,
and the manifest digest detects a corrupt or unexpected asset. This protocol
does not provide publisher authenticity independent of GitHub and HTTPS.

Schema 1 is permanent. Ignore additive fields and publish incompatible protocols
alongside it. Future releases must retain schema-1 metadata and raw assets so
old launchers can reach a version supporting newer protocols.

The endpoint, protocol, and target mapping are fixed in code. The endpoint
environment override is a developer and test hook only.

### Replacement

Install only a strictly newer stable version. Equal or older versions produce an
up-to-date result without rewriting or downgrading the executable.

Resolve the current executable's actual regular-file path and download the exact
target asset to a newly created sibling. Before replacement, verify the target,
length, and digest, and reject unsupported schemas or insecure redirects. Set
the executable mode, flush the file, atomically rename it over that exact path,
and flush the directory. Any failure before rename leaves the binary unchanged.
Remove temporary files; install no backup, helper, cache, receipt, or
configuration.

The updater uses current process permissions and never invokes `sudo` or
requests credentials. Permission errors print
`sudo <resolved-current-path> update` to run outside Diffo.

Existing processes keep their original Linux executable image, including askpass
per ADR 0062. After an update, tell the user to quit and relaunch. Never reload
code in memory or restart automatically.

### Interface

Do not perform passive update checks or show availability notices. Updating is
always deliberate: use the fixed launcher argument or
`Application: Update
Diffo` from the shared command palette. Explicit updates
run outside the input loop through the workbench queue and remain cancellable.
Their result stays visible; success tells the user to quit and relaunch.
