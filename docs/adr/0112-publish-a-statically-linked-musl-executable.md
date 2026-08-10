# ADR 0112: Publish a statically linked musl executable

Supersedes the GNU-linked production build decision in
[ADR 0070](0070-publish-and-self-update-diffo.md). Retains the publication model
from [ADR 0075](0075-continuous-main-releases.md).

## Context

Diffo publishes one Linux executable. Building the `x86_64-unknown-linux-gnu`
target on Ubuntu 24.04 can introduce symbol-version requirements from glibc
2.39, so the result is not guaranteed to start on Ubuntu 22.04, whose glibc
version is 2.35. Pinning release builds to Ubuntu 22.04 would establish the
required baseline, but it would preserve the same coupling between the build
environment and the oldest usable glibc version.

The existing installation and update paths identify the executable as
`diffo-x86_64-unknown-linux-gnu` with manifest target
`x86_64-unknown-linux-gnu`. Released executables have those values fixed in
code. Renaming the asset or target immediately would strand existing clients;
requiring users to reinstall or invoke a different command would turn a libc
implementation detail into a visible migration.

Diffo uses Rustls rather than OpenSSL and does not link libgit2. Its production
executable is therefore suitable for static linking with musl without adding a
runtime libc package.

## Decision

Build the production executable for `x86_64-unknown-linux-musl` and statically
link its C runtime. Publish only that executable.

Preserve `diffo-x86_64-unknown-linux-gnu` as the release asset name and
`x86_64-unknown-linux-gnu` as the schema-1 manifest target. These values become
stable identifiers for the existing x86_64 Linux compatibility channel, not a
description of the executable's linked libc. Do not introduce a second musl
asset or a transitional release.

The first release implementing this decision publishes the musl executable under
the existing identifiers. Both the installer and every existing updater download
and atomically install it through their current paths. The installed musl
executable continues to request the same identifiers on later updates.

Keep the public installation command unchanged. Keep `diffo update`, including
the permission fallback that tells the user to run
`sudo <resolved-path>
update`, unchanged.

Support Ubuntu 22.04 and newer on x86_64. The release must run without a glibc
runtime dependency. Other architectures remain unsupported.

## Consequences

One publication migrates both new installations and existing installations to
the statically linked executable. Users do not reinstall Diffo, change the
installation command, or use a different update command. Clients may skip the
first musl release because every later release retains the identifiers they
already understand.

The executable no longer inherits a minimum glibc version from its build
environment. This removes the need to keep an old GNU build environment solely
to preserve runtime compatibility.

The published filename and manifest target retain `gnu` for protocol
compatibility even though the executable uses musl. Renaming them later would
require a separate protocol migration and provides no runtime benefit.

Diffo uses musl's libc and resolver behavior rather than the host's glibc and
NSS modules. Release validation must exercise startup, terminal handling, Git
subprocesses, update discovery, download, and self-replacement on Ubuntu 22.04.

## Alternatives considered

### Build the GNU target on Ubuntu 22.04

This is the smallest build-workflow change and establishes glibc 2.35 as the
current minimum. It is not selected because compatibility would remain coupled
to the release environment and could regress when that environment changes.

### Rename the asset and manifest target to musl

This would describe the executable accurately. It is not selected because old
executables have the GNU identifiers fixed in code and could not discover the
renamed asset. Retaining aliases or requiring a staged migration adds paths
without improving the resulting executable.

### Publish both GNU and musl executables

This preserves native GNU behavior while offering a static alternative. It is
not selected because Diffo deliberately publishes one Linux executable, and a
second target would add release, installer, updater, and test paths.
