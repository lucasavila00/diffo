use super::{
    Arc, ChangeRegion, ChangeWarningAreas, DiffKey, DiffViewMode, HighlightCache, HighlightedDiff,
    MAX_SYNC_BYTES, MAX_SYNC_LINES, PREPARED_BUFFER_CACHE_SIZE, PrepareCommit, PrepareOutcome,
    PrepareRequest, ProjectionOptions, Renderer, ScrollAnchor, ScrollbarMetrics, Span,
    SyntaxHighlighter, ViewportTransition, channel, inline_change_regions,
    inline_rows_with_options, parse_unified_patch, side_by_side_change_regions,
    side_by_side_rows_with_options, sync_channel, terminal_safe_text, thread,
};
use diffo_diff::DiffDocument;
use diffo_highlight::LineRange;
use diffo_ui::text_view::{SyntaxCoverage, centered_window};

mod anchor;
mod hunk;
mod hunk_syntax;
pub(in crate::diff) mod state;
mod syntax;
use anchor::first_change;
use hunk::{aggregate_hunk_rows, hunk_change_regions};
use hunk_syntax::highlight_aggregate;
use syntax::{ProjectionHighlightRequest, highlight_visible_window, projection_highlight_ranges};
pub(in crate::diff) use syntax::{diff_file_lines, should_syntax_highlight};

pub(in crate::diff) fn prepare_diff(
    request: &PrepareRequest,
    highlighter: &SyntaxHighlighter,
) -> Option<HighlightCache> {
    let mode = request.key.mode;
    debug_assert_eq!(request.mode, mode);
    let (mut cache, aggregate_documents) = prepare_rows(request)?;
    let highlight_request = ProjectionHighlightRequest {
        viewport_rows: request.viewport_rows,
        mode,
        target_scroll: request.target_scroll,
        prefetch_viewports: request.prefetch_viewports,
    };
    let file_syntax_eligible =
        request.key.selection.file_path().is_some() && should_syntax_highlight(&cache.document);
    let (old_range, new_range) = projection_highlight_ranges(
        &cache.inline,
        &cache.inline_changes,
        &cache.side_by_side,
        &cache.side_by_side_changes,
        &cache.hunk,
        &cache.hunk_changes,
        highlight_request,
    );
    let aggregate_syntax = request
        .key
        .hunk_segments
        .as_deref()
        .zip(aggregate_documents.as_deref())
        .map(|(segments, documents)| {
            highlight_aggregate(
                highlighter,
                segments,
                documents,
                &cache.hunk,
                &cache.hunk_changes,
                highlight_request,
            )
        });
    let syntax_highlighted = aggregate_syntax
        .as_ref()
        .map_or(file_syntax_eligible, |syntax| syntax.enabled);
    let highlighted_window = aggregate_syntax
        .is_none()
        .then(|| {
            highlight_visible_window(
                highlighter,
                &request.key,
                &cache.document,
                syntax_highlighted,
                old_range,
                new_range,
            )
        })
        .flatten();
    cache.highlighted = highlighted_window
        .as_ref()
        .map_or_else(HighlightedDiff::default, |window| window.styles.clone());
    cache.hunk_highlighted = aggregate_syntax
        .as_ref()
        .map_or_else(Vec::new, |syntax| syntax.styles.clone());
    cache.syntax_highlighted = syntax_highlighted;
    cache.highlighted_old_coverage = highlighted_window
        .as_ref()
        .and_then(|window| window.old_coverage)
        .into_iter()
        .collect();
    cache.highlighted_new_coverage = highlighted_window
        .as_ref()
        .and_then(|window| window.new_coverage)
        .into_iter()
        .collect();
    if let Some(syntax) = aggregate_syntax {
        cache.highlighted_hunk_coverage = syntax.row_coverage;
        cache.hunk_old_coverage = syntax.old_coverage;
        cache.hunk_new_coverage = syntax.new_coverage;
        #[cfg(test)]
        {
            cache.highlighted_lines_processed = syntax.lines_processed;
        }
    } else {
        #[cfg(test)]
        {
            cache.highlighted_lines_processed = highlighted_window.as_ref().map_or(0, |window| {
                window
                    .old_lines_processed
                    .saturating_add(window.new_lines_processed)
            });
        }
    }
    Some(cache)
}

