use std::{fmt::Write as _, hint::black_box, path::Path};

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main, measurement::WallTime};
use diffo_diff::{DiffDocument, parse_unified_patch};
use diffo_highlight::{
    HIGHLIGHT_LOOKBEHIND_LINES, HighlightWindowRequest, LineRange, MAX_HIGHLIGHT_BYTES_PER_SIDE,
    MAX_HIGHLIGHT_FILE_LINES, SyntaxHighlighter,
};

const VIEWPORT_LINES: u32 = 40;
const DEEP_VIEWPORT_START: u32 = 9_000;

struct Language {
    name: &'static str,
    path: &'static str,
    line: fn(usize) -> String,
}

fn rust_line(number: usize) -> String {
    format!("pub const LINE_{number:05}: usize = {number};")
}

fn typescript_line(number: usize) -> String {
    format!("export const item{number} = (value: number | undefined): number => value ?? {number};")
}

fn json_line(number: usize) -> String {
    format!(r#""item_{number}": {{"enabled": true, "value": {number}}},"#)
}

fn markdown_line(number: usize) -> String {
    format!("* Item **{number}** links to [`value_{number}`](https://example.test/{number}).")
}

const LANGUAGES: &[Language] = &[
    Language {
        name: "rust",
        path: "src/generated.rs",
        line: rust_line,
    },
    Language {
        name: "typescript",
        path: "src/generated.tsx",
        line: typescript_line,
    },
    Language {
        name: "json",
        path: "generated.json",
        line: json_line,
    },
    Language {
        name: "markdown",
        path: "generated.md",
        line: markdown_line,
    },
];

fn document(language: &Language, lines: usize) -> DiffDocument {
    let mut patch = format!("@@ -1,{lines} +1,{lines} @@\n");
    for number in 1..=lines {
        writeln!(patch, " {}", (language.line)(number)).expect("writing to a String cannot fail");
    }
    parse_unified_patch(&patch).expect("benchmark patch must be valid")
}

fn window(start: u32) -> HighlightWindowRequest {
    let range = LineRange::new(start, start + VIEWPORT_LINES - 1);
    HighlightWindowRequest {
        old: Some(range),
        new: Some(range),
        lookbehind_lines: HIGHLIGHT_LOOKBEHIND_LINES,
        maximum_bytes_per_side: MAX_HIGHLIGHT_BYTES_PER_SIDE,
    }
}

fn bench_initialization(criterion: &mut Criterion) {
    criterion.bench_function("initialize/syntax-highlighter", |bencher| {
        bencher.iter(|| black_box(SyntaxHighlighter::new()));
    });
}

fn bench_windows(criterion: &mut Criterion) {
    let highlighter = SyntaxHighlighter::new();
    let mut group = criterion.benchmark_group("window");

    for language in LANGUAGES {
        let document = document(language, MAX_HIGHLIGHT_FILE_LINES - 1);
        for (position, start) in [("top", 1), ("deep", DEEP_VIEWPORT_START)] {
            group.bench_with_input(
                BenchmarkId::new(language.name, position),
                &start,
                |bencher, start| {
                    bencher.iter(|| {
                        black_box(highlighter.highlight_window(
                            Path::new(language.path),
                            &document,
                            window(*start),
                        ))
                    });
                },
            );
        }
    }
    group.finish();
}

fn bench_reference_boundary(criterion: &mut Criterion) {
    let language = &LANGUAGES[0];
    let document = document(language, MAX_HIGHLIGHT_FILE_LINES - 1);
    let highlighter = SyntaxHighlighter::new();
    let mut group: criterion::BenchmarkGroup<'_, WallTime> =
        criterion.benchmark_group("reference-boundary");
    group.sample_size(10);
    group.bench_function("rust/9999-lines", |bencher| {
        bencher.iter(|| black_box(highlighter.highlight(Path::new(language.path), &document)));
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_initialization,
    bench_windows,
    bench_reference_boundary
);
criterion_main!(benches);
