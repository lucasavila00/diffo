# ADR 0070: Publish and self-update Diffo from GitHub Releases

Status: Accepted

Builds on [ADR 0039](0039-independent-app-modes.md),
[ADR 0055](0055-command-queue.md), and
[ADR 0062](0062-use-procfs-image-for-askpass.md).

## Context

Diffo needs public releases, update discovery, and recovery when the TUI cannot start.
It remains a single-binary, no-configuration application.
Updating a privileged executable must not replace it with partial, corrupt, or
wrong-platform bytes.

## Decision

### Releases

Use `https://github.com/lucasavila00/diffo` as the public canonical repository and
GitHub Releases as the fixed release and update authority. Publish stable releases
from immutable `<major>.<minor>.<patch>` tags, using the tag as the release version.
Ignore drafts and prereleases.

Support only Debian stable and Ubuntu 24.04 or newer on x86_64 GNU/Linux. Other Linux
distributions and architectures are unsupported. Compile exactly one production
`x86_64-unknown-linux-gnu` executable on Ubuntu 24.04 for both distributions. Publish
only that executable, unsigned schema-1 update metadata, and `SHA256SUMS`. The release
workflow does not repeat tests or the full `make all` suite owned by repository CI.
Installation documentation tells users to verify the binary and place it at any path
they choose.

`DIFFO_E2E_BINARY` is a developer and test hook, not user configuration. When unset,
local `make all` keeps its normal development-profile behavior.

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
diffo-x86_64-unknown-linux-gnu
SHA256SUMS
```

Fetch metadata through GitHub's permanent latest-release URLs. The manifest contains
its schema, Diffo version, and each asset's name, length, target, and SHA-256 digest.
HTTPS protects the metadata and asset in transit, and the manifest digest detects a
corrupt or unexpected asset. This protocol does not provide publisher authenticity
independent of GitHub and HTTPS.

Schema 1 is permanent. Ignore additive fields and publish incompatible protocols
alongside it. Future releases must retain schema-1 metadata and raw assets so old
launchers can reach a version supporting newer protocols.

The endpoint, protocol, and target mapping are fixed in code. The endpoint environment
override is a developer and test hook only.

### Replacement

Install only a strictly newer stable version. Equal or older versions produce an
up-to-date result without rewriting or downgrading the executable.

Resolve the current executable's actual regular-file path and download the exact target
asset to a newly created sibling. Before replacement, verify the target, length, and
digest, and reject unsupported schemas or insecure redirects. Set
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
- Test schema compatibility, versions, targets, filenames, lengths, and digests.
- Fault every filesystem stage and prove only complete verified bytes become visible.
- Test permission failures without elevation or partial replacement.
- Test passive discovery without focus changes, F1 availability, and persistent results.
- Verify release builds derive their embedded version from the stable Git tag.
