# ADR 0011: Generated stress diffs

Status: Accepted

## Decision

Mock mode generates large patches at runtime. Do not store them in RON.

Generate files with:

- 5,000 lines.
- 50,000 lines.
- 500,000 lines.
- 5,000,000 lines.

Use tiny `+x` lines. Stress row count, parser cost, memory, background loading, and
scrolling. Do not waste memory on large line contents.

Keep the normal generated Rust, JSON, and long-line cases too.

## Tests

- Check every generated path exists.
- Check every patch has the exact requested added-line count.
- Keep fixture files small in Git.
