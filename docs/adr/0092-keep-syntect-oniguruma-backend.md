# ADR 0092: Keep syntect's Oniguruma backend

Status: Rejected

Evaluates
[Tune the syntect engine](../highlight-performance/07-syntect-engine-tuning.md).

## Problem

The bat-compatible syntax bundle uses syntect with the native Oniguruma regular
expression engine. Syntect also supports the pure-Rust `fancy-regex` engine, which
could have changed parsing cost and removed a native dependency.

## Result

We rebuilt `two-face` with `syntect-fancy` instead of its default
`syntect-onig` feature. All highlighting and syntax snapshot tests passed.

Rust deep-window highlighting regressed from about 11.6 ms to 39.9 ms, or roughly
244%. The result already failed the no-regression rule, so the longer TypeScript run
was stopped.

Pruning languages from the bundle was not combined with this test. Removing the
curated bat syntax coverage would violate this hypothesis's correctness condition.

## Decision

Keep the Oniguruma backend. The pure-Rust backend is useful for portability, but it
is not a performance improvement for Diffo's workload.
