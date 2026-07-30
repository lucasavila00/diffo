use std::time::{SystemTime, UNIX_EPOCH};

use crossterm::event::Event;
use diffo_core::{
    HeadState, MergeRef, MergeRefKind, MergeTarget, RepositoryAction, RepositoryOperationState,
    RepositoryQueryId, RepositorySnapshot,
};
use diffo_ui::{
    command_palette::{Command, CommandId},
    search_picker::{SearchItem, SearchPicker, SearchPickerEvent},
};
use ratatui::{Frame, layout::Rect};

use super::{Modal, Workbench, checkout_picker::relative_commit_age};

pub(super) const MERGE_COMMAND: CommandId = CommandId::new("git.merge");
pub(super) const ABORT_MERGE_COMMAND: CommandId = CommandId::new("git.abort_merge");

pub(super) fn palette_command(snapshot: &RepositorySnapshot) -> Option<Command> {
    if snapshot.operation == RepositoryOperationState::Merge {
        Some(Command {
            id: ABORT_MERGE_COMMAND,
            label: "Git: Abort Merge",
        })
    } else if snapshot.operation == RepositoryOperationState::None
        && !matches!(snapshot.head, HeadState::Unborn { .. })
    {
        Some(Command {
            id: MERGE_COMMAND,
            label: "Git: Merge...",
        })
    } else {
        None
    }
}

pub(super) enum MergePickerEvent {
    Consumed,
    Close,
    Merge(MergeTarget),
    Quit,
}

pub(super) struct MergePicker {
    pub(super) query_id: RepositoryQueryId,
    refs: Vec<MergeRef>,
    loaded_at_unix_seconds: i64,
    picker: SearchPicker<MergeIdentity, MergeTarget>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MergeIdentity {
    kind: MergeRefKind,
    full_ref: String,
}

impl MergePicker {
    pub(super) fn loading(query_id: RepositoryQueryId) -> Self {
        Self {
            query_id,
            refs: Vec::new(),
            loaded_at_unix_seconds: 0,
            picker: SearchPicker::new("Select a branch or tag to merge from", "Loading refs..."),
        }
    }

    pub(super) fn install(&mut self, refs: Vec<MergeRef>, snapshot: &RepositorySnapshot) {
        self.refs = refs;
        self.loaded_at_unix_seconds = current_unix_seconds();
        self.picker.set_empty_message("No matching refs");
        self.refresh(snapshot);
    }

    pub(super) fn refresh(&mut self, snapshot: &RepositorySnapshot) {
        let current = match &snapshot.head {
            HeadState::Named { name, .. } => Some(name.as_str()),
            HeadState::Unborn { .. } | HeadState::Detached { .. } => None,
        };
        let tracked_remote = snapshot
            .upstream
            .as_ref()
            .map(|upstream| upstream.name.as_str());
        let items = self
            .refs
            .iter()
            .filter(|item| match item.kind {
                MergeRefKind::Local => current != Some(item.name.as_str()),
                MergeRefKind::Remote => tracked_remote != Some(item.name.as_str()),
                MergeRefKind::Tag => true,
            })
            .map(|item| SearchItem {
                identity: MergeIdentity {
                    kind: item.kind,
                    full_ref: item.full_ref.clone(),
                },
                payload: MergeTarget {
                    kind: item.kind,
                    name: item.name.clone(),
                    full_ref: item.full_ref.clone(),
                    object_id: item.object_id.clone(),
                    commit_id: item.commit_id.clone(),
                    expected_head: snapshot.head.clone(),
                },
                label: item.name.clone(),
                preferred_match: None,
                trailing: relative_commit_age(
                    item.tip_commit_unix_seconds,
                    self.loaded_at_unix_seconds,
                ),
                aliases: match item.kind {
                    MergeRefKind::Remote => item
                        .name
                        .split_once('/')
                        .map(|(_, short)| vec![short.to_owned()])
                        .unwrap_or_default(),
                    MergeRefKind::Local | MergeRefKind::Tag => Vec::new(),
                },
                enabled: !matches!(snapshot.head, HeadState::Unborn { .. })
                    && snapshot.operation == RepositoryOperationState::None,
            })
            .collect();
        self.picker.reconcile_items(items);
    }

