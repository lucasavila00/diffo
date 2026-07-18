use super::{
    AnchorRow, DiffBlock, DiffDocument, DiffKey, DiffViewMode, Duration, HighlightCache,
    HighlightedDiff, MAX_HIGHLIGHT_FILE_LINES, MAX_SYNC_BYTES, MAX_SYNC_LINES, PrepareOutcome,
    PrepareRequest, ProjectionOptions, RenderLine, Renderer, RowKind, ScrollAnchor,
    SyntaxHighlighter, TrySendError, env, inline_change_starts, inline_rows_with_options,
    parse_unified_patch, side_by_side_change_starts, side_by_side_rows_with_options,
};

impl ScrollAnchor {
    pub(super) fn capture(cache: &HighlightCache, mode: DiffViewMode, first_row: usize) -> Self {
        let row_count = projection_len(cache, mode);
        Self {
            rows: (first_row..row_count)
                .take(16)
                .filter_map(|index| {
                    anchor_row(cache, mode, index).map(|row| (index - first_row, index, row))
                })
                .collect(),
        }
    }

    pub(super) fn resolve(&self, cache: &HighlightCache, mode: DiffViewMode) -> Option<usize> {
        let row_count = projection_len(cache, mode);
        self.rows
            .iter()
            .find_map(|(viewport_offset, old_index, anchor)| {
                (0..row_count)
                    .filter(|index| anchor.matches(cache, mode, *index))
                    .min_by_key(|index| index.abs_diff(*old_index))
                    .map(|index| index.saturating_sub(*viewport_offset))
            })
    }
}

impl AnchorRow {
    pub(super) fn matches(&self, cache: &HighlightCache, mode: DiffViewMode, index: usize) -> bool {
        match (self, mode) {
            (Self::Inline { kind, text }, DiffViewMode::Inline) => cache
                .inline
                .get(index)
                .is_some_and(|row| row.kind == *kind && row.text == *text),
            (Self::SideBySide { old, new }, DiffViewMode::SideBySide) => {
                cache.side_by_side.get(index).is_some_and(|row| {
                    side_line_matches(old.as_ref(), row.old.as_ref())
                        && side_line_matches(new.as_ref(), row.new.as_ref())
                })
            }
            _ => false,
        }
    }
}

pub(super) fn side_line_matches(
    expected: Option<&(RowKind, String)>,
    actual: Option<&RenderLine>,
) -> bool {
    match (expected, actual) {
        (Some((kind, text)), Some(actual)) => actual.kind == *kind && actual.text == *text,
        (None, None) => true,
        _ => false,
    }
}

pub(super) fn projection_len(cache: &HighlightCache, mode: DiffViewMode) -> usize {
    match mode {
        DiffViewMode::Inline => cache.inline.len(),
        DiffViewMode::SideBySide => cache.side_by_side.len(),
    }
}

pub(super) fn first_change(cache: &HighlightCache, mode: DiffViewMode) -> Option<usize> {
    match mode {
        DiffViewMode::Inline => cache.inline_changes.first().copied(),
        DiffViewMode::SideBySide => cache.side_by_side_changes.first().copied(),
    }
}

pub(super) fn anchor_row(
    cache: &HighlightCache,
    mode: DiffViewMode,
    index: usize,
) -> Option<AnchorRow> {
    match mode {
        DiffViewMode::Inline => cache.inline.get(index).map(|row| AnchorRow::Inline {
            kind: row.kind,
            text: row.text.clone(),
        }),
        DiffViewMode::SideBySide => {
            cache
                .side_by_side
                .get(index)
                .map(|row| AnchorRow::SideBySide {
                    old: row.old.as_ref().map(|line| (line.kind, line.text.clone())),
                    new: row.new.as_ref().map(|line| (line.kind, line.text.clone())),
                })
        }
    }
}

pub(super) fn prepare_diff(
    request: PrepareRequest,
    highlighter: &SyntaxHighlighter,
) -> Option<HighlightCache> {
    let document = parse_unified_patch(&request.key.patch).ok()?;
    let syntax_highlighted = should_syntax_highlight(&document);
    let syntax_styles = if syntax_highlighted {
        highlighter.highlight(&request.key.file.path, &document)
    } else {
        HighlightedDiff::default()
    };
    let options = ProjectionOptions {
        mark_conflicts: request.key.mark_conflicts,
    };
    let inline = inline_rows_with_options(&document, options);
    let inline_changes = inline_change_starts(&inline);
    let side_by_side = side_by_side_rows_with_options(&document, options);
    let side_by_side_changes = side_by_side_change_starts(&side_by_side);
    Some(HighlightCache {
        key: request.key,
        document,
        inline,
        side_by_side,
        inline_changes,
        side_by_side_changes,
        highlighted: syntax_styles,
        #[cfg(test)]
        syntax_highlighted,
    })
}

pub(super) fn should_syntax_highlight(document: &DiffDocument) -> bool {
    diff_file_lines(document) < MAX_HIGHLIGHT_FILE_LINES
}

