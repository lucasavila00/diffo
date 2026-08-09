use std::{
    ffi::OsStr,
    path::Path,
    sync::mpsc::Sender,
    time::{Duration, Instant},
};

use diffo_ai_config::{AI_REVIEW_BATCH_CHANGES, MAX_CODEX_RUNTIME_SECONDS};
use diffo_app::review::{ReviewCodexOutcome, ReviewCodexTaskResult, ReviewProgress, ReviewRequest};
use diffo_core::{ApplicationCommandId, CancellationHandle};

use super::{CodexTaskResult, run_review_codex, timeout_message};

pub(super) fn run_review_worker(
    id: ApplicationCommandId,
    request: &ReviewRequest,
    executable: &OsStr,
    repository_root: &Path,
    cancellation: &CancellationHandle,
    sender: &Sender<CodexTaskResult>,
) {
    let batches = request.batches(AI_REVIEW_BATCH_CHANGES);
    let changes = request.change_count();
    let started = Instant::now();
    let mut stops = 0_usize;
    for (index, batch) in batches.iter().enumerate() {
        let change_start = index
            .saturating_mul(AI_REVIEW_BATCH_CHANGES)
            .saturating_add(1);
        let progress = ReviewProgress {
            batch: index + 1,
            batches: batches.len(),
            change_start,
            change_end: change_start
                .saturating_add(batch.change_count())
                .saturating_sub(1),
            changes,
            files: batch.file_paths(),
        };
        if sender
            .send(CodexTaskResult::ReviewProgress(id, progress))
            .is_err()
        {
            break;
        }
        let timeout =
            Duration::from_secs(MAX_CODEX_RUNTIME_SECONDS).saturating_sub(started.elapsed());
        let outcome = if timeout.is_zero() {
            ReviewCodexOutcome::Failed(timeout_message())
        } else {
            run_review_codex(executable, repository_root, batch, cancellation, timeout)
        };
        if let ReviewCodexOutcome::Generated(review) = &outcome {
            stops = stops.saturating_add(review.stops.len());
        }
        let complete = index + 1 == batches.len()
            || stops >= 8
            || !matches!(outcome, ReviewCodexOutcome::Generated(_));
        if sender
            .send(CodexTaskResult::Review(ReviewCodexTaskResult {
                id,
                outcome,
                complete,
            }))
            .is_err()
            || complete
        {
            break;
        }
    }
}