    pub(super) fn handle_event(&mut self, event: &Event, area: Rect) -> MergePickerEvent {
        match self.picker.handle_event(event, area) {
            SearchPickerEvent::Consumed => MergePickerEvent::Consumed,
            SearchPickerEvent::Cancel => MergePickerEvent::Close,
            SearchPickerEvent::Activate(target) => MergePickerEvent::Merge(target),
            SearchPickerEvent::Quit => MergePickerEvent::Quit,
        }
    }

    pub(super) fn render(&self, frame: &mut Frame, area: Rect) {
        self.picker.render(frame, area);
    }
}

impl Workbench {
    pub(super) fn handle_merge_picker_event(
        &mut self,
        event: &Event,
        area: Rect,
    ) -> Option<super::WorkbenchCommand> {
        let picker_event = match self.modal.as_mut() {
            Some(Modal::MergePicker(picker)) => picker.handle_event(event, area),
            _ => return None,
        };
        match picker_event {
            MergePickerEvent::Close => self.close_modal(),
            MergePickerEvent::Merge(target) => {
                self.close_modal();
                self.commands
                    .enqueue(RepositoryAction::Merge(Box::new(target)));
            }
            MergePickerEvent::Quit => self.should_quit = true,
            MergePickerEvent::Consumed => {}
        }
        Some(super::WorkbenchCommand::Redraw)
    }

    pub(super) fn refresh_merge_picker(&mut self) {
        if let Some(Modal::MergePicker(picker)) = self.modal.as_mut() {
            if self.diff.model.snapshot.operation == RepositoryOperationState::None {
                picker.refresh(&self.diff.model.snapshot);
            } else {
                self.close_modal();
            }
        }
    }

    pub(super) fn execute_merge_command(&mut self, command: CommandId) -> bool {
        if command == MERGE_COMMAND {
            if !matches!(self.diff.model.snapshot.head, HeadState::Unborn { .. })
                && self.diff.model.snapshot.operation == RepositoryOperationState::None
            {
                self.open_merge_picker();
            }
        } else if command == ABORT_MERGE_COMMAND {
            if self.diff.model.snapshot.operation == RepositoryOperationState::Merge {
                self.commands.enqueue(RepositoryAction::AbortMerge);
            }
        } else {
            return false;
        }
        true
    }

    fn open_merge_picker(&mut self) {
        let query_id = RepositoryQueryId(self.next_query_id);
        self.next_query_id = self.next_query_id.saturating_add(1);
        self.set_modal(Modal::MergePicker(MergePicker::loading(query_id)));
        self.pending_merge_query = Some(query_id);
    }

    pub fn take_merge_query(&mut self) -> Option<RepositoryQueryId> {
        self.pending_merge_query.take()
    }

    pub fn merge_refs_loaded(&mut self, query_id: RepositoryQueryId, refs: Vec<MergeRef>) {
        let snapshot = self.diff.model.snapshot.clone();
        if let Some(Modal::MergePicker(picker)) = self.modal.as_mut()
            && picker.query_id == query_id
        {
            picker.install(refs, &snapshot);
            self.request_redraw();
        }
    }

    pub fn merge_refs_load_failed(&mut self, query_id: RepositoryQueryId, message: &str) {
        if matches!(self.modal, Some(Modal::MergePicker(ref picker)) if picker.query_id == query_id)
        {
            self.close_modal();
            self.show_error("Could not load merge refs", message);
        }
    }
}

fn current_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workbench::{Activity, ApplicationAction, Modal};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use diffo_core::{OperationResult, UpstreamState};

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn merge_ref(kind: MergeRefKind, name: &str, id: &str) -> MergeRef {
        let prefix = match kind {
            MergeRefKind::Local => "refs/heads/",
            MergeRefKind::Remote => "refs/remotes/",
            MergeRefKind::Tag => "refs/tags/",
        };
        MergeRef {
            kind,
            name: name.to_owned(),
            full_ref: format!("{prefix}{name}"),
            object_id: id.to_owned(),
            commit_id: id.to_owned(),
            tip_commit_unix_seconds: None,
        }
    }

