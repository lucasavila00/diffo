use super::{
    Arc, ChangeRegion, DiffBlock, DiffDocument, DiffKey, DiffViewMode, HIGHLIGHT_LOOKBEHIND_LINES,
    HighlightCache, HighlightedDiff, HunkButtonMetrics, MAX_HIGHLIGHT_BYTES_PER_SIDE,
    MAX_HIGHLIGHT_FILE_LINES, MAX_SYNC_BYTES, MAX_SYNC_LINES, Model, PREPARED_BUFFER_CACHE_SIZE,
    PrepareCommit, PrepareOutcome, PrepareRequest, ProjectionOptions, RenderLine, Renderer,
    RowKind, ScrollAnchor, ScrollbarMetrics, Span, SyntaxHighlighter, ViewportTransition, channel,
    inline_change_regions, inline_rows_with_options, parse_unified_patch,
    side_by_side_change_regions, side_by_side_rows_with_options, sync_channel, terminal_safe_text,
    thread,
};
use diffo_diff::SideBySideRow;
use diffo_highlight::{HighlightWindowRequest, LineRange};
use diffo_ui::text_view::{ScrollCommand, ViewportMetrics, centered_window};

mod anchor;
pub(in crate::diff) mod state;
use anchor::first_change;

#[derive(Clone, Copy)]
struct ProjectionHighlightRequest {
    viewport_rows: usize,
    mode: DiffViewMode,
    target_scroll: Option<usize>,
    prefetch_viewports: usize,
}

pub(in crate::diff) fn prepare_diff(
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
    let inline_changes = inline_change_regions(&inline);
    let side_by_side = if request.mode == DiffViewMode::SideBySide {
        side_by_side_rows_with_options(&document, options)
    } else {
        Vec::new()
    };
    let side_by_side_changes = side_by_side_change_regions(&side_by_side);
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
            prefetch_viewports: request.prefetch_viewports,
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
            .and_then(|window| window.old_coverage)
            .into_iter()
            .collect(),
        highlighted_new_coverage: highlighted_window
            .as_ref()
            .and_then(|window| window.new_coverage)
            .into_iter()
            .collect(),
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
    inline_changes: &[ChangeRegion],
    side_by_side: &[SideBySideRow],
    side_by_side_changes: &[ChangeRegion],
    request: ProjectionHighlightRequest,
) -> (Option<LineRange>, Option<LineRange>) {
    let window_viewports = request.prefetch_viewports.max(1);
    debug_assert_eq!(window_viewports % 2, 1);
    let inline_target = request
        .target_scroll
        .filter(|_| request.mode == DiffViewMode::Inline)
        .or_else(|| inline_changes.first().map(|change| change.first))
        .unwrap_or(0);
    let side_target = request
        .target_scroll
        .filter(|_| request.mode == DiffViewMode::SideBySide)
        .or_else(|| side_by_side_changes.first().map(|change| change.first))
        .unwrap_or(0);
    let inline_window = centered_window(
        inline_target,
        inline.len(),
        request.viewport_rows,
        window_viewports,
    );
    let side_window = centered_window(
        side_target,
        side_by_side.len(),
        request.viewport_rows,
        window_viewports,
    );
    let mut old = None;
    let mut new = None;
    let include_inline = request.target_scroll.is_none() || request.mode == DiffViewMode::Inline;
    for row in inline
        .iter()
        .skip(inline_window.start)
        .take(inline_window.len())
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
        .skip(side_window.start)
        .take(side_window.len())
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

pub(in crate::diff) fn should_syntax_highlight(document: &DiffDocument) -> bool {
    diff_file_lines(document) < MAX_HIGHLIGHT_FILE_LINES
}

pub(in crate::diff) fn diff_file_lines(document: &DiffDocument) -> usize {
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
        model: &Model,
    ) -> ViewportTransition {
        let same_file = before
            .zip(after)
            .is_some_and(|(before, after)| before.file == after.file);
        let same_mode = before
            .zip(after)
            .is_some_and(|(before, after)| before.mode == after.mode);
        let vertical = if same_file && same_mode {
            self.highlighted.as_ref().and_then(|cache| {
                anchor
                    .and_then(|anchor| anchor.resolve(cache, cache.key.mode))
                    .or_else(|| first_change(cache, cache.key.mode))
            })
        } else if same_file {
            Some(0)
        } else {
            self.highlighted
                .as_ref()
                .and_then(|cache| first_change(cache, cache.key.mode))
        }
        .unwrap_or(0);
        ViewportTransition {
            vertical,
            horizontal: if same_file && same_mode {
                model.diff_horizontal_scroll
            } else {
                0
            },
        }
    }

    pub(in crate::diff) fn vertical_message(
        &mut self,
        message: crate::diff::Message,
        model: &crate::diff::Model,
    ) -> crate::diff::Message {
        let command = match message {
            crate::diff::Message::SetDiffScroll(target) => ScrollCommand::Vertical(target),
            crate::diff::Message::ScrollDiffUp => {
                ScrollCommand::Lines(-diffo_ui::text_view::LINE_SCROLL_ROWS)
            }
            crate::diff::Message::ScrollDiffDown => {
                ScrollCommand::Lines(diffo_ui::text_view::LINE_SCROLL_ROWS)
            }
            crate::diff::Message::ScrollDiffPageUp(lines) => {
                ScrollCommand::Lines(-i64::try_from(lines).unwrap_or(i64::MAX))
            }
            crate::diff::Message::ScrollDiffPageDown(lines) => {
                ScrollCommand::Lines(i64::try_from(lines).unwrap_or(i64::MAX))
            }
            crate::diff::Message::ScrollDiffVerticalBy(lines) => ScrollCommand::Lines(lines),
            _ => return message,
        };
        let target = self
            .vertical_scroll
            .request(
                command,
                model.diff_scroll,
                ViewportMetrics {
                    maximum_vertical: usize::MAX,
                    ..ViewportMetrics::default()
                },
            )
            .unwrap_or(model.diff_scroll);
        crate::diff::Message::JumpDiffToPosition(target)
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
        if requested.patch.len() <= MAX_SYNC_BYTES
            && requested.patch.lines().count() <= MAX_SYNC_LINES
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
                cache: prepare_diff(request, &self.highlighter),
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
                .map(|row| {
                    Span::raw(terminal_safe_text(&row.text))
                        .width()
                        .saturating_add(7)
                })
                .max()
                .unwrap_or(0)
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
            vertical_scroll: diffo_ui::text_view::PreparedVerticalScroll::default(),
            diff_viewport_rows: 1,
            failed: None,
            scrollbars: ScrollbarMetrics::default(),
            scrollbar_drag: None,
            staged_picker: diffo_ui::file_picker::FilePicker::default(),
            unstaged_picker: diffo_ui::file_picker::FilePicker::default(),
            hunk_buttons: HunkButtonMetrics::default(),
            content_revision: 0,
            #[cfg(test)]
            highlight_computations: 0,
        }
    }
}
