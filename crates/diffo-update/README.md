# diffo-update

`diffo-update` owns Diffo's fixed GitHub Releases update protocol. It verifies signed
schema-1 metadata, selects the one supported GNU/Linux target, validates downloaded
bytes, and atomically replaces the resolved running executable. Release builds use
the stable Git tag embedded by the release workflow as their current update version;
ordinary builds fall back to the Cargo package version.

The crate does not own user interface state, terminal setup, repository access,
privilege elevation, configuration, or release publication. Endpoint and public-key
environment overrides exist only as developer and test hooks.
