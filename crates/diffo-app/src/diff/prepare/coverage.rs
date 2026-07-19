use diffo_highlight::LineRange;

use super::HighlightCache;

const MAX_COVERAGE_WINDOWS: usize = 8;

pub(in crate::diff) fn range_is_covered(coverage: &[LineRange], needed: Option<LineRange>) -> bool {
    needed.is_none_or(|needed| {
        coverage
            .iter()
            .any(|coverage| coverage.start <= needed.start && coverage.end >= needed.end)
    })
}

pub(in crate::diff) fn merge_coverage(coverage: &mut Vec<LineRange>, incoming: Vec<LineRange>) {
    for range in incoming {
        if let Some(existing) = coverage.iter_mut().find(|existing| {
            existing.start <= range.end.saturating_add(1)
                && range.start <= existing.end.saturating_add(1)
        }) {
            existing.start = existing.start.min(range.start);
            existing.end = existing.end.max(range.end);
        } else {
            coverage.push(range);
        }
    }
    if coverage.len() > MAX_COVERAGE_WINDOWS {
        coverage.drain(..coverage.len() - MAX_COVERAGE_WINDOWS);
    }
}

pub(in crate::diff) fn retain_covered_styles(cache: &mut HighlightCache) {
    cache.highlighted.old.retain(|line, _| {
        cache
            .highlighted_old_coverage
            .iter()
            .any(|range| range.contains(*line))
    });
    cache.highlighted.new.retain(|line, _| {
        cache
            .highlighted_new_coverage
            .iter()
            .any(|range| range.contains(*line))
    });
}