fn prepare_rows(request: &PrepareRequest) -> Option<(HighlightCache, Option<Vec<DiffDocument>>)> {
    let mode = request.key.mode;
    let (document, hunk, documents, hunk_targets) = if mode == DiffViewMode::Hunk {
        let aggregate = aggregate_hunk_rows(request.key.hunk_segments.as_deref()?)?;
        (
            aggregate.document,
            aggregate.rows,
            Some(aggregate.documents),
            aggregate.targets,
        )
    } else {
        (
            parse_unified_patch(&request.key.patch).ok()?,
            Vec::new(),
            None,
            Vec::new(),
        )
    };
    let options = ProjectionOptions {
        mark_conflicts: request.key.mark_conflicts,
    };
    let inline = if mode == DiffViewMode::Inline {
        inline_rows_with_options(&document, options)
    } else {
        Vec::new()
    };
    let side_by_side = if mode == DiffViewMode::SideBySide {
        side_by_side_rows_with_options(&document, options)
    } else {
        Vec::new()
    };
    let cache = HighlightCache {
        key: request.key.clone(),
        document,
        inline_changes: inline_change_regions(&inline),
        side_by_side_changes: side_by_side_change_regions(&side_by_side),
        hunk_changes: hunk_change_regions(&hunk),
        inline,
        side_by_side,
        hunk,
        hunk_targets,
        highlighted: HighlightedDiff::default(),
        hunk_highlighted: Vec::new(),
        syntax_highlighted: false,
        highlighted_old_coverage: SyntaxCoverage::default(),
        highlighted_new_coverage: SyntaxCoverage::default(),
        highlighted_hunk_coverage: SyntaxCoverage::default(),
        hunk_old_coverage: Vec::new(),
        hunk_new_coverage: Vec::new(),
        #[cfg(test)]
        highlighted_lines_processed: 0,
    };
    Some((cache, documents))
}

impl Renderer {
    pub(in crate::diff) fn navigation_preparation_target(
        &self,
        requested: Option<&DiffKey>,
        mode: DiffViewMode,
    ) -> Option<usize> {
        self.vertical_scroll.requested().filter(|target| {
            requested != self.displayed_key()
                || !self.syntax_ready_for_viewport(self.displayed_mode(mode), *target)
        })
    }

    pub(in crate::diff) fn commit_ready_navigation(
        &mut self,
        requested: Option<&DiffKey>,
        mode: DiffViewMode,
        horizontal: usize,
    ) -> Option<ViewportTransition> {
        let target = self.vertical_scroll.requested()?;
        if requested != self.displayed_key()
            || !self.syntax_ready_for_viewport(self.displayed_mode(mode), target)
        {
            return None;
        }
        let _ = self.vertical_scroll.take_ready(true);
        Some(ViewportTransition {
            vertical: target,
            horizontal,
        })
    }

    pub(in crate::diff) fn document_viewport_transition(
        &self,
        before: Option<&DiffKey>,
        after: Option<&DiffKey>,
        anchor: Option<&ScrollAnchor>,
        horizontal: usize,
    ) -> ViewportTransition {
        let same_selection = before
            .zip(after)
            .is_some_and(|(before, after)| before.selection == after.selection);
        let same_mode = before
            .zip(after)
            .is_some_and(|(before, after)| before.mode == after.mode);
        let vertical = if same_selection && same_mode {
            self.highlighted.as_ref().and_then(|cache| {
                anchor
                    .and_then(|anchor| anchor.resolve(cache, cache.key.mode))
                    .or_else(|| first_change(cache, cache.key.mode))
            })
        } else if same_selection {
            Some(0)
        } else {
            self.highlighted
                .as_ref()
                .and_then(|cache| first_change(cache, cache.key.mode))
        }
        .unwrap_or(0);
        ViewportTransition {
            vertical,
            horizontal: if same_selection && same_mode {
                horizontal
            } else {
                0
            },
        }
    }

    pub(in crate::diff) fn syntax_target(
        &self,
        requested: Option<&DiffKey>,
        mode: DiffViewMode,
        scroll: usize,
    ) -> Option<usize> {
        (requested == self.displayed_key()
            && !self.syntax_ready_for_viewport(self.displayed_mode(mode), scroll))
        .then_some(scroll)
    }

