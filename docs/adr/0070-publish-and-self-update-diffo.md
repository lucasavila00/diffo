# ADR 0070: Publish and self-update Diffo from GitHub Releases

Status: Accepted

Builds on [ADR 0039](0039-independent-app-modes.md),
[ADR 0055](0055-command-queue.md), and
[ADR 0062](0062-use-procfs-image-for-askpass.md).

## Context

Diffo needs public releases, update discovery, and recovery when the TUI cannot start.
It remains a single-binary, no-configuration application.
Updating a privileged executable must not replace it with partial, corrupt,
wrong-platform, or untrusted bytes.

## Decision

### Releases

Use `https://github.com/lucasavila00/diffo` as the public canonical repository and
GitHub Releases as the fixed release and update authority. Publish stable releases
from immutable `v<major>.<minor>.<patch>` tags matching the workspace version. Ignore
drafts and prereleases.

Support only Debian stable and Ubuntu 24.04 or newer on x86_64 GNU/Linux. Other Linux
distributions and architectures are unsupported. Compile exactly one production
`x86_64-unknown-linux-gnu` executable on Ubuntu 24.04 for both distributions. Run
`DIFFO_E2E_BINARY=<absolute-production-path> make all`; every black-box E2E test must
launch those exact bytes and must not build another Diffo. Smoke-test the same file on
Ubuntu 24.04 and Debian stable, then publish it as the single Linux asset with SHA-256
digests, signed update metadata, and a GitHub artifact attestation. Installation
documentation tells users to verify the binary and place it at any path they choose.

`DIFFO_E2E_BINARY` is a developer and release-test hook, not user configuration. When
unset, local `make all` keeps its normal development-profile behavior.

### Launcher

Dispatch before repository discovery, terminal setup, or application construction:

- no argument starts Diffo;
- exactly `update` starts the embedded updater;
- every other argument is rejected.

The application never receives or parses CLI options. The updater is an isolated entry
path in the same executable, not a helper program, and must remain usable when TUI
initialization fails.

### Update protocol

Every stable release publishes:

```text
update-v1.json
update-v1.json.sig
diffo-<target-triple>
```

Fetch metadata through GitHub's permanent latest-release URLs. The manifest contains
its schema, Diffo version, and each asset's name, length, target, and SHA-256 digest.
Verify it with the release public key compiled into Diffo.

Schema 1 is permanent. Ignore additive fields and publish incompatible protocols
alongside it. Future releases must retain schema-1 metadata and raw assets so old
launchers can reach a version supporting newer protocols. Rotate signing keys through
a bridge release signed by the old key. Never fall back to unsigned updates.

The endpoint, key, protocol, and target mapping are fixed in code. Environment
overrides are developer and test hooks only.

### Replacement

Install only a strictly newer stable version. Equal or older versions produce an
up-to-date result without rewriting or downgrading the executable.

Resolve the current executable's actual regular-file path and download the exact target
asset to a newly created sibling. Before replacement, verify the manifest signature,
target, length, and digest, and reject unsupported schemas or insecure redirects. Set
the executable mode, flush the file, atomically rename it over that exact path, and
flush the directory. Any failure before rename leaves the binary unchanged. Remove
temporary files; install no backup, helper, cache, receipt, or configuration.

The updater uses current process permissions and never invokes `sudo` or requests
credentials. Permission errors print `sudo <resolved-current-path> update` to run
outside Diffo.

Existing processes keep their original Linux executable image, including askpass per
ADR 0062. After an update, tell the user to quit and relaunch. Never reload code in
memory or restart automatically.

### Interface

After the first frame, perform one background manifest check per process without
blocking startup or rendering. Store no check or dismissal state. Network and
verification failures during this passive check are silent.

For a verified newer version, show one persistent informational toast naming both
versions without taking focus. Add `Application: Update Diffo` to the shared F1 palette
in every activity; the palette command is the only in-TUI way to start the update. Run
updates in a separate process through the workbench command queue. Show persistent
success, verification, network, and permission results. Success tells the user to quit
and relaunch.

## Verification

- Test launcher dispatch before TUI initialization and rejection of other arguments.
- Test schema compatibility, versions, targets, signatures, lengths, and digests.
- Fault every filesystem stage and prove only complete verified bytes become visible.
- Test permission failures without elevation or partial replacement.
- Test passive discovery without focus changes, F1 availability, and persistent results.
- Before publication, prove every black-box E2E process used `DIFFO_E2E_BINARY`, then
  update the previous release on Debian stable and Ubuntu 24.04 and verify the new
  binary starts and its launcher can still check for updates.
