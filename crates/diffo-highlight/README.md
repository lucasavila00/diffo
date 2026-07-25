# diffo-highlight

`diffo-highlight` provides bounded syntax highlighting for Diffo.

It highlights visible code windows with bundled syntax definitions and exposes token
text with Monokai Extended foreground colors. Theme backgrounds and font attributes
are intentionally excluded so every code view has the same syntax style. Fixed line,
look-behind, and byte limits keep syntax work off the full-file critical path.

## Performance

Run the syntax-highlighting benchmarks with:

```sh
cargo bench --package diffo-highlight --bench highlight
```

Criterion accepts a name filter for focused comparisons. For example, this profiles
only a deep Rust viewport for 30 seconds, making it suitable for `perf` or a
flamegraph:

```sh
cargo bench --package diffo-highlight --bench highlight -- \
  'window/rust/deep' --profile-time 30
```

Benchmark inputs are generated before measurement. The harness separately measures
highlighter initialization, bounded viewport highlighting for representative
syntaxes, and the 9,999-line reference boundary.
