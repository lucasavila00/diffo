use std::{collections::HashSet, fmt::Write as _, path::PathBuf};

use diffo_ai_config::{MAX_AI_REVIEW_CONTEXT_BYTES, MAX_AI_REVIEW_TARGETS_PER_CHANGE};
use diffo_core::{ChangeKind, HeadState, RepositorySnapshot};
use diffo_diff::{inline_change_regions, inline_rows, parse_unified_patch};

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
    #[must_use]
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

    #[must_use]
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
pub struct ReviewTarget {
    pub id: String,
    pub file: FileKey,
    pub diff_row: usize,
    pub location: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ReviewChange {
    file: FileKey,
    old_path: Option<PathBuf>,
    kind: ChangeKind,
    patch: String,
    targets: Vec<ReviewTarget>,
    omitted_targets: usize,
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
    pub target_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewResult {
    pub overview: Vec<String>,
    pub stops: Vec<ReviewStop>,
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

        let mut used_ids = HashSet::new();
        let changes = entries
            .into_iter()
            .map(|(file, old_path, kind, patch)| {
                let parsed = parse_unified_patch(&patch).ok();
                let rows = parsed.as_ref().map(inline_rows).unwrap_or_default();
                let regions = inline_change_regions(&rows);
                let target_indexes =
                    representative_target_indexes(regions.len(), MAX_AI_REVIEW_TARGETS_PER_CHANGE);
                let omitted_targets = regions.len().saturating_sub(target_indexes.len());
                let target_indexes = if target_indexes.is_empty() {
                    vec![0]
                } else {
                    target_indexes
                };
                let targets = target_indexes
                    .into_iter()
                    .map(|index| {
                        let diff_row = regions.get(index).map_or(0, |region| region.first);
                        let mut id = stable_target_id(&file.path, &patch, index);
                        if !used_ids.insert(id.clone()) {
                            id.push(match file.area {
                                ChangeArea::Staged => 's',
                                ChangeArea::Unstaged => 'u',
                            });
                            used_ids.insert(id.clone());
                        }
                        ReviewTarget {
                            id,
                            file: file.clone(),
                            diff_row,
                            location: rows.get(diff_row).and_then(|row| row.number).map_or_else(
                                || format!("change at diff row {}", diff_row + 1),
                                |line| format!("change near line {line}"),
                            ),
                        }
                    })
                    .collect();
                ReviewChange {
                    file,
                    old_path,
                    kind,
                    patch,
                    targets,
                    omitted_targets,
                }
            })
            .collect();
        Some(Self {
            expected_head: snapshot.head.clone(),
            changes,
        })
    }

    #[must_use]
    pub fn rebind_staging(&self, snapshot: &RepositorySnapshot) -> Option<Self> {
        let current = Self::from_snapshot(snapshot)?;
        (self.expected_head == current.expected_head
            && semantic_changes(&self.changes) == semantic_changes(&current.changes))
        .then_some(current)
    }

    #[must_use]
    pub fn change_count(&self) -> usize {
        self.changes.len()
    }

    #[must_use]
    pub fn file_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        for change in &self.changes {
            if !paths.contains(&change.file.path) {
                paths.push(change.file.path.clone());
            }
        }
        paths
    }

    #[must_use]
    pub(crate) fn target(&self, id: &str) -> Option<&ReviewTarget> {
        self.changes
            .iter()
            .flat_map(|change| &change.targets)
            .find(|target| target.id == id)
    }

    #[must_use]
    fn contains_target(&self, id: &str) -> bool {
        self.target(id).is_some()
    }

    #[cfg(test)]
    pub(crate) fn first_target_id(&self) -> &str {
        &self.changes[0].targets[0].id
    }

    #[cfg(test)]
    pub(crate) fn target_ids(&self) -> Vec<String> {
        self.changes
            .iter()
            .flat_map(|change| &change.targets)
            .map(|target| target.id.clone())
            .collect()
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
            if context
                .len()
                .saturating_add(header.len() + footer.len() + ending.len())
                > budget
            {
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
                || !self.contains_target(&stop.target_id)
                || !primary.insert(&stop.target_id)
            {
                return None;
            }
        }
        Some(ReviewResult { overview, stops })
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
        "<change origin=\"{origin}\" path=\"{}\"{old_path} kind=\"{:?}\">\n<targets>\n",
        escaped(&change.file.path.to_string_lossy()),
        change.kind
    );
    for target in &change.targets {
        writeln!(
            header,
            "<target id=\"{}\" location=\"{}\" />",
            target.id,
            escaped(&target.location)
        )
        .expect("writing to a String cannot fail");
    }
    if change.omitted_targets > 0 {
        writeln!(
            header,
            "<omitted-targets count=\"{}\" />",
            change.omitted_targets
        )
        .expect("writing to a String cannot fail");
    }
    header.push_str("</targets>\n<patch>\n");
    header
}

