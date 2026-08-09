use std::{collections::HashSet, fmt::Write as _, path::PathBuf};

use diffo_ai_config::MAX_AI_REVIEW_CONTEXT_BYTES;
use diffo_core::{ChangeKind, HeadState, RepositorySnapshot};
use diffo_diff::{RowKind, inline_rows, parse_unified_patch};

use crate::diff::{ChangeArea, FileKey};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttentionCategory {
    Behavior,
    Correctness,
    Security,
    Concurrency,
    ErrorPath,
    PublicApi,
    Performance,
    TestCoverage,
}

impl AttentionCategory {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "behavior" => Some(Self::Behavior),
            "correctness" => Some(Self::Correctness),
            "security" => Some(Self::Security),
            "concurrency" => Some(Self::Concurrency),
            "error-path" => Some(Self::ErrorPath),
            "public-api" => Some(Self::PublicApi),
            "performance" => Some(Self::Performance),
            "test-coverage" => Some(Self::TestCoverage),
            _ => None,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Behavior => "behavior",
            Self::Correctness => "correctness",
            Self::Security => "security",
            Self::Concurrency => "concurrency",
            Self::ErrorPath => "error path",
            Self::PublicApi => "public API",
            Self::Performance => "performance",
            Self::TestCoverage => "test coverage",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewHunk {
    pub id: String,
    pub file: FileKey,
    pub target: usize,
    pub header: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ReviewChange {
    file: FileKey,
    old_path: Option<PathBuf>,
    kind: ChangeKind,
    patch: String,
    hunks: Vec<ReviewHunk>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewRequest {
    expected_head: HeadState,
    changes: Vec<ReviewChange>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewStop {
    pub title: String,
    pub category: AttentionCategory,
    pub reason: String,
    pub primary_hunk_id: String,
    pub related_hunk_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewResult {
    pub overview: Vec<String>,
    pub stops: Vec<ReviewStop>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AskRequest {
    pub review_request: ReviewRequest,
    pub review: ReviewResult,
    pub selected_hunk_id: Option<String>,
    pub question: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AskResult {
    pub text: Vec<String>,
    pub hunk_ids: Vec<String>,
}

impl ReviewRequest {
    #[must_use]
    pub fn from_snapshot(snapshot: &RepositorySnapshot) -> Option<Self> {
        let mut entries = Vec::new();
        for area in [ChangeArea::Staged, ChangeArea::Unstaged] {
            let mut files = snapshot
                .files
                .iter()
                .filter_map(|file| {
                    let diff = match area {
                        ChangeArea::Staged => file.staged.as_ref(),
                        ChangeArea::Unstaged => file.unstaged.as_ref(),
                    }?;
                    Some((file, diff))
                })
                .collect::<Vec<_>>();
            files.sort_by(|(left, _), (right, _)| left.path.cmp(&right.path));
            entries.extend(files.into_iter().map(|(file, diff)| {
                (
                    FileKey {
                        path: file.path.clone(),
                        area,
                    },
                    file.old_path.clone(),
                    file.kind,
                    diff.text.clone(),
                )
            }));
        }
        if entries.is_empty() {
            return None;
        }

        let mut next_hunk = 1_usize;
        let changes = entries
            .into_iter()
            .map(|(file, old_path, kind, patch)| {
                let parsed = parse_unified_patch(&patch).ok();
                let targets = parsed
                    .as_ref()
                    .map(inline_rows)
                    .unwrap_or_default()
                    .into_iter()
                    .enumerate()
                    .filter_map(|(index, row)| (row.kind == RowKind::Header).then_some(index))
                    .collect::<Vec<_>>();
                let headers = parsed
                    .as_ref()
                    .map(|document| {
                        document
                            .hunks
                            .iter()
                            .map(|hunk| hunk.header.clone())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let count = targets.len().max(1);
                let hunks = (0..count)
                    .map(|index| {
                        let hunk = ReviewHunk {
                            id: format!("H{next_hunk:04}"),
                            file: file.clone(),
                            target: targets.get(index).copied().unwrap_or(0),
                            header: headers
                                .get(index)
                                .cloned()
                                .unwrap_or_else(|| "whole change".to_owned()),
                        };
                        next_hunk = next_hunk.saturating_add(1);
                        hunk
                    })
                    .collect();
                ReviewChange {
                    file,
                    old_path,
                    kind,
                    patch,
                    hunks,
                }
            })
            .collect();
        Some(Self {
            expected_head: snapshot.head.clone(),
            changes,
        })
    }

    #[must_use]
    pub fn still_matches(&self, snapshot: &RepositorySnapshot) -> bool {
        Self::from_snapshot(snapshot).is_some_and(|current| current == *self)
    }

    #[must_use]
    pub fn hunk(&self, id: &str) -> Option<&ReviewHunk> {
        self.changes
            .iter()
            .flat_map(|change| &change.hunks)
            .find(|hunk| hunk.id == id)
    }

    #[must_use]
    pub fn contains_hunk(&self, id: &str) -> bool {
        self.hunk(id).is_some()
    }

    #[must_use]
    pub fn prompt_context(&self, repository: &str) -> String {
        self.prompt_context_with_budget(repository, MAX_AI_REVIEW_CONTEXT_BYTES)
    }

    fn prompt_context_with_budget(&self, repository: &str, budget: usize) -> String {
        let prelude = format!(
            "<repository name=\"{}\">\n<changes total=\"{}\">\n",
            escaped(repository),
            self.changes.len()
        );
        let headers = self.changes.iter().map(change_header).collect::<Vec<_>>();
        let footers = self
            .changes
            .iter()
            .map(|_| "</patch>\n</change>\n".to_owned())
            .collect::<Vec<_>>();
        let patch_lengths = self
            .changes
            .iter()
            .map(|change| escaped(&change.patch).len())
            .collect::<Vec<_>>();
        let ending = "</changes>\n</repository>\n";
        let omission_reserve = 80;
        let fixed = prelude.len()
            + headers.iter().map(String::len).sum::<usize>()
            + footers.iter().map(String::len).sum::<usize>()
            + ending.len()
            + omission_reserve;
        let allocations = fair_allocations(&patch_lengths, budget.saturating_sub(fixed));
        let mut context = prelude;
        let mut included = 0_usize;
        for (((change, header), footer), allocation) in self
            .changes
            .iter()
            .zip(headers)
            .zip(footers)
            .zip(allocations)
        {
            if context.len().saturating_add(header.len() + footer.len() + ending.len()) > budget {
                break;
            }
            context.push_str(&header);
            let remaining = budget.saturating_sub(context.len() + footer.len() + ending.len());
            context.push_str(&sample_escaped(&change.patch, allocation.min(remaining)));
            context.push_str(&footer);
            included = included.saturating_add(1);
        }
        if included < self.changes.len() {
            writeln!(
                context,
                "<omitted-changes count=\"{}\" reason=\"context-budget\" />",
                self.changes.len().saturating_sub(included)
            )
            .expect("writing to a String cannot fail");
        }
        context.push_str(ending);
        debug_assert!(context.len() <= budget);
        context
    }

    #[must_use]
    pub fn validate_review(
        &self,
        overview: Vec<String>,
        stops: Vec<ReviewStop>,
    ) -> Option<ReviewResult> {
        if !(1..=3).contains(&overview.len())
            || !(1..=8).contains(&stops.len())
            || overview.iter().any(|line| !valid_text(line, 240))
        {
            return None;
        }
        let mut primary = HashSet::new();
        for stop in &stops {
            if !valid_text(&stop.title, 80)
                || !valid_text(&stop.reason, 240)
                || !self.contains_hunk(&stop.primary_hunk_id)
                || !primary.insert(&stop.primary_hunk_id)
                || stop.related_hunk_ids.len() > 4
                || stop
                    .related_hunk_ids
                    .iter()
                    .any(|id| !self.contains_hunk(id))
            {
                return None;
            }
        }
        Some(ReviewResult { overview, stops })
    }
}

impl AskRequest {
    #[must_use]
    pub fn prompt_context(&self, repository: &str) -> String {
        let mut suffix = String::new();
        suffix.push_str("<review-map>\n");
        for stop in &self.review.stops {
            writeln!(
                suffix,
                "<stop hunk=\"{}\" category=\"{}\">{}: {}</stop>",
                stop.primary_hunk_id,
                stop.category.label(),
                escaped(&stop.title),
                escaped(&stop.reason)
            )
            .expect("writing to a String cannot fail");
        }
        suffix.push_str("</review-map>\n");
        if let Some(selected) = &self.selected_hunk_id {
            writeln!(suffix, "<selected-hunk>{}</selected-hunk>", escaped(selected))
                .expect("writing to a String cannot fail");
        }
        writeln!(
            suffix,
            "<question>{}</question>",
            escaped(&self.question)
        )
        .expect("writing to a String cannot fail");
        let snapshot_budget = MAX_AI_REVIEW_CONTEXT_BYTES.saturating_sub(suffix.len());
        let mut context = self
            .review_request
            .prompt_context_with_budget(repository, snapshot_budget);
        context.push_str(&suffix);
        context
    }

    #[must_use]
    pub fn validate_answer(&self, text: Vec<String>, hunk_ids: Vec<String>) -> Option<AskResult> {
        if !(1..=3).contains(&text.len())
            || text.iter().any(|line| !valid_text(line, 240))
            || hunk_ids.len() > 5
            || hunk_ids
                .iter()
                .any(|id| !self.review_request.contains_hunk(id))
        {
            return None;
        }
        Some(AskResult { text, hunk_ids })
    }
}

fn change_header(change: &ReviewChange) -> String {
    let origin = match change.file.area {
        ChangeArea::Staged => "staged",
        ChangeArea::Unstaged => "unstaged",
    };
    let old_path = change.old_path.as_ref().map_or_else(String::new, |path| {
        format!(" old-path=\"{}\"", escaped(&path.to_string_lossy()))
    });
    let mut header = format!(
        "<change origin=\"{origin}\" path=\"{}\"{old_path} kind=\"{:?}\">\n<hunks>\n",
        escaped(&change.file.path.to_string_lossy()),
        change.kind
    );
    for hunk in &change.hunks {
        writeln!(
            header,
            "<hunk id=\"{}\" header=\"{}\" />",
            hunk.id,
            escaped(&hunk.header)
        )
        .expect("writing to a String cannot fail");
    }
    header.push_str("</hunks>\n<patch>\n");
    header
}

const OMISSION: &str = "\n[... oversized diff omitted ...]\n";

fn sample_escaped(text: &str, budget: usize) -> String {
    let text = escaped(text);
    if text.len() <= budget {
        return text;
    }
    if budget <= OMISSION.len() {
        return String::new();
    }
    let available = budget - OMISSION.len();
    let prefix_end = floor_char_boundary(&text, available.div_ceil(2));
    let suffix_start = ceil_char_boundary(&text, text.len().saturating_sub(available / 2));
    format!("{}{OMISSION}{}", &text[..prefix_end], &text[suffix_start..])
}

fn floor_char_boundary(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while !text.is_char_boundary(index) {
        index = index.saturating_sub(1);
    }
    index
}

fn ceil_char_boundary(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while index < text.len() && !text.is_char_boundary(index) {
        index += 1;
    }
    index
}

fn fair_allocations(lengths: &[usize], budget: usize) -> Vec<usize> {
    if lengths.is_empty() {
        return Vec::new();
    }
    let share = budget / lengths.len();
    let remainder = budget % lengths.len();
    lengths
        .iter()
        .enumerate()
        .map(|(index, length)| (*length).min(share + usize::from(index < remainder)))
        .collect()
}

fn valid_text(text: &str, maximum: usize) -> bool {
    !text.is_empty()
        && text == text.trim()
        && text.chars().count() <= maximum
        && !text.chars().any(char::is_control)
}

fn escaped(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&apos;"),
            _ => output.push(character),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use diffo_core::{FileDiff, FileState};

    use super::*;

    fn snapshot(patch: String) -> RepositorySnapshot {
        RepositorySnapshot {
            files: vec![FileState {
                path: "src/lib.rs".into(),
                old_path: None,
                kind: ChangeKind::Modified,
                staged: Some(FileDiff {
                    text: patch.clone(),
                }),
                unstaged: Some(FileDiff { text: patch }),
            }],
            ..RepositorySnapshot::default()
        }
    }

    #[test]
    fn assigns_distinct_stable_ids_to_staged_and_unstaged_hunks() {
        let request = ReviewRequest::from_snapshot(&snapshot(
            "@@ -1 +1 @@\n-old\n+new\n@@ -4 +4 @@\n-a\n+b\n".to_owned(),
        ))
        .unwrap();
        let context = request.prompt_context("repo");
        for id in ["H0001", "H0002", "H0003", "H0004"] {
            assert!(context.contains(id));
        }
        assert!(context.contains("origin=\"staged\""));
        assert!(context.contains("origin=\"unstaged\""));
    }

    #[test]
    fn oversized_context_keeps_both_projections_and_omission_markers() {
        let patch = format!("@@ -1 +1 @@\n-old\n+{}\n", "x".repeat(400_000));
        let request = ReviewRequest::from_snapshot(&snapshot(patch)).unwrap();
        let context = request.prompt_context("repo");
        assert!(context.len() <= MAX_AI_REVIEW_CONTEXT_BYTES);
        assert!(context.contains("origin=\"staged\""));
        assert!(context.contains("origin=\"unstaged\""));
        assert!(context.contains("oversized diff omitted"));
    }

    #[test]
    fn rejects_an_invented_hunk_id() {
        let request = ReviewRequest::from_snapshot(&snapshot(
            "@@ -1 +1 @@\n-old\n+new\n".to_owned(),
        ))
        .unwrap();
        assert!(
            request
                .validate_review(
                    vec!["Overview".to_owned()],
                    vec![ReviewStop {
                        title: "Stop".to_owned(),
                        category: AttentionCategory::Behavior,
                        reason: "Reason".to_owned(),
                        primary_hunk_id: "H9999".to_owned(),
                        related_hunk_ids: Vec::new(),
                    }],
                )
                .is_none()
        );
    }
}
