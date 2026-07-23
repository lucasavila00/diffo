use std::time::{SystemTime, UNIX_EPOCH};

use crossterm::event::{Event, MouseButton, MouseEventKind};
use diffo_core::{
    BranchKind, BranchRef, CheckoutTarget, DeleteBranchTarget, HeadState, RepositoryQueryId,
    RepositorySnapshot,
};
use diffo_ui::command_palette::CommandId;
use diffo_ui::search_picker::{SearchItem, SearchPicker, SearchPickerEvent};
use ratatui::{Frame, layout::Rect};

use super::{Message, Modal, ToastKind, Workbench, create_branch::CreateBranchModal};

pub(super) enum CheckoutPickerEvent {
    Consumed,
    Close,
    Checkout(CheckoutTarget),
    CreateBranch(CheckoutTarget),
    DeleteBranch(DeleteBranchTarget),
    Quit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CheckoutPickerPurpose {
    Checkout,
    CreateBranch,
    DeleteBranch,
}

pub(super) struct CheckoutPicker {
    pub(super) query_id: RepositoryQueryId,
    branches: Vec<BranchRef>,
    loaded_at_unix_seconds: i64,
    purpose: CheckoutPickerPurpose,
    picker: SearchPicker<CheckoutIdentity, CheckoutTarget>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CheckoutIdentity {
    kind: BranchKind,
    full_ref: String,
}

impl CheckoutPicker {
    pub(super) fn loading(query_id: RepositoryQueryId) -> Self {
        Self::loading_for(query_id, CheckoutPickerPurpose::Checkout)
    }

    pub(super) fn loading_create_branch(query_id: RepositoryQueryId) -> Self {
        Self::loading_for(query_id, CheckoutPickerPurpose::CreateBranch)
    }

    pub(super) fn loading_delete_branch(query_id: RepositoryQueryId) -> Self {
        Self::loading_for(query_id, CheckoutPickerPurpose::DeleteBranch)
    }

    fn loading_for(query_id: RepositoryQueryId, purpose: CheckoutPickerPurpose) -> Self {
        let title = match purpose {
            CheckoutPickerPurpose::Checkout => "Checkout to",
            CheckoutPickerPurpose::CreateBranch => "Create branch from",
            CheckoutPickerPurpose::DeleteBranch => "Delete branch",
        };
        Self {
            query_id,
            branches: Vec::new(),
            loaded_at_unix_seconds: 0,
            purpose,
            picker: SearchPicker::new(title, "Loading branches..."),
        }
    }

    pub(super) fn install(&mut self, branches: Vec<BranchRef>, snapshot: &RepositorySnapshot) {
        self.install_at(branches, snapshot, current_unix_seconds());
    }

    fn install_at(
        &mut self,
        branches: Vec<BranchRef>,
        snapshot: &RepositorySnapshot,
        now_unix_seconds: i64,
    ) {
        self.branches = branches;
        self.loaded_at_unix_seconds = now_unix_seconds;
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
            .filter_map(|branch| {
                if self.purpose == CheckoutPickerPurpose::DeleteBranch
                    && (branch.kind != BranchKind::Local || current == Some(branch.name.as_str()))
                {
                    return None;
                }
                let enabled = match self.purpose {
                    CheckoutPickerPurpose::CreateBranch | CheckoutPickerPurpose::DeleteBranch => {
                        true
                    }
                    CheckoutPickerPurpose::Checkout => match branch.kind {
                        BranchKind::Local => current != Some(branch.name.as_str()),
                        BranchKind::Remote => tracked_remote != Some(branch.name.as_str()),
                    },
                };
                let aliases = match branch.kind {
                    BranchKind::Local => Vec::new(),
                    BranchKind::Remote => branch
                        .name
                        .split_once('/')
                        .map(|(_, short)| vec![short.to_owned()])
                        .unwrap_or_default(),
                };
                Some(SearchItem {
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
                    preferred_match: None,
                    trailing: relative_commit_age(
                        branch.tip_commit_unix_seconds,
                        self.loaded_at_unix_seconds,
                    ),
                    aliases,
                    enabled,
                })
            })
            .collect();
        self.picker.reconcile_items(items);
    }

