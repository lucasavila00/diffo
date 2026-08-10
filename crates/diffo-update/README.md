# diffo-update

`diffo-update` owns Diffo's fixed public update protocol served from the tagless
`release` branch. It parses schema-1 metadata, selects the one supported
`x86_64` Linux update target, verifies downloaded bytes by length and SHA-256
digest, and atomically replaces the resolved running executable. The target and
asset retain their original GNU names as compatibility identifiers while the
published executable is statically linked with musl. Release builds use the
continuous mainline version embedded by the release workflow as their current
update version; ordinary builds fall back to the Cargo package version. That
version is an internal ordering key; update outcomes identify builds by their
source commit SHA.

The crate does not own user interface state, terminal setup, repository access,
privilege elevation, configuration, release authentication, or release
publication. The endpoint environment override exists only as a developer and
test hook.
