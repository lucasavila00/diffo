use super::{
    AnchorRow, Arc, DiffBlock, DiffDocument, DiffKey, DiffViewMode, Duration,
    HIGHLIGHT_LOOKBEHIND_LINES, HIGHLIGHT_PREFETCH_VIEWPORTS, HighlightCache, HighlightedDiff,
    HunkButtonMetrics, MAX_HIGHLIGHT_BYTES_PER_SIDE, MAX_HIGHLIGHT_FILE_LINES, MAX_SYNC_BYTES,
    MAX_SYNC_LINES, PREPARED_BUFFER_CACHE_SIZE, PrepareCommit, PrepareOutcome, PrepareRequest,
    ProjectionOptions, RenderLine, Renderer, RowKind, ScrollAnchor, ScrollbarMetrics,
    SyntaxHighlighter, TrySendError, env, inline_change_starts, inline_rows_with_options,
    parse_unified_patch, side_by_side_change_starts, side_by_side_rows_with_options, sync_channel,
    thread,
};
use diffo_diff::SideBySideRow;
use diffo_highlight::{HighlightWindowRequest, LineRange};

#[derive(Clone, Copy)]
struct ProjectionHighlightRequest {
    viewport_rows: usize,
    mode: DiffViewMode,
    target_scroll: Option<usize>,
    prefetch_viewports: usize,
}

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
    let options = ProjectionOptions {
        mark_conflicts: request.key.mark_conflicts,
    };
    let inline = if request.mode == DiffViewMode::Inline {
        inline_rows_with_options(&document, options)
    } else {
        Vec::new()
    };
    let inline_changes = inline_change_starts(&inline);
    let side_by_side = if request.mode == DiffViewMode::SideBySide {
        side_by_side_rows_with_options(&document, options)
    } else {
        Vec::new()
    };
    let side_by_side_changes = side_by_side_change_starts(&side_by_side);
    let syntax_highlighted = should_syntax_highlight(&document);
    let (old_range, new_range) = projection_highlight_ranges(
        &inline,
        &inline_changes,
        &side_by_side,
        &side_by_side_changes,
        ProjectionHighlightRequest {
            viewport_rows: request.viewport_rows,
            mode: request.mode,
            target_scroll: request.target_scroll,
            prefetch_viewports: HIGHLIGHT_PREFETCH_VIEWPORTS,
        },
    );
    let highlighted_window = syntax_highlighted.then(|| {
        highlighter.highlight_window(
            &request.key.file.path,
            &document,
            HighlightWindowRequest {
                old: old_range,
                new: new_range,
                lookbehind_lines: HIGHLIGHT_LOOKBEHIND_LINES,
                maximum_bytes_per_side: MAX_HIGHLIGHT_BYTES_PER_SIDE,
            },
        )
    });
    let syntax_styles = highlighted_window
        .as_ref()
        .map_or_else(HighlightedDiff::default, |window| window.styles.clone());
    Some(HighlightCache {
        key: request.key,
        document,
        inline,
        side_by_side,
        inline_changes,
        side_by_side_changes,
        highlighted: syntax_styles,
        syntax_highlighted,
        highlighted_old_coverage: highlighted_window
            .as_ref()
            .and_then(|window| window.old_coverage),
        highlighted_new_coverage: highlighted_window
            .as_ref()
            .and_then(|window| window.new_coverage),
        #[cfg(test)]
        highlighted_lines_processed: highlighted_window.as_ref().map_or(0, |window| {
            window
                .old_lines_processed
                .saturating_add(window.new_lines_processed)
        }),
    })
}

fn projection_highlight_ranges(
    inline: &[RenderLine],
    inline_changes: &[usize],
    side_by_side: &[SideBySideRow],
    side_by_side_changes: &[usize],
    request: ProjectionHighlightRequest,
) -> (Option<LineRange>, Option<LineRange>) {
    let rows = request
        .viewport_rows
        .max(1)
        .saturating_mul(request.prefetch_viewports.max(1));
    let inline_start = request
        .target_scroll
        .filter(|_| request.mode == DiffViewMode::Inline)
        .or_else(|| inline_changes.first().copied())
        .unwrap_or(0);
    let side_start = request
        .target_scroll
        .filter(|_| request.mode == DiffViewMode::SideBySide)
        .or_else(|| side_by_side_changes.first().copied())
        .unwrap_or(0);
    let mut old = None;
    let mut new = None;
    let include_inline = request.target_scroll.is_none() || request.mode == DiffViewMode::Inline;
    for row in inline
        .iter()
        .skip(inline_start)
        .take(rows)
        .filter(|_| include_inline)
    {
        match row.kind {
            RowKind::Removed => include_line(&mut old, row.number),
            RowKind::Added | RowKind::Context | RowKind::Changed | RowKind::Conflict => {
                include_line(&mut new, row.number);
            }
            RowKind::Header | RowKind::Meta => {}
        }
    }
    let include_side = request.target_scroll.is_none() || request.mode == DiffViewMode::SideBySide;
    for row in side_by_side
        .iter()
        .skip(side_start)
        .take(rows)
        .filter(|_| include_side)
    {
        include_line(&mut old, row.old.as_ref().and_then(|line| line.number));
        include_line(&mut new, row.new.as_ref().and_then(|line| line.number));
    }
    (old, new)
}