    pub(super) fn handle_event(&mut self, event: &Event, area: Rect) -> CheckoutPickerEvent {
        if self.purpose == CheckoutPickerPurpose::DeleteBranch
            && let Event::Mouse(mouse) = event
            && mouse.kind == MouseEventKind::Down(MouseButton::Left)
            && !self.picker.contains(area, mouse.column, mouse.row)
        {
            return CheckoutPickerEvent::Close;
        }
        match self.picker.handle_event(event, area) {
            SearchPickerEvent::Consumed => CheckoutPickerEvent::Consumed,
            SearchPickerEvent::Cancel => CheckoutPickerEvent::Close,
            SearchPickerEvent::Activate(target) => match self.purpose {
                CheckoutPickerPurpose::Checkout => CheckoutPickerEvent::Checkout(target),
                CheckoutPickerPurpose::CreateBranch => CheckoutPickerEvent::CreateBranch(target),
                CheckoutPickerPurpose::DeleteBranch => {
                    let Some(name) = target.full_ref.strip_prefix("refs/heads/") else {
                        return CheckoutPickerEvent::Consumed;
                    };
                    CheckoutPickerEvent::DeleteBranch(DeleteBranchTarget {
                        name: name.to_owned(),
                        full_ref: target.full_ref,
                        object_id: target.object_id,
                        force: false,
                    })
                }
            },
            SearchPickerEvent::Quit => CheckoutPickerEvent::Quit,
        }
    }

    pub(super) fn render(&self, frame: &mut Frame, area: Rect) {
        self.picker.render(frame, area);
    }

    pub(super) fn branches(&self) -> Vec<BranchRef> {
        self.branches.clone()
    }
}

fn current_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
        })
}

fn relative_commit_age(
    tip_commit_unix_seconds: Option<i64>,
    now_unix_seconds: i64,
) -> Option<String> {
    const MINUTE: i64 = 60;
    const HOUR: i64 = 60 * MINUTE;
    const DAY: i64 = 24 * HOUR;
    const MONTH: i64 = 30 * DAY;
    const YEAR: i64 = 365 * DAY;

    let elapsed = now_unix_seconds.saturating_sub(tip_commit_unix_seconds?);
    let (value, suffix) = if elapsed < MINUTE {
        return Some("now".to_owned());
    } else if elapsed < HOUR {
        (elapsed / MINUTE, "m")
    } else if elapsed < DAY {
        (elapsed / HOUR, "h")
    } else if elapsed < MONTH {
        (elapsed / DAY, "d")
    } else if elapsed < YEAR {
        (elapsed / MONTH, "mo")
    } else {
        (elapsed / YEAR, "y")
    };
    Some(format!("{value}{suffix} ago"))
}

