use super::*;

#[test]
fn operation_toasts_render_in_diff_and_explorer() {
    let mut rendered = Vec::new();
    for activity in [Activity::Diff, Activity::Explorer] {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        workbench.active = activity;
        assert_eq!(
            workbench
                .diff
                .model
                .start_repository_action(RepositoryAction::Sync),
            Some(RepositoryAction::Sync)
        );
        let id = workbench.commands.enqueue(RepositoryAction::Sync);
        let _ = workbench.commands.start_next();
        workbench.operation_completed(
            id,
            RepositoryAction::Sync,
            OperationResult::Sync {
                plan: Box::new(SyncPlan {
                    branch: "main".to_owned(),
                    upstream: "origin/main".to_owned(),
                    local_only: 0,
                    upstream_only: 1,
                }),
            },
            RepositorySnapshot::default(),
        );
        assert_eq!(workbench.diff.model.network_operation(), None);
        assert_eq!(
            workbench.toasts.as_slice()[0].title,
            "Fast-forwarded main by 1 commit."
        );
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| workbench.render(frame)).unwrap();
        rendered.push((
            activity,
            buffer_region(terminal.backend().buffer(), Rect::new(55, 25, 44, 3)),
        ));
    }
    insta::assert_debug_snapshot!(rendered);
}