    pub(in crate::diff) fn syntax_ready_for_viewport(
        &self,
        mode: DiffViewMode,
        target: usize,
    ) -> bool {
        let Some(cache) = self.highlighted.as_ref() else {
            return false;
        };
        if !cache.syntax_highlighted {
            return true;
        }
        if mode == DiffViewMode::Hunk && cache.key.hunk_segments.is_some() {
            let window = centered_window(target, cache.hunk.len(), self.diff_viewport_rows, 1);
            let needed = (!window.is_empty()).then(|| {
                LineRange::new(
                    u32::try_from(window.start).unwrap_or(u32::MAX),
                    u32::try_from(window.end.saturating_sub(1)).unwrap_or(u32::MAX),
                )
            });
            return cache.highlighted_hunk_coverage.covers(needed);
        }
        let (old, new) = projection_highlight_ranges(
            &cache.inline,
            &cache.inline_changes,
            &cache.side_by_side,
            &cache.side_by_side_changes,
            &cache.hunk,
            &cache.hunk_changes,
            ProjectionHighlightRequest {
                viewport_rows: self.diff_viewport_rows,
                mode,
                target_scroll: Some(target),
                prefetch_viewports: 1,
            },
        );
        cache.highlighted_old_coverage.covers(old) && cache.highlighted_new_coverage.covers(new)
    }

    pub(in crate::diff) fn prepare_requested(
        &mut self,
        requested: Option<&DiffKey>,
        viewport_rows: usize,
        mode: DiffViewMode,
        target_scroll: Option<usize>,
        prefetch_viewports: usize,
    ) -> Option<PrepareCommit> {
        let mut installed_target = None;
        while let Ok(outcome) = self.prepare_rx.try_recv() {
            if let Some(commit) = self.accept_prepared_outcome(requested, outcome) {
                installed_target = Some(commit.target_scroll);
            }
        }
        if let Some(target_scroll) = installed_target {
            return Some(PrepareCommit { target_scroll });
        }
        let Some(requested) = requested else {
            let changed = self.displayed_key().is_some();
            if changed {
                self.highlighted = None;
                self.failed = None;
                self.content_revision = self.content_revision.saturating_add(1);
            }
            return changed.then_some(PrepareCommit {
                target_scroll: None,
            });
        };
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
        if requested.workload_bytes() <= MAX_SYNC_BYTES
            && requested.workload_lines() <= MAX_SYNC_LINES
        {
            let request = PrepareRequest {
                key: requested.clone(),
                viewport_rows,
                mode,
                target_scroll,
                prefetch_viewports,
            };
            let outcome = PrepareOutcome {
                key: requested.clone(),
                target_scroll,
                cache: prepare_diff(&request, &self.highlighter),
            };
            self.install_outcome(outcome);
            return Some(PrepareCommit { target_scroll });
        }
        if !self.submitted.contains(&job) {
            let request = PrepareRequest {
                key: requested.clone(),
                viewport_rows,
                mode,
                target_scroll,
                prefetch_viewports,
            };
            if self.prepare_tx.send(request).is_ok() {
                self.submitted.clear();
                self.submitted.push(job);
            }
        }
        None
    }

    pub(in crate::diff) fn accept_prepared_outcome(
        &mut self,
        requested: Option<&DiffKey>,
        outcome: PrepareOutcome,
    ) -> Option<PrepareCommit> {
        self.submitted
            .retain(|job| job != &(outcome.key.clone(), outcome.target_scroll));
        if requested != Some(&outcome.key) {
            return None;
        }
        let target_scroll = outcome.target_scroll;
        self.install_outcome(outcome);
        Some(PrepareCommit { target_scroll })
    }