fn include_line(range: &mut Option<LineRange>, line: Option<u32>) {
    let Some(line) = line else {
        return;
    };
    match range {
        Some(range) => {
            range.start = range.start.min(line);
            range.end = range.end.max(line);
        }
        None => *range = Some(LineRange::new(line, line)),
    }
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
    pub(super) fn guard_vertical_message(
        &mut self,
        message: diffo_app::Message,
        model: &diffo_app::Model,
    ) -> Option<diffo_app::Message> {
        let base = self.pending_scroll.unwrap_or(model.diff_scroll);
        let target = match message {
            diffo_app::Message::SetDiffScroll(target) => target,
            diffo_app::Message::ScrollDiffUp => base.saturating_sub(4),
            diffo_app::Message::ScrollDiffDown => base.saturating_add(4),
            diffo_app::Message::ScrollDiffPageUp(lines) => base.saturating_sub(lines),
            diffo_app::Message::ScrollDiffPageDown(lines) => base.saturating_add(lines),
            diffo_app::Message::ScrollDiffBy(lines) => {
                let magnitude = usize::try_from(lines.unsigned_abs()).unwrap_or(usize::MAX);
                if lines >= 0 {
                    base.saturating_add(magnitude)
                } else {
                    base.saturating_sub(magnitude)
                }
            }
            _ => return Some(message),
        };
        if self.syntax_ready_for_viewport(self.displayed_mode(model.diff_view_mode), target) {
            self.pending_scroll = None;
            Some(message)
        } else {
            self.pending_scroll = Some(target);
            None
        }
    }

    pub(super) fn syntax_ready_for_viewport(&self, mode: DiffViewMode, target: usize) -> bool {
        let Some(cache) = self.highlighted.as_ref() else {
            return false;
        };
        if !cache.syntax_highlighted {
            return true;
        }
        let (old, new) = projection_highlight_ranges(
            &cache.inline,
            &cache.inline_changes,
            &cache.side_by_side,
            &cache.side_by_side_changes,
            ProjectionHighlightRequest {
                viewport_rows: self.diff_viewport_rows,
                mode,
                target_scroll: Some(target),
                prefetch_viewports: 1,
            },
        );
        range_is_covered(cache.highlighted_old_coverage, old)
            && range_is_covered(cache.highlighted_new_coverage, new)
    }

    pub(super) fn prepare_requested(
        &mut self,
        requested: Option<&DiffKey>,
        viewport_rows: usize,
        mode: DiffViewMode,
    ) -> Option<PrepareCommit> {
        let mut matching_outcome = None;
        while let Ok(outcome) = self.prepare_rx.try_recv() {
            self.submitted
                .retain(|job| job != &(outcome.key.clone(), outcome.target_scroll));
            let target_matches =
                outcome.target_scroll.is_none() || self.pending_scroll == outcome.target_scroll;
            if requested == Some(&outcome.key) && target_matches {
                matching_outcome = Some(outcome);
            }
        }
        if let Some(outcome) = matching_outcome {
            let target_scroll = outcome.target_scroll;
            self.install_outcome(outcome);
            if target_scroll.is_some() {
                self.pending_scroll = None;
            }
            return Some(PrepareCommit { target_scroll });
        }
        let Some(requested) = requested else {
            let changed = self.displayed_key().is_some();
            if changed {
                self.highlighted = None;
                self.failed = None;
                self.content_revision = self.content_revision.saturating_add(1);
            }
            self.pending_scroll = None;
            return changed.then_some(PrepareCommit {
                target_scroll: None,
            });
        };
        let target_scroll = self.pending_scroll;
        if self.displayed_key() == Some(requested) && target_scroll.is_none() {
            return None;
        }
        let job = (requested.clone(), target_scroll);
        if target_scroll.is_none()
            && let Some(position) = self
                .prepared_cache
                .iter()
                .position(|cache| cache.key == *requested)
        {
            let cache = self.prepared_cache.remove(position);
            self.install_cache(cache);
            return Some(PrepareCommit {
                target_scroll: None,
            });
        }
        if requested.patch.len() <= MAX_SYNC_BYTES
            && requested.patch.lines().count() <= MAX_SYNC_LINES
        {
            let request = PrepareRequest {
                key: requested.clone(),
                viewport_rows,
                mode,
                target_scroll,
            };
            let outcome = PrepareOutcome {
                key: requested.clone(),
                target_scroll,
                cache: prepare_diff(request, &self.highlighter),
            };
            self.install_outcome(outcome);
            self.pending_scroll = None;
            return Some(PrepareCommit { target_scroll });
        }
        if !self.submitted.contains(&job) {
            let request = PrepareRequest {
                key: requested.clone(),
                viewport_rows,
                mode,
                target_scroll,
            };
            match self.prepare_tx.try_send(request) {
                Ok(()) => self.submitted.push(job),
                Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {}
            }
        }
        None
    }

    pub(super) fn install_outcome(&mut self, outcome: PrepareOutcome) {
        if let Some(cache) = outcome.cache {
            #[cfg(test)]
            if cache.syntax_highlighted {
                self.highlight_computations += 1;
            }
            self.failed = None;
            self.install_cache(cache);
        } else {
            let changed = self.displayed_key() != Some(&outcome.key);
            self.highlighted = None;
            self.failed = Some(outcome.key);
            if changed {
                self.content_revision = self.content_revision.saturating_add(1);
            }
        }
    }

    pub(super) fn install_cache(&mut self, cache: HighlightCache) {
        let changed = self
            .highlighted
            .as_ref()
            .is_none_or(|current| current.key != cache.key);
        if let Some(current) = self.highlighted.replace(cache)
            && self
                .highlighted
                .as_ref()
                .is_some_and(|replacement| replacement.key != current.key)
        {
            self.prepared_cache
                .retain(|cached| cached.key != current.key);
            self.prepared_cache.insert(0, current);
            self.prepared_cache.truncate(PREPARED_BUFFER_CACHE_SIZE);
        }
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

    pub(super) fn displayed_mode(&self, fallback: DiffViewMode) -> DiffViewMode {
        self.displayed_key().map_or(fallback, |key| key.mode)
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

fn range_is_covered(coverage: Option<LineRange>, needed: Option<LineRange>) -> bool {
    needed.is_none_or(|needed| {
        coverage
            .is_some_and(|coverage| coverage.start <= needed.start && coverage.end >= needed.end)
    })
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer {
    #[must_use]
    /// Create a renderer and its background diff worker.
    ///
    /// # Panics
    ///
    /// Panics if the operating system cannot start the worker thread.
    pub fn new() -> Self {
        let highlighter = Arc::new(SyntaxHighlighter::new());
        let worker_highlighter = Arc::clone(&highlighter);
        let (prepare_tx, requests) = sync_channel::<PrepareRequest>(1);
        let (results, prepare_rx) = sync_channel(1);
        let prepare_delay = preparation_delay_from_environment();
        thread::Builder::new()
            .name("diffo-diff-prepare".to_owned())
            .spawn(move || {
                while let Ok(request) = requests.recv() {
                    if !prepare_delay.is_zero() {
                        thread::sleep(prepare_delay);
                    }
                    let key = request.key.clone();
                    let target_scroll = request.target_scroll;
                    let cache = prepare_diff(request, &worker_highlighter);
                    if results
                        .send(PrepareOutcome {
                            key,
                            target_scroll,
                            cache,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .expect("failed to start diff preparation worker");
        Self {
            highlighter,
            highlighted: None,
            prepared_cache: Vec::new(),
            prepare_tx,
            prepare_rx,
            submitted: Vec::new(),
            requested: None,
            pending_scroll: None,
            diff_viewport_rows: 1,
            failed: None,
            scrollbars: ScrollbarMetrics::default(),
            scrollbar_drag: None,
            hunk_buttons: HunkButtonMetrics::default(),
            hovered_hunk_button: None,
            content_revision: 0,
            network_animation_tick: 0,
            #[cfg(test)]
            highlight_computations: 0,
        }
    }
}
