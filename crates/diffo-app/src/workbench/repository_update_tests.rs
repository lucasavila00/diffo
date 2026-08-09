use std::time::{Duration, Instant};

use diffo_core::{
    ApplicationCommandId, ChangeKind, FailureKind, FileDiff, FileState, OperationFailure,
    OperationResult, RepositoryAction, RepositorySnapshot, RepositoryUpdate, RepositoryUpdateKind,
    SyncPlan, SyncProgress,
};

use super::{ToastKind, Workbench};

#[test]
fn sync_progress_shows_the_selected_plan_and_concrete_git_step() {
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    let id = workbench.commands.enqueue(RepositoryAction::Sync);
    let _ = workbench.take_application_command(Instant::now());
    let plan = SyncPlan {
        branch: "main".to_owned(),
        upstream: "origin/main".to_owned(),
        local_only: 2,
        upstream_only: 3,
        establish_upstream: false,
    };

    workbench.accept_sync_progress(id, SyncProgress::Plan(plan));

    assert_eq!(
        workbench.toasts.as_slice()[0].title,
        "origin/main has 3 upstream-only commits. main has 2 local-only commits. Plan: rebase 2 commits onto origin/main, then push."
    );
    workbench.accept_sync_progress(id, SyncProgress::Rebasing { commits: 2 });
    assert_eq!(
        workbench
            .commands
            .active()
            .and_then(|command| command.phase.as_deref()),
        Some("Rebasing 2 commits")
    );
}

#[test]
fn first_sync_progress_and_completion_use_the_selected_remote_action() {
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    let action = RepositoryAction::SyncToRemote("origin".to_owned());
    assert_eq!(
        workbench.diff.model.start_repository_action(action.clone()),
        Some(action.clone())
    );
    let id = workbench.commands.enqueue(action.clone());
    let _ = workbench.take_application_command(Instant::now());
    let plan = SyncPlan {
        branch: "topic".to_owned(),
        upstream: "origin/topic".to_owned(),
        local_only: 1,
        upstream_only: 0,
        establish_upstream: true,
    };

    workbench.accept_sync_progress(id, SyncProgress::Plan(plan.clone()));
    assert!(
        workbench
            .commands
            .active()
            .unwrap()
            .phase
            .as_deref()
            .unwrap()
            .contains("set origin/topic as upstream")
    );

    workbench.operation_completed(
        id,
        action,
        OperationResult::Sync {
            plan: Box::new(plan),
        },
        RepositorySnapshot::default(),
    );
    assert_eq!(workbench.diff.model.network_operation(), None);
    assert_eq!(
        workbench.toasts.as_slice()[0].title,
        "Pushed topic. Set upstream to origin/topic."
    );
}

#[test]
fn command_progress_is_hidden_at_149_ms_and_visible_at_150_ms() {
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    workbench.commands.enqueue(RepositoryAction::Fetch);
    let started = Instant::now();
    let _ = workbench
        .take_application_command(started)
        .expect("fetch command starts");

    workbench.tick(started + Duration::from_millis(149));
    assert!(!workbench.command_progress.is_visible());
    workbench.tick(started + Duration::from_millis(150));
    assert!(workbench.command_progress.is_visible());
}

#[test]
fn fast_stage_completion_never_reveals_progress_or_creates_a_toast() {
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    let id = workbench.commands.enqueue(RepositoryAction::StageAll);
    let started = Instant::now();
    let _ = workbench
        .take_application_command(started)
        .expect("stage command starts");
    let snapshot = RepositorySnapshot {
        files: vec![FileState {
            path: "staged.txt".into(),
            old_path: None,
            kind: ChangeKind::Modified,
            staged: Some(FileDiff {
                text: "staged".to_owned(),
            }),
            unstaged: None,
        }],
        ..RepositorySnapshot::default()
    };

    assert!(workbench.accept_repository_update(RepositoryUpdate {
        generation: 1,
        kind: RepositoryUpdateKind::CommandCompleted {
            command_id: id,
            action: RepositoryAction::StageAll,
            result: OperationResult::Stage,
            snapshot: snapshot.clone(),
        },
    }));
    workbench.tick(started + Duration::from_millis(150));

    assert_eq!(workbench.diff.model.snapshot, snapshot);
    assert!(!workbench.command_progress.is_visible());
    assert!(workbench.commands.active().is_none());
    assert!(workbench.toasts.as_slice().is_empty());
}

