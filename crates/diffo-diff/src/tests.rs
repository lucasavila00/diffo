use super::{
    DiffBlock, ProjectionOptions, RenderLine, RowKind, SideBySideRow, inline_change_starts,
    inline_rows, inline_rows_with_options, parse_unified_patch, side_by_side_change_starts,
    side_by_side_rows, side_by_side_rows_with_options,
};

const PATCH: &str = "diff --git a/file.txt b/file.txt\n--- a/file.txt\n+++ b/file.txt\n@@ -1,4 +1,4 @@\n same\n-old one\n-old two\n+new one\n+new two\n end\n";

fn compact_line(line: &RenderLine) -> String {
    format!("{:?} {:?}: {}", line.kind, line.number, line.text)
}

fn compact_side(rows: &[SideBySideRow]) -> Vec<String> {
    rows.iter()
        .map(|row| {
            format!(
                "{:?} | {} | {}",
                row.kind,
                row.old.as_ref().map_or_else(|| "∅".to_owned(), compact_line),
                row.new.as_ref().map_or_else(|| "∅".to_owned(), compact_line),
            )
        })
        .collect()
}

#[test]
fn parses_hunks_and_line_numbers() {
    let document = parse_unified_patch(PATCH).expect("patch should parse");

    insta::assert_debug_snapshot!(document);
}

#[test]
fn projects_inline_and_side_by_side_rows() {
    let document = parse_unified_patch(PATCH).expect("patch should parse");
    let inline = inline_rows(&document);
    let side = side_by_side_rows(&document);

    let compact_inline = inline.iter().map(compact_line).collect::<Vec<_>>();
    insta::assert_debug_snapshot!((compact_inline, compact_side(&side)));
}

#[test]
fn keeps_separate_change_blocks_as_navigation_targets() {
    let patch = "@@ -1,7 +1,7 @@\n one\n-old two\n+new two\n three\n four\n-old five\n+new five\n six\n seven\n";
    let document = parse_unified_patch(patch).expect("patch should parse");
    let inline = inline_rows(&document);
    let side = side_by_side_rows(&document);

    assert_eq!(inline_change_starts(&inline).len(), 2);
    assert_eq!(side_by_side_change_starts(&side).len(), 2);
}

#[test]
fn detects_binary_and_rejects_combined_diff() {
    assert!(
        parse_unified_patch("Binary files a/x and b/x differ")
            .expect("binary")
            .binary
    );
    assert!(
        parse_unified_patch("GIT binary patch\nliteral 1\nabc")
            .expect("binary patch")
            .binary
    );
    assert!(parse_unified_patch("diff --cc file\n@@@ -1 -1 +1 @@@").is_err());
    assert!(parse_unified_patch("@@@ -1 -1 +1 @@@").is_err());
}

#[test]
fn near_matches_are_not_git_metadata() {
    for text in [
        "prefix GIT binary patch",
        " GIT binary patch",
        "Binary file a/x changed",
        " Binary files a/x and b/x differ",
        "diff --cached file.rs",
        "text @@@ -1 -1 +1 @@@",
    ] {
        let document = parse_unified_patch(text).expect("near match should be accepted");
        assert!(!document.binary, "near match: {text}");
    }
}

#[test]
fn binary_markers_inside_source_code_are_not_binary_metadata() {
    let patch = "@@ -1 +1 @@\n-if patch.contains(\"GIT binary patch\") {}\n+if line == \"GIT binary patch\" {}\n";

    let document = parse_unified_patch(patch).expect("text patch should parse");

    assert!(!document.binary);
    assert_eq!(document.hunks.len(), 1);
}

#[test]
fn git_metadata_sentinels_are_plain_file_content_inside_hunks() {
    for sentinel in [
        "GIT binary patch",
        "Binary files a/x and b/x differ",
        "diff --cc file.rs",
        "@@@ -1 -1 +1 @@@",
    ] {
        let patch = format!("@@ -1,2 +1,2 @@\n {sentinel}\n-{sentinel}\n+{sentinel}\n");
        let document = parse_unified_patch(&patch).expect("content patch should parse");

        assert!(!document.binary, "sentinel: {sentinel}");
        assert_eq!(document.hunks.len(), 1, "sentinel: {sentinel}");
        let blocks = &document.hunks[0].blocks;
        assert!(matches!(blocks[0], DiffBlock::Context(_)));
        let DiffBlock::Change { removed, added, .. } = &blocks[1] else {
            panic!("sentinel did not remain change content: {sentinel}");
        };
        assert_eq!(removed[0].text, sentinel);
        assert_eq!(added[0].text, sentinel);
    }
}

#[test]
fn metadata_sentinels_remain_content_across_multiple_hunks() {
    let patch = "@@ -1 +1 @@\n-GIT binary patch\n+Binary files a/x and b/x differ\n@@ -10 +10 @@\n-diff --cc file.rs\n+@@@ -1 -1 +1 @@@\n";

    let document = parse_unified_patch(patch).expect("two text hunks should parse");

    assert!(!document.binary);
    assert_eq!(document.hunks.len(), 2);
}

#[test]
fn promotes_merge_markers_to_conflict_rows() {
    let patch = "@@ -1 +1,5 @@\n-old\n+<<<<<<< HEAD\n+ours\n+=======\n+theirs\n+>>>>>>> branch\n";
    let document = parse_unified_patch(patch).expect("conflict patch should parse");
    let ordinary = inline_rows(&document);
    assert!(ordinary.iter().all(|row| row.kind != RowKind::Conflict));

    let options = ProjectionOptions {
        mark_conflicts: true,
    };
    let inline = inline_rows_with_options(&document, options);

    let compact_inline = inline.iter().map(compact_line).collect::<Vec<_>>();
    let side = side_by_side_rows_with_options(&document, options);
    insta::assert_debug_snapshot!((compact_inline, compact_side(&side)));
}

#[test]
fn every_conflict_sentinel_requires_conflicted_projection() {
    let patch = "@@ -1,4 +1,4 @@\n-<<<<<<< old\n+<<<<<<< new\n-||||||| base\n+=======\n->>>>>>> old\n+>>>>>>> new\n unchanged\n";
    let document = parse_unified_patch(patch).expect("marker content should parse");

    assert!(
        inline_rows(&document)
            .iter()
            .all(|row| row.kind != RowKind::Conflict)
    );
    let marked = inline_rows_with_options(
        &document,
        ProjectionOptions {
            mark_conflicts: true,
        },
    );
    assert_eq!(
        marked
            .iter()
            .filter(|row| row.kind == RowKind::Conflict)
            .count(),
        6
    );
}