    pub(in crate::diff) fn install_outcome(&mut self, outcome: PrepareOutcome) {
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

    pub(in crate::diff) fn install_cache(&mut self, mut cache: HighlightCache) {
        if let Some(current) = self
            .highlighted
            .as_mut()
            .filter(|current| current.key == cache.key)
        {
            current.highlighted.old.append(&mut cache.highlighted.old);
            current.highlighted.new.append(&mut cache.highlighted.new);
            for (index, mut highlighted) in cache.hunk_highlighted.into_iter().enumerate() {
                let Some(current_highlighted) = current.hunk_highlighted.get_mut(index) else {
                    continue;
                };
                current_highlighted.old.append(&mut highlighted.old);
                current_highlighted.new.append(&mut highlighted.new);
                if let Some(coverage) = current.hunk_old_coverage.get_mut(index) {
                    coverage.merge(
                        cache
                            .hunk_old_coverage
                            .get(index)
                            .into_iter()
                            .flat_map(|coverage| coverage.iter().copied()),
                    );
                    coverage.retain_styles(&mut current_highlighted.old);
                }
                if let Some(coverage) = current.hunk_new_coverage.get_mut(index) {
                    coverage.merge(
                        cache
                            .hunk_new_coverage
                            .get(index)
                            .into_iter()
                            .flat_map(|coverage| coverage.iter().copied()),
                    );
                    coverage.retain_styles(&mut current_highlighted.new);
                }
            }
            current
                .highlighted_hunk_coverage
                .merge(cache.highlighted_hunk_coverage.iter().copied());
            current
                .highlighted_old_coverage
                .merge(cache.highlighted_old_coverage.iter().copied());
            current
                .highlighted_new_coverage
                .merge(cache.highlighted_new_coverage.iter().copied());
            current
                .highlighted_old_coverage
                .retain_styles(&mut current.highlighted.old);
            current
                .highlighted_new_coverage
                .retain_styles(&mut current.highlighted.new);
            return;
        }
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

    pub(in crate::diff) fn displayed_key(&self) -> Option<&DiffKey> {
        self.highlighted
            .as_ref()
            .map(|cache| &cache.key)
            .or(self.failed.as_ref())
    }

    pub(in crate::diff) fn displayed_mode(&self, fallback: DiffViewMode) -> DiffViewMode {
        self.displayed_key().map_or(fallback, |key| key.mode)
    }

    pub(in crate::diff) fn displayed_rows(&self, mode: DiffViewMode) -> usize {
        if let Some(cache) = self.highlighted.as_ref() {
            match mode {
                DiffViewMode::Inline => cache.inline.len(),
                DiffViewMode::SideBySide => cache.side_by_side.len(),
                DiffViewMode::Hunk => cache.hunk.len(),
            }
        } else if let Some(failed) = self.failed.as_ref() {
            failed.patch.lines().count()
        } else {
            0
        }
    }

    pub(in crate::diff) fn displayed_columns(
        &self,
        mode: DiffViewMode,
        first_row: usize,
        row_count: usize,
    ) -> usize {
        if let Some(cache) = self.highlighted.as_ref() {
            match mode {
                DiffViewMode::Inline => cache
                    .inline
                    .iter()
                    .skip(first_row)
                    .take(row_count)
                    .map(|row| {
                        Span::raw(terminal_safe_text(&row.text))
                            .width()
                            .saturating_add(7)
                    })
                    .max()
                    .unwrap_or(0),
                DiffViewMode::SideBySide => cache
                    .side_by_side
                    .iter()
                    .skip(first_row)
                    .take(row_count)
                    .flat_map(|row| [row.old.as_ref(), row.new.as_ref()])
                    .flatten()
                    .map(|line| Span::raw(terminal_safe_text(&line.text)).width())
                    .max()
                    .unwrap_or(0),
                DiffViewMode::Hunk => cache
                    .hunk
                    .iter()
                    .skip(first_row)
                    .take(row_count)
                    .map(|row| {
                        Span::raw(terminal_safe_text(&row.text))
                            .width()
                            .saturating_add(usize::from(row.prefix.is_some()))
                    })
                    .max()
                    .unwrap_or(0),
            }
        } else if let Some(failed) = self.failed.as_ref() {
            failed
                .patch
                .lines()
                .skip(first_row)
                .take(row_count)
                .map(|line| Span::raw(terminal_safe_text(line)).width())
                .max()
                .unwrap_or(0)
        } else {
            0
        }
    }

    pub(in crate::diff) fn change_targets(&self, mode: DiffViewMode) -> &[ChangeRegion] {
        self.highlighted.as_ref().map_or(&[], |cache| match mode {
            DiffViewMode::Inline => cache.inline_changes.as_slice(),
            DiffViewMode::SideBySide => cache.side_by_side_changes.as_slice(),
            DiffViewMode::Hunk => cache.hunk_changes.as_slice(),
        })
    }
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
        let (prepare_tx, requests) = channel::<PrepareRequest>();
        let (results, prepare_rx) = sync_channel(1);
        thread::Builder::new()
            .name("diffo-diff-prepare".to_owned())
            .spawn(move || {
                while let Ok(mut request) = requests.recv() {
                    while let Ok(newer) = requests.try_recv() {
                        request = newer;
                    }
                    let key = request.key.clone();
                    let target_scroll = request.target_scroll;
                    let cache = prepare_diff(&request, &worker_highlighter);
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
            requested_selection: None,
            displayed_selection: None,
            vertical_scroll: diffo_ui::text_view::PreparedVerticalScroll::default(),
            diff_viewport_rows: 1,
            failed: None,
            scrollbars: ScrollbarMetrics::default(),
            scrollbar_drag: None,
            staged_picker: diffo_ui::file_picker::FilePicker::default(),
            unstaged_picker: diffo_ui::file_picker::FilePicker::default(),
            change_warnings: ChangeWarningAreas::default(),
            content_revision: 0,
            #[cfg(test)]
            highlight_computations: 0,
        }
    }
}
