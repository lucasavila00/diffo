# ADR 0031: Remove read-only mode

## Decision

Diffo repositories are always actionable.

- Remove `AccessMode` and repository capability checks.
- Always show and dispatch stage, commit, fetch, pull, and push actions.
- Always load mock fixtures through the mutable in-memory repository.
- Treat rejected actions as operation failures, not a product mode.

Mock fixtures enter through the same mutable in-memory repository boundary.