impl Workbench {
    pub(super) fn execute_branch_picker_command(&mut self, command: CommandId) -> bool {
        if command == super::CHECKOUT_COMMAND {
            self.open_checkout_picker();
        } else if command == super::delete_branch::DELETE_BRANCH_COMMAND {
            self.open_delete_branch();
        } else {
            return false;
        }
        true
    }

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
        match self.modal.as_mut() {
            Some(Modal::CheckoutPicker(picker)) if picker.query_id == query_id => {
                picker.install(branches, &snapshot);
            }
            Some(Modal::CreateBranch(modal)) if modal.query_id == Some(query_id) => {
                modal.install(branches, snapshot.head);
            }
            _ => {}
        }
    }

    pub fn branches_load_failed(&mut self, query_id: RepositoryQueryId, message: &str) {
        if self.modal.as_ref().is_some_and(|modal| {
            matches!(modal, Modal::CheckoutPicker(picker) if picker.query_id == query_id)
                || matches!(modal, Modal::CreateBranch(create) if create.query_id == Some(query_id))
        }) {
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

    pub(super) fn open_create_branch(&mut self) {
        let query_id = RepositoryQueryId(self.next_query_id);
        self.next_query_id = self.next_query_id.saturating_add(1);
        self.set_modal(Modal::CreateBranch(CreateBranchModal::loading(query_id)));
        self.pending_branch_query = Some(query_id);
    }

    pub(super) fn open_create_branch_from(&mut self) {
        let query_id = RepositoryQueryId(self.next_query_id);
        self.next_query_id = self.next_query_id.saturating_add(1);
        self.set_modal(Modal::CheckoutPicker(
            CheckoutPicker::loading_create_branch(query_id),
        ));
        self.pending_branch_query = Some(query_id);
    }

    pub(super) fn open_delete_branch(&mut self) {
        let query_id = RepositoryQueryId(self.next_query_id);
        self.next_query_id = self.next_query_id.saturating_add(1);
        self.set_modal(Modal::CheckoutPicker(
            CheckoutPicker::loading_delete_branch(query_id),
        ));
        self.pending_branch_query = Some(query_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workbench::{
        Activity, ApplicationAction, CHECKOUT_COMMAND, create_branch::CREATE_BRANCH_FROM_COMMAND,
        delete_branch::DELETE_BRANCH_COMMAND, workbench_areas,
    };
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};
    use diffo_core::{CreateBranchStartPoint, RepositoryAction, UpstreamState};
    use diffo_ui::tool_areas;
    use ratatui::{Terminal, backend::TestBackend};

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
            tip_commit_unix_seconds: None,
        }
    }

    #[test]
    fn formats_fixed_relative_commit_age_boundaries() {
        const DAY: i64 = 24 * 60 * 60;
        let now = 2 * 365 * DAY;

        assert_eq!(relative_commit_age(None, now), None);
        for (tip, expected) in [
            (now + 1, "now"),
            (now - 59, "now"),
            (now - 60, "1m ago"),
            (now - 3_599, "59m ago"),
            (now - 3_600, "1h ago"),
            (now - DAY, "1d ago"),
            (now - 30 * DAY, "1mo ago"),
            (now - 365 * DAY, "1y ago"),
        ] {
            assert_eq!(
                relative_commit_age(Some(tip), now).as_deref(),
                Some(expected)
            );
        }
    }

    #[test]
    fn installs_relative_ages_and_selects_the_first_enabled_branch() {
        let now = 10_000;
        let snapshot = RepositorySnapshot {
            head: HeadState::Named {
                name: "main".to_owned(),
                commit: "aaa".to_owned(),
            },
            upstream: Some(UpstreamState {
                name: "origin/main".to_owned(),
                ahead: 0,
                behind: 0,
                recent_local_commits: Vec::new(),
            }),
            ..RepositorySnapshot::default()
        };
        let mut main = branch(BranchKind::Local, "main", "aaa");
        main.tip_commit_unix_seconds = Some(now);
        let mut tracked = branch(BranchKind::Remote, "origin/main", "aaa");
        tracked.tip_commit_unix_seconds = Some(now);
        let mut topic = branch(BranchKind::Local, "topic", "bbb");
        topic.tip_commit_unix_seconds = Some(now - 12 * 60);
        let mut picker = CheckoutPicker::loading(RepositoryQueryId(1));

        picker.install_at(vec![main, tracked, topic], &snapshot, now);

        assert_eq!(
            picker.picker.selected_identity(),
            Some(&CheckoutIdentity {
                kind: BranchKind::Local,
                full_ref: "refs/heads/topic".to_owned(),
            })
        );
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| picker.render(frame, frame.area()))
            .unwrap();
        let contents = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect::<String>();
        assert!(contents.contains("now"));
        assert!(contents.contains("12m ago"));
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
                recent_local_commits: Vec::new(),
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
            .take_application_command(std::time::Instant::now())
            .expect("checkout queued");
        assert_eq!(
            command.action,
            ApplicationAction::Repository(RepositoryAction::Checkout(Box::new(CheckoutTarget {
                kind: BranchKind::Local,
                full_ref: "refs/heads/topic".to_owned(),
                object_id: "bbb".to_owned(),
            })))
        );
    }

    #[test]
    fn create_branch_from_reuses_picker_and_enables_the_current_branch() {
        let snapshot = RepositorySnapshot {
            head: HeadState::Named {
                name: "main".to_owned(),
                commit: "aaa".to_owned(),
            },
            ..RepositorySnapshot::default()
        };
        let mut workbench = Workbench::new(snapshot);
        let area = Rect::new(0, 0, 100, 30);

        let _ = workbench.execute_palette_command(CREATE_BRANCH_FROM_COMMAND);
        let query_id = workbench.take_branch_query().expect("branch query queued");
        workbench.branches_loaded(
            query_id,
            vec![
                branch(BranchKind::Local, "main", "aaa"),
                branch(BranchKind::Local, "topic", "bbb"),
            ],
        );
        let _ = workbench.handle_event(&key(KeyCode::Enter), area);
        assert!(matches!(workbench.modal, Some(Modal::CreateBranch(_))));
        for character in "new-topic".chars() {
            let _ = workbench.handle_event(&key(KeyCode::Char(character)), area);
        }
        let _ = workbench.handle_event(&key(KeyCode::Enter), area);

        let command = workbench
            .take_application_command(std::time::Instant::now())
            .expect("create branch queued");
        assert!(matches!(
            command.action,
            ApplicationAction::Repository(RepositoryAction::CreateBranch(target))
                if target.name == "new-topic"
                    && matches!(&target.start_point, CreateBranchStartPoint::Branch(
                        CheckoutTarget { full_ref, object_id, .. }
                    ) if full_ref == "refs/heads/main" && object_id == "aaa")
        ));
    }

    #[test]
    fn delete_branch_picker_omits_current_and_remote_branches_and_queues_safe_target() {
        let snapshot = RepositorySnapshot {
            head: HeadState::Named {
                name: "main".to_owned(),
                commit: "aaa".to_owned(),
            },
            ..RepositorySnapshot::default()
        };
        let mut workbench = Workbench::new(snapshot);
        let area = Rect::new(0, 0, 100, 30);

        let _ = workbench.execute_palette_command(DELETE_BRANCH_COMMAND);
        let query_id = workbench.take_branch_query().expect("branch query queued");
        workbench.branches_loaded(
            query_id,
            vec![
                branch(BranchKind::Local, "main", "aaa"),
                branch(BranchKind::Remote, "origin/topic", "bbb"),
                branch(BranchKind::Local, "topic", "bbb"),
            ],
        );
        let Some(Modal::CheckoutPicker(picker)) = workbench.modal.as_ref() else {
            panic!("delete branch picker should be open");
        };
        assert_eq!(
            picker.picker.selected_identity(),
            Some(&CheckoutIdentity {
                kind: BranchKind::Local,
                full_ref: "refs/heads/topic".to_owned(),
            })
        );

        let _ = workbench.handle_event(&key(KeyCode::Enter), area);
        let command = workbench
            .take_application_command(std::time::Instant::now())
            .expect("delete branch queued");
        assert!(matches!(
            command.action,
            ApplicationAction::Repository(RepositoryAction::DeleteBranch(target))
                if target.name == "topic"
                    && target.full_ref == "refs/heads/topic"
                    && target.object_id == "bbb"
                    && !target.force
        ));
    }

    #[test]
    fn delete_branch_picker_allows_local_branches_from_detached_head() {
        let snapshot = RepositorySnapshot {
            head: HeadState::Detached {
                commit: "aaa".to_owned(),
            },
            ..RepositorySnapshot::default()
        };
        let mut picker = CheckoutPicker::loading_delete_branch(RepositoryQueryId(1));

        picker.install(
            vec![
                branch(BranchKind::Local, "main", "aaa"),
                branch(BranchKind::Remote, "origin/main", "aaa"),
            ],
            &snapshot,
        );

        assert_eq!(
            picker.picker.selected_identity(),
            Some(&CheckoutIdentity {
                kind: BranchKind::Local,
                full_ref: "refs/heads/main".to_owned(),
            })
        );
    }

    #[test]
    fn delete_branch_picker_closes_on_outside_click() {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        let area = Rect::new(0, 0, 100, 30);
        let _ = workbench.execute_palette_command(DELETE_BRANCH_COMMAND);

        let _ = workbench.handle_event(
            &Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 0,
                row: 0,
                modifiers: KeyModifiers::NONE,
            }),
            area,
        );

        assert!(workbench.modal.is_none());
        assert!(workbench.take_branch_query().is_none());
    }

    #[test]
    fn delete_branch_command_is_shared_by_every_activity() {
        for activity in [Activity::Diff, Activity::Explorer] {
            let mut workbench = Workbench::new(RepositorySnapshot::default());
            workbench.active = activity;

            let _ = workbench.execute_palette_command(DELETE_BRANCH_COMMAND);

            assert_eq!(workbench.take_branch_query(), Some(RepositoryQueryId(1)));
            assert!(matches!(workbench.modal, Some(Modal::CheckoutPicker(_))));
        }
    }

    #[test]
    fn shared_branch_status_does_not_open_checkout_from_any_activity() {
        let area = Rect::new(0, 0, 100, 30);
        let status = tool_areas(workbench_areas(area).content).status;
        let click = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: status.x.saturating_add(2),
            row: status.y,
            modifiers: KeyModifiers::NONE,
        });
        for activity in [Activity::Diff, Activity::Explorer] {
            let mut workbench = Workbench::new(RepositorySnapshot {
                head: HeadState::Named {
                    name: "main".to_owned(),
                    commit: "aaa".to_owned(),
                },
                ..RepositorySnapshot::default()
            });
            workbench.active = activity;

            let _ = workbench.handle_event(&click, area);

            assert_eq!(workbench.take_branch_query(), None);
            assert!(workbench.modal.is_none());
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
            .take_application_command(std::time::Instant::now())
            .expect("checkout queued");
        assert_eq!(
            command.action,
            ApplicationAction::Repository(RepositoryAction::Checkout(Box::new(CheckoutTarget {
                kind: BranchKind::Local,
                full_ref: "refs/heads/zzz".to_owned(),
                object_id: "new-zzz".to_owned(),
            })))
        );
    }
}
