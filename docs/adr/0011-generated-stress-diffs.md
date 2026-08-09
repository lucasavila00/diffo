# ADR 0011: Generated stress diffs

## Decision

Mock mode generates large patches at runtime. Do not store them in RON.

Generate files with:

- 5,000 lines.
- 50,000 lines.
- 500,000 lines.
- 5,000,000 lines.

Generate deterministic Rust source with varied declarations and pseudo-random
unique names. This makes scroll movement and anchoring visible. Do not use one
repeated placeholder line.

Keep the normal generated Rust, JSON, and long-line cases too.