pub(super) fn diff_file_lines(document: &DiffDocument) -> usize {
    let mut maximum = 0;
    let mut include = |line: &diffo_diff::DiffLine| {
        maximum = maximum.max(
            line.old_number
                .into_iter()
                .chain(line.new_number)
                .max()
                .map_or(0, |number| number as usize),
        );
    };
    for block in document.hunks.iter().flat_map(|hunk| &hunk.blocks) {
        match block {
            DiffBlock::Context(lines) => lines.iter().for_each(&mut include),
            DiffBlock::Change { removed, added, .. } => {
                removed.iter().chain(added).for_each(&mut include);
            }
            DiffBlock::Meta(_) => {}
        }
    }
    maximum
}

pub(super) fn preparation_delay_from_environment() -> Duration {
    // Developer/test hook for exercising atomic background transitions in a PTY.
    env::var("DIFFO_E2E_DIFF_PREP_DELAY_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(|milliseconds| Duration::from_millis(milliseconds.min(5_000)))
        .unwrap_or_default()
}

impl Renderer {
    pub(super) fn prepare_requested(&mut self, requested: Option<&DiffKey>) -> bool {
        let mut matching_outcome = None;
        while let Ok(outcome) = self.prepare_rx.try_recv() {
            let outcome_key = match &outcome {
                Ok(cache) => &cache.key,
                Err(key) => key,
            };
            self.submitted.retain(|key| key != outcome_key);
            if requested == Some(outcome_key) {
                matching_outcome = Some(outcome);
            }
        }
        if let Some(outcome) = matching_outcome {
            self.install_outcome(outcome);
            return true;
        }
        let Some(requested) = requested else {
            let changed = self.displayed_key().is_some();
            if changed {
                self.highlighted = None;
                self.failed = None;
                self.content_revision = self.content_revision.saturating_add(1);
            }
            return changed;
        };
        if self.displayed_key() == Some(requested) {
            return false;
        }
        if requested.patch.len() <= MAX_SYNC_BYTES
            && requested.patch.lines().count() <= MAX_SYNC_LINES
        {
            let request = PrepareRequest {
                key: requested.clone(),
            };
            let outcome = prepare_diff(request, &self.highlighter).ok_or_else(|| requested.clone());
            self.install_outcome(outcome);
            return true;
        }
        if !self.submitted.contains(requested) {
            let request = PrepareRequest {
                key: requested.clone(),
            };
            match self.prepare_tx.try_send(request) {
                Ok(()) => self.submitted.push(requested.clone()),
                Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {}
            }
        }
        false
    }

    pub(super) fn install_outcome(&mut self, outcome: PrepareOutcome) {
        match outcome {
            Ok(cache) => {
                #[cfg(test)]
                if cache.syntax_highlighted {
                    self.highlight_computations += 1;
                }
                self.failed = None;
                self.install_cache(cache);
            }
            Err(key) => {
                let changed = self.displayed_key() != Some(&key);
                self.highlighted = None;
                self.failed = Some(key);
                if changed {
                    self.content_revision = self.content_revision.saturating_add(1);
                }
            }
        }
    }

    pub(super) fn install_cache(&mut self, cache: HighlightCache) {
        let changed = self
            .highlighted
            .as_ref()
            .is_none_or(|current| current.key != cache.key);
        self.highlighted = Some(cache);
        if changed {
            self.content_revision = self.content_revision.saturating_add(1);
        }
    }

    pub(super) fn displayed_key(&self) -> Option<&DiffKey> {
        self.highlighted
            .as_ref()
            .map(|cache| &cache.key)
            .or(self.failed.as_ref())
    }

    pub(super) fn displayed_rows(&self, mode: DiffViewMode) -> usize {
        if let Some(cache) = self.highlighted.as_ref() {
            match mode {
                DiffViewMode::Inline => cache.inline.len(),
                DiffViewMode::SideBySide => cache.side_by_side.len(),
            }
        } else if let Some(failed) = self.failed.as_ref() {
            failed.patch.lines().count()
        } else {
            0
        }
    }

    pub(super) fn displayed_columns(
        &self,
        mode: DiffViewMode,
        viewport_columns: usize,
        first_row: usize,
        row_count: usize,
    ) -> usize {
        if mode == DiffViewMode::SideBySide {
            return viewport_columns;
        }
        if let Some(cache) = self.highlighted.as_ref() {
            cache
                .inline
                .iter()
                .skip(first_row)
                .take(row_count)
                .map(|row| row.text.chars().count().saturating_add(7))
                .max()
                .unwrap_or(0)
        } else if let Some(failed) = self.failed.as_ref() {
            failed
                .patch
                .lines()
                .skip(first_row)
                .take(row_count)
                .map(|line| line.chars().count())
                .max()
                .unwrap_or(0)
        } else {
            0
        }
    }

    pub(super) fn change_targets(&self, mode: DiffViewMode) -> &[usize] {
        self.highlighted.as_ref().map_or(&[], |cache| match mode {
            DiffViewMode::Inline => cache.inline_changes.as_slice(),
            DiffViewMode::SideBySide => cache.side_by_side_changes.as_slice(),
        })
    }
}