fn semantic_changes(
    changes: &[ReviewChange],
) -> Vec<(PathBuf, Option<PathBuf>, ChangeKind, String)> {
    let mut changes = changes
        .iter()
        .map(|change| {
            (
                change.file.path.clone(),
                change.old_path.clone(),
                change.kind,
                change.patch.clone(),
            )
        })
        .collect::<Vec<_>>();
    changes.sort_by(|left, right| left.0.cmp(&right.0).then(left.3.cmp(&right.3)));
    changes
}

fn stable_target_id(file_path: &std::path::Path, diff_text: &str, index: usize) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in file_path
        .to_string_lossy()
        .as_bytes()
        .iter()
        .chain(diff_text.as_bytes())
        .chain(index.to_le_bytes().iter())
    {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("T{hash:016x}")
}

fn representative_target_indexes(count: usize, maximum: usize) -> Vec<usize> {
    if count <= maximum {
        return (0..count).collect();
    }
    let prefix = maximum.div_ceil(2);
    let suffix = maximum / 2;
    (0..prefix).chain(count - suffix..count).collect()
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
    fn assigns_distinct_stable_ids_to_staged_and_unstaged_targets() {
        let request = ReviewRequest::from_snapshot(&snapshot(
            "@@ -1 +1 @@\n-old\n+new\n@@ -4 +4 @@\n-a\n+b\n".to_owned(),
        ))
        .unwrap();
        let context = request.prompt_context("repo");
        assert_eq!(context.matches("<target id=").count(), 4);
        assert!(context.contains("origin=\"staged\""));
        assert!(context.contains("origin=\"unstaged\""));
    }

    #[test]
    fn whole_file_patches_target_each_changed_region() {
        let patch = concat!(
            "@@ -1,9 +1,9 @@\n",
            " one\n",
            "-old first\n",
            "+new first\n",
            " three\n",
            " four\n",
            " five\n",
            " six\n",
            " seven\n",
            "-old last\n",
            "+new last\n",
            " nine\n",
        );
        let request = ReviewRequest::from_snapshot(&snapshot(patch.to_owned())).unwrap();

        assert_eq!(request.changes[0].targets.len(), 2);
        assert!(request.changes[0].targets[0].diff_row < request.changes[0].targets[1].diff_row);
        assert_eq!(request.changes[0].targets[0].location, "change near line 2");
        assert_eq!(request.changes[0].targets[1].location, "change near line 8");
    }

    #[test]
    fn target_manifest_keeps_bounded_prefix_and_suffix_candidates() {
        let indexes = representative_target_indexes(100, 8);

        assert_eq!(indexes, vec![0, 1, 2, 3, 96, 97, 98, 99]);
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
    fn rejects_an_invented_target_id() {
        let request =
            ReviewRequest::from_snapshot(&snapshot("@@ -1 +1 @@\n-old\n+new\n".to_owned()))
                .unwrap();
        assert!(
            request
                .validate_review(
                    vec!["Overview".to_owned()],
                    vec![ReviewStop {
                        title: "Stop".to_owned(),
                        category: AttentionCategory::Behavior,
                        reason: "Reason".to_owned(),
                        target_id: "T9999".to_owned(),
                    }],
                )
                .is_none()
        );
    }

    #[test]
    fn changes_keep_stable_path_order() {
        let mut snapshot = snapshot("@@ -1 +1 @@\n-old\n+new\n".to_owned());
        snapshot.files.push(FileState {
            path: "src/second.rs".into(),
            old_path: None,
            kind: ChangeKind::Added,
            staged: Some(FileDiff {
                text: "@@ -0,0 +1 @@\n+second\n".to_owned(),
            }),
            unstaged: None,
        });
        let request = ReviewRequest::from_snapshot(&snapshot).unwrap();
        assert_eq!(
            request.file_paths(),
            vec![PathBuf::from("src/lib.rs"), PathBuf::from("src/second.rs")]
        );
        assert_eq!(request.change_count(), 3);
    }
}
