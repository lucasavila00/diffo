# ADR 0051: Crate READMEs are crate-level rustdocs

## Problem

Every workspace package needs a useful landing page in the repository, in Cargo
package metadata, and in generated Rust API documentation. Maintaining a README
and separate crate-level `//!` documentation would duplicate the same overview
and allow the two versions to drift.

A README generator could copy crate-level rustdocs into checked-in files, but
that would add a tool and a generated-file freshness check for content that
rustdoc can already include directly.

## Decision

Every package under `crates/` has a checked-in `README.md` and declares it with
`readme = "README.md"` in its package manifest.

The README is the source of truth for the crate overview. Each library crate
includes it at the top of `src/lib.rs`, and each binary-only crate includes it
at the top of `src/main.rs`:

```rust
#![doc = include_str!("../README.md")]
```

Rustdoc therefore consumes the exact checked-in README instead of a copied or
separately maintained version. This reverses the usual generator direction while
preserving the important property: repository documentation and crate-level
rustdocs have one source.

Crate READMEs describe purpose, responsibilities, and boundaries. Documentation
for specific public APIs stays with those Rust items. The root README remains
the workspace and product overview.

## Consequences

- Git hosts, Cargo package metadata, and rustdoc show the same crate overview.
- Documentation builds require no extra generator or installed Cargo subcommand.
- README Markdown must also be valid and useful when rendered by rustdoc.
- Changes to a crate's role or boundaries require an update to its README.