#[test]
fn cancelled_command_installs_the_post_operation_snapshot() {
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    let id = workbench.commands.enqueue(RepositoryAction::Sync);
    let _ = workbench
        .take_application_command(Instant::now())
        .expect("sync command starts");
    let snapshot = RepositorySnapshot {
        files: vec![FileState {
            path: "fetched.txt".into(),
            old_path: None,
            kind: ChangeKind::Modified,
            staged: None,
            unstaged: Some(FileDiff {
                text: "fetched".to_owned(),
            }),
        }],
        ..RepositorySnapshot::default()
    };

    assert!(workbench.accept_repository_update(RepositoryUpdate {
        generation: 1,
        kind: RepositoryUpdateKind::CommandCancelled {
            command_id: id,
            action: RepositoryAction::Sync,
            snapshot: snapshot.clone(),
        },
    }));

    assert_eq!(workbench.diff.model.snapshot, snapshot);
    assert!(workbench.commands.active().is_none());
    assert!(workbench.toasts.as_slice().is_empty());
}

#[test]
fn generations_reject_stale_updates_and_only_matching_command_ids_finish_commands() {
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    let id = workbench.commands.enqueue(RepositoryAction::Fetch);
    let started = Instant::now();
    let _ = workbench
        .take_application_command(started)
        .expect("fetch command starts");
    workbench.tick(started + Duration::from_millis(150));
    let failure = |command_id| RepositoryUpdateKind::CommandFailed {
        command_id,
        failure: OperationFailure {
            action: RepositoryAction::Fetch,
            kind: FailureKind::Network,
            detail: "offline".to_owned(),
        },
        snapshot: None,
    };

    assert!(workbench.accept_repository_update(RepositoryUpdate {
        generation: 2,
        kind: RepositoryUpdateKind::Snapshot(RepositorySnapshot::default()),
    }));
    assert!(!workbench.accept_repository_update(RepositoryUpdate {
        generation: 1,
        kind: failure(id),
    }));
    assert!(workbench.accept_repository_update(RepositoryUpdate {
        generation: 3,
        kind: failure(ApplicationCommandId(99)),
    }));
    assert_eq!(workbench.active_command_id(), Some(id));
    assert!(workbench.command_progress.is_visible());
    assert!(workbench.toasts.as_slice().is_empty());

    assert!(workbench.accept_repository_update(RepositoryUpdate {
        generation: 4,
        kind: failure(id),
    }));
    assert!(workbench.commands.active().is_none());
    assert!(!workbench.command_progress.is_visible());
    assert!(matches!(
        workbench.modal,
        Some(super::Modal::Error(ref error))
            if error.title == "Fetch failed" && error.detail == "offline"
    ));
    assert!(workbench.toasts.as_slice().is_empty());
}

#[test]
fn watcher_snapshots_preserve_toasts_and_visible_command_progress() {
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    workbench.show_toast(ToastKind::Info, "Existing result");
    let toast_id = workbench.toasts.as_slice()[0].id;
    workbench.commands.enqueue(RepositoryAction::Fetch);
    let started = Instant::now();
    let _ = workbench
        .take_application_command(started)
        .expect("fetch command starts");
    workbench.tick(started + Duration::from_millis(150));

    assert!(workbench.accept_repository_update(RepositoryUpdate {
        generation: 1,
        kind: RepositoryUpdateKind::Snapshot(RepositorySnapshot::default()),
    }));

    assert!(workbench.command_progress.is_visible());
    assert!(workbench.has_active_command());
    assert!(
        workbench
            .toasts
            .as_slice()
            .iter()
            .any(|toast| toast.id == toast_id)
    );
}

#[test]
fn watcher_snapshot_after_completion_keeps_the_result_toast() {
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    let id = workbench.commands.enqueue(RepositoryAction::Sync);
    let _ = workbench
        .take_application_command(Instant::now())
        .expect("sync command starts");
    assert!(workbench.accept_repository_update(RepositoryUpdate {
        generation: 1,
        kind: RepositoryUpdateKind::CommandCompleted {
            command_id: id,
            action: RepositoryAction::Sync,
            result: OperationResult::Sync {
                plan: Box::new(SyncPlan {
                    branch: "main".to_owned(),
                    upstream: "origin/main".to_owned(),
                    local_only: 0,
                    upstream_only: 1,
                    establish_upstream: false,
                }),
            },
            snapshot: RepositorySnapshot::default(),
        },
    }));
    let toast_id = workbench.toasts.as_slice()[0].id;

    assert!(workbench.accept_repository_update(RepositoryUpdate {
        generation: 2,
        kind: RepositoryUpdateKind::Snapshot(RepositorySnapshot::default()),
    }));

    assert!(
        workbench
            .toasts
            .as_slice()
            .iter()
            .any(|toast| toast.id == toast_id)
    );
}