    #[test]
    fn merge_command_is_shared_and_abort_replaces_it_during_a_merge() {
        for activity in [Activity::Diff, Activity::Explorer] {
            let mut workbench = Workbench::new(RepositorySnapshot {
                head: HeadState::Named {
                    name: "main".to_owned(),
                    commit: "aaa".to_owned(),
                },
                ..RepositorySnapshot::default()
            });
            workbench.active = activity;

            assert!(workbench.execute_merge_command(MERGE_COMMAND));
            assert!(matches!(workbench.modal, Some(Modal::MergePicker(_))));

            workbench.close_modal();
            workbench.diff.model.snapshot.operation = RepositoryOperationState::Merge;
            assert_eq!(
                palette_command(&workbench.diff.model.snapshot).map(|command| command.id),
                Some(ABORT_MERGE_COMMAND)
            );
            assert!(workbench.execute_merge_command(ABORT_MERGE_COMMAND));
            let command = workbench
                .take_application_command(std::time::Instant::now())
                .expect("abort queued");
            assert_eq!(
                command.action,
                ApplicationAction::Repository(RepositoryAction::AbortMerge)
            );
        }
    }

    #[test]
    fn command_is_hidden_for_unborn_and_other_operation_states() {
        assert!(palette_command(&RepositorySnapshot::default()).is_none());
        let mut snapshot = RepositorySnapshot {
            head: HeadState::Detached {
                commit: "aaa".to_owned(),
            },
            operation: RepositoryOperationState::Other,
            ..RepositorySnapshot::default()
        };
        assert!(palette_command(&snapshot).is_none());

        snapshot.operation = RepositoryOperationState::None;
        assert_eq!(
            palette_command(&snapshot).map(|command| command.id),
            Some(MERGE_COMMAND)
        );
    }

    #[test]
    fn picker_omits_current_and_upstream_and_queues_typed_merge() {
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
        let mut workbench = Workbench::new(snapshot.clone());
        let area = Rect::new(0, 0, 100, 30);
        assert!(workbench.execute_merge_command(MERGE_COMMAND));
        let query_id = workbench.take_merge_query().expect("merge query");
        workbench.merge_refs_loaded(
            query_id,
            vec![
                merge_ref(MergeRefKind::Local, "main", "aaa"),
                merge_ref(MergeRefKind::Remote, "origin/main", "aaa"),
                merge_ref(MergeRefKind::Local, "topic", "bbb"),
                merge_ref(MergeRefKind::Tag, "v1", "ccc"),
            ],
        );

        let _ = workbench.handle_event(&key(KeyCode::Enter), area);
        let command = workbench
            .take_application_command(std::time::Instant::now())
            .expect("merge queued");
        assert!(matches!(
            command.action,
            ApplicationAction::Repository(RepositoryAction::Merge(target))
                if target.name == "topic"
                    && target.full_ref == "refs/heads/topic"
                    && target.object_id == "bbb"
                    && target.expected_head == snapshot.head
        ));
    }

    #[test]
    fn conflict_result_uses_information_feedback() {
        assert_eq!(
            crate::diff::model::operation_result_toast(&OperationResult::Merge {
                name: "topic".to_owned(),
                conflicts: 2,
            }),
            Some((
                crate::diff::ToastKind::Info,
                "Merge stopped with conflicts in 2 files".to_owned(),
            ))
        );
    }

    #[test]
    fn stale_merge_ref_results_cannot_queue_a_merge() {
        let snapshot = RepositorySnapshot {
            head: HeadState::Named {
                name: "main".to_owned(),
                commit: "aaa".to_owned(),
            },
            ..RepositorySnapshot::default()
        };
        let mut workbench = Workbench::new(snapshot);
        let area = Rect::new(0, 0, 100, 30);

        assert!(workbench.execute_merge_command(MERGE_COMMAND));
        assert_eq!(workbench.take_merge_query(), Some(RepositoryQueryId(1)));
        workbench.close_modal();
        assert!(workbench.execute_merge_command(MERGE_COMMAND));
        assert_eq!(workbench.take_merge_query(), Some(RepositoryQueryId(2)));

        workbench.merge_refs_loaded(
            RepositoryQueryId(1),
            vec![merge_ref(MergeRefKind::Local, "stale", "111")],
        );
        let _ = workbench.handle_event(&key(KeyCode::Enter), area);
        assert_eq!(workbench.commands.queued_len(), 0);

        workbench.merge_refs_loaded(
            RepositoryQueryId(2),
            vec![merge_ref(MergeRefKind::Local, "topic", "bbb")],
        );
        let _ = workbench.handle_event(&key(KeyCode::Enter), area);
        assert_eq!(workbench.commands.queued_len(), 1);
    }
}
