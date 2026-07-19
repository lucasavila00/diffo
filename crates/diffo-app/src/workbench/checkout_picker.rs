use crossterm::event::{Event, MouseButton, MouseEventKind};
use diffo_core::{
    BranchKind, BranchRef, CheckoutTarget, HeadState, RepositoryQueryId, RepositorySnapshot,
};
use diffo_ui::search_picker::{SearchItem, SearchPicker, SearchPickerEvent};
use diffo_ui::tool_areas;
use ratatui::{Frame, layout::Rect};

use super::{Message, Modal, ToastKind, Workbench};

pub(super) enum CheckoutPickerEvent {
    Consumed,
    Close,
    Checkout(CheckoutTarget),
    Quit,
}

pub(super) struct CheckoutPicker {
    pub(super) query_id: RepositoryQueryId,
    branches: Vec<BranchRef>,
    picker: SearchPicker<CheckoutIdentity, CheckoutTarget>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CheckoutIdentity {
    kind: BranchKind,
    full_ref: String,
}

impl CheckoutPicker {
    pub(super) fn loading(query_id: RepositoryQueryId) -> Self {
        Self {
            query_id,
            branches: Vec::new(),
            picker: SearchPicker::new("Checkout to", "Loading branches..."),
        }
    }

    pub(super) fn install(&mut self, branches: Vec<BranchRef>, snapshot: &RepositorySnapshot) {
        self.branches = branches;
        self.picker.set_empty_message("No matching branches");
        self.refresh(snapshot);
    }

    pub(super) fn refresh(&mut self, snapshot: &RepositorySnapshot) {
        let current = match &snapshot.head {
            HeadState::Named { name, .. } | HeadState::Unborn { name } => Some(name.as_str()),
            HeadState::Detached { .. } => None,
        };
        let tracked_remote = snapshot
            .upstream
            .as_ref()
            .map(|upstream| upstream.name.as_str());
        let items = self
            .branches
            .iter()
            .map(|branch| {
                let enabled = match branch.kind {
                    BranchKind::Local => current != Some(branch.name.as_str()),
                    BranchKind::Remote => tracked_remote != Some(branch.name.as_str()),
                };
                let aliases = match branch.kind {
                    BranchKind::Local => Vec::new(),
                    BranchKind::Remote => branch
                        .name
                        .split_once('/')
                        .map(|(_, short)| vec![short.to_owned()])
                        .unwrap_or_default(),
                };
                SearchItem {
                    identity: CheckoutIdentity {
                        kind: branch.kind,
                        full_ref: branch.full_ref.clone(),
                    },
                    payload: CheckoutTarget {
                        kind: branch.kind,
                        full_ref: branch.full_ref.clone(),
                        object_id: branch.object_id.clone(),
                    },
                    label: branch.name.clone(),
                    aliases,
                    enabled,
                }
            })
            .collect();
        self.picker.reconcile_items(items);
    }

    pub(super) fn handle_event(&mut self, event: &Event, area: Rect) -> CheckoutPickerEvent {
        match self.picker.handle_event(event, area) {
            SearchPickerEvent::Consumed => CheckoutPickerEvent::Consumed,
            SearchPickerEvent::Cancel => CheckoutPickerEvent::Close,
            SearchPickerEvent::Activate(target) => CheckoutPickerEvent::Checkout(target),
            SearchPickerEvent::Quit => CheckoutPickerEvent::Quit,
        }
    }

    pub(super) fn render(&self, frame: &mut Frame, area: Rect) {
        self.picker.render(frame, area);
    }
}

impl Workbench {
    pub fn take_branch_query(&mut self) -> Option<RepositoryQueryId> {
        self.pending_branch_query.take()
    }

    pub fn repository_changed(&mut self, snapshot: RepositorySnapshot) {
        let _ = self.update_diff(Message::SnapshotLoaded(snapshot));
        if let Some(Modal::CheckoutPicker(picker)) = self.modal.as_mut() {
            picker.refresh(&self.diff.model.snapshot);
        }
    }

    pub fn branches_loaded(&mut self, query_id: RepositoryQueryId, branches: Vec<BranchRef>) {
        let snapshot = self.diff.model.snapshot.clone();
        if let Some(Modal::CheckoutPicker(picker)) = self.modal.as_mut().filter(
            |modal| matches!(modal, Modal::CheckoutPicker(picker) if picker.query_id == query_id),
        ) {
            picker.install(branches, &snapshot);
        }
    }

    pub fn branches_load_failed(&mut self, query_id: RepositoryQueryId, message: &str) {
        if self.modal.as_ref().is_some_and(
            |modal| matches!(modal, Modal::CheckoutPicker(picker) if picker.query_id == query_id),
        ) {
            self.close_modal();
            self.show_toast(
                ToastKind::Error,
                format!("Could not load branches: {message}"),
            );
        }
    }

    pub(super) fn open_checkout_picker(&mut self) {
        let query_id = RepositoryQueryId(self.next_query_id);
        self.next_query_id = self.next_query_id.saturating_add(1);
        self.set_modal(Modal::CheckoutPicker(CheckoutPicker::loading(query_id)));
        self.pending_branch_query = Some(query_id);
    }

    pub(super) fn open_checkout_from_head_click(&mut self, event: &Event, content: Rect) -> bool {
        let Event::Mouse(mouse) = event else {
            return false;
        };
        if mouse.kind != MouseEventKind::Down(MouseButton::Left)
            || !crate::diff::head_control_at_position(
                &self.diff.model,
                tool_areas(content).status,
                mouse.column,
                mouse.row,
            )
        {
            return false;
        }
        self.open_checkout_picker();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workbench::{Activity, CHECKOUT_COMMAND, workbench_areas};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};
    use diffo_core::{RepositoryAction, UpstreamState};

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn branch(kind: BranchKind, name: &str, object_id: &str) -> BranchRef {
        let prefix = match kind {
            BranchKind::Local => "refs/heads/",
            BranchKind::Remote => "refs/remotes/",
        };
        BranchRef {
            kind,
            name: name.to_owned(),
            full_ref: format!("{prefix}{name}"),
            object_id: object_id.to_owned(),
        }
    }

