# diffo-update

`diffo-update` owns Diffo's fixed GitHub Releases update protocol. It verifies signed
schema-1 metadata, selects the one supported GNU/Linux target, validates downloaded
bytes, and atomically replaces the resolved running executable.

The crate does not own user interface state, terminal setup, repository access,
privilege elevation, configuration, or release publication. Endpoint and public-key
environment overrides exist only as developer and test hooks.
