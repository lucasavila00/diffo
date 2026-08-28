use std::sync::Arc;

use diffo_core::ChangeKind;
use ratatui::text::Line;

use super::{
    ChangeArea, FileKey, HighlightCache, Model, ReviewHunkSegment, ReviewHunkSet, ReviewSelection,
};

pub(super) fn worktree_hunks(model: &Model) -> ReviewHunkSet {
    let segments = [ChangeArea::Staged, ChangeArea::Unstaged]
        .into_iter()
        .flat_map(|area| {
            model.snapshot.files.iter().filter_map(move |file| {
                let diff = match area {
                    ChangeArea::Staged => file.staged.as_ref(),
                    ChangeArea::Unstaged => file.unstaged.as_ref(),
                }?;
                Some(ReviewHunkSegment {
                    selection: ReviewSelection::File(FileKey {
                        path: file.path.clone(),
                        area,
                    }),
                    patch: Arc::from(diff.text.as_str()),
                    mark_conflicts: file.kind == ChangeKind::Conflicted,
                })
            })
        })
        .collect::<Vec<_>>();
    ReviewHunkSet {
        id: "worktree".to_owned(),
        title: Line::raw(" Changes "),
        segments: Arc::from(segments),
    }
}

pub(super) fn hunk_focus_target(
    cache: &HighlightCache,
    selection: &ReviewSelection,
) -> Option<usize> {
    let target = cache
        .hunk_targets
        .iter()
        .find_map(|(candidate, row)| (candidate == selection).then_some(*row))?;
    Some(
        cache
            .hunk_changes
            .iter()
            .find(|change| change.first >= target)
            .map_or(target, |change| change.first),
    )
}