    #[test]
    fn rejects_stale_loads_and_enqueues_typed_targets() {
        let snapshot = RepositorySnapshot {
            head: HeadState::Named {
                name: "main".to_owned(),
                commit: "aaa".to_owned(),
            },
            upstream: Some(UpstreamState {
                name: "origin/main".to_owned(),
                ahead: 0,
                behind: 0,
            }),
            ..RepositorySnapshot::default()
        };
        let mut workbench = Workbench::new(snapshot);
        let area = Rect::new(0, 0, 100, 30);

        let _ = workbench.execute_palette_command(CHECKOUT_COMMAND);
        assert_eq!(workbench.take_branch_query(), Some(RepositoryQueryId(1)));
        let _ = workbench.handle_event(&key(KeyCode::Esc), area);
        let _ = workbench.execute_palette_command(CHECKOUT_COMMAND);
        assert_eq!(workbench.take_branch_query(), Some(RepositoryQueryId(2)));

        workbench.branches_loaded(
            RepositoryQueryId(1),
            vec![branch(BranchKind::Local, "stale", "111")],
        );
        let _ = workbench.handle_event(&key(KeyCode::Enter), area);
        assert_eq!(workbench.commands.queued_len(), 0);

        workbench.branches_loaded(
            RepositoryQueryId(2),
            vec![
                branch(BranchKind::Local, "main", "aaa"),
                branch(BranchKind::Local, "topic", "bbb"),
                branch(BranchKind::Remote, "origin/main", "aaa"),
            ],
        );
        let _ = workbench.handle_event(&key(KeyCode::Enter), area);
        let command = workbench
            .take_repository_command(std::time::Instant::now())
            .expect("checkout queued");
        assert_eq!(
            command.action,
            RepositoryAction::Checkout(Box::new(CheckoutTarget {
                kind: BranchKind::Local,
                full_ref: "refs/heads/topic".to_owned(),
                object_id: "bbb".to_owned(),
            }))
        );
    }

    #[test]
    fn shared_branch_status_opens_checkout_from_every_activity() {
        let area = Rect::new(0, 0, 100, 30);
        let status = tool_areas(workbench_areas(area).content).status;
        let click = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: status.x.saturating_add(2),
            row: status.y,
            modifiers: KeyModifiers::NONE,
        });
        for activity in [Activity::Diff, Activity::Explorer, Activity::Search] {
            let mut workbench = Workbench::new(RepositorySnapshot {
                head: HeadState::Named {
                    name: "main".to_owned(),
                    commit: "aaa".to_owned(),
                },
                ..RepositorySnapshot::default()
            });
            workbench.active = activity;

            let _ = workbench.handle_event(&click, area);

            assert_eq!(workbench.take_branch_query(), Some(RepositoryQueryId(1)));
            assert!(matches!(workbench.modal, Some(Modal::CheckoutPicker(_))));
        }
    }

    #[test]
    fn current_load_failure_closes_and_toasts_but_stale_failure_does_nothing() {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        let _ = workbench.execute_palette_command(CHECKOUT_COMMAND);
        let _ = workbench.take_branch_query();
        workbench.branches_load_failed(RepositoryQueryId(9), "stale");
        assert!(matches!(workbench.modal, Some(Modal::CheckoutPicker(_))));
        assert!(workbench.toasts.as_slice().is_empty());

        workbench.branches_load_failed(RepositoryQueryId(1), "broken");

        assert!(workbench.modal.is_none());
        assert_eq!(workbench.toasts.as_slice().len(), 1);
        assert_eq!(workbench.toasts.as_slice()[0].kind, ToastKind::Error);
    }

    #[test]
    fn refresh_between_down_and_enter_keeps_identity_and_uses_newest_object_id() {
        let snapshot = RepositorySnapshot {
            head: HeadState::Named {
                name: "main".to_owned(),
                commit: "aaa".to_owned(),
            },
            ..RepositorySnapshot::default()
        };
        let mut workbench = Workbench::new(snapshot.clone());
        let area = Rect::new(0, 0, 100, 30);
        let _ = workbench.execute_palette_command(CHECKOUT_COMMAND);
        let query_id = workbench.take_branch_query().expect("branch query queued");
        workbench.branches_loaded(
            query_id,
            vec![
                branch(BranchKind::Local, "main", "aaa"),
                branch(BranchKind::Local, "topic", "old-topic"),
                branch(BranchKind::Local, "zzz", "old-zzz"),
            ],
        );
        let _ = workbench.handle_event(&key(KeyCode::Down), area);

        workbench.branches_loaded(
            query_id,
            vec![
                branch(BranchKind::Local, "zzz", "new-zzz"),
                branch(BranchKind::Local, "main", "aaa"),
                branch(BranchKind::Local, "topic", "new-topic"),
            ],
        );
        workbench.repository_changed(snapshot);
        let _ = workbench.handle_event(&key(KeyCode::Enter), area);

        let command = workbench
            .take_repository_command(std::time::Instant::now())
            .expect("checkout queued");
        assert_eq!(
            command.action,
            RepositoryAction::Checkout(Box::new(CheckoutTarget {
                kind: BranchKind::Local,
                full_ref: "refs/heads/zzz".to_owned(),
                object_id: "new-zzz".to_owned(),
            }))
        );
    }
}
