# diffo-update

`diffo-update` owns Diffo's fixed public GitHub Releases update protocol. It parses
schema-1 metadata, selects the one supported GNU/Linux target, verifies downloaded
bytes by length and SHA-256 digest, and atomically replaces the resolved running
executable. Release builds use the stable Git tag embedded by the release workflow as
their current update version; ordinary builds fall back to the Cargo package version.

The crate does not own user interface state, terminal setup, repository access,
privilege elevation, configuration, release authentication, or release publication.
The endpoint environment override exists only as a developer and test hook.
