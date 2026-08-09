use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use diffo_core::CreateBranchStartPoint;
use diffo_ui::command_palette::{Command, CommandPalette, PaletteEvent};
use ratatui::{Frame, layout::Rect};

use super::{
    PromptModal, Workbench, WorkbenchCommand,
    checkout_picker::{CheckoutPicker, CheckoutPickerEvent},
    create_branch::{CreateBranchEvent, CreateBranchModal},
    delete_branch::DeleteBranchConfirmation,
    error_dialog::{ErrorDialog, ErrorDialogEvent},
    help,
    merge::MergePicker,
    quick_open::QuickOpenEvent,
    render_prompt,
    sync_remote::{SyncRemoteEvent, SyncRemotePicker},
};

pub(super) enum Modal {
    Help,
    QuickOpen(super::quick_open::QuickOpen),
    CommandPalette(CommandPalette),
    CheckoutPicker(CheckoutPicker),
    MergePicker(MergePicker),
    CreateBranch(CreateBranchModal),
    DeleteBranchConfirmation(DeleteBranchConfirmation),
    SyncRemotePicker(SyncRemotePicker),
    CommitEditor,
    GitPrompt(PromptModal),
    Error(ErrorDialog),
}

impl Modal {
    pub(super) fn command_palette(commands: Vec<Command>) -> Self {
        let mut palette = CommandPalette::default();
        palette.open(commands);
        Self::CommandPalette(palette)
    }
}

impl Workbench {
    pub(super) fn set_modal(&mut self, modal: Modal) {
        self.dismiss_active_popover();
        self.full_screen = false;
        self.full_screen_pending = false;
        if matches!(
            self.modal,
            Some(Modal::CheckoutPicker(_) | Modal::CreateBranch(_))
        ) {
            self.pending_branch_query = None;
        }
        if matches!(self.modal, Some(Modal::MergePicker(_))) {
            self.pending_merge_query = None;
        }
        if matches!(self.modal, Some(Modal::SyncRemotePicker(_))) {
            self.pending_sync_remote_query = None;
        }
        self.modal = Some(modal);
        self.request_redraw();
    }

    pub(super) fn close_modal(&mut self) {
        if matches!(
            self.modal,
            Some(Modal::CheckoutPicker(_) | Modal::CreateBranch(_))
        ) {
            self.pending_branch_query = None;
        }
        if matches!(self.modal, Some(Modal::MergePicker(_))) {
            self.pending_merge_query = None;
        }
        if matches!(self.modal, Some(Modal::SyncRemotePicker(_))) {
            self.pending_sync_remote_query = None;
        }
        self.modal = None;
        self.request_redraw();
    }

    pub(super) fn render_modal(&self, frame: &mut Frame, content: Rect, area: Rect) {
        match self.modal.as_ref() {
            Some(Modal::Help) => help::render(frame, content, self.active_help_rows()),
            Some(Modal::QuickOpen(modal)) => modal.render(frame, area),
            Some(Modal::CommandPalette(palette)) => palette.render(frame, content),
            Some(Modal::CheckoutPicker(picker)) => picker.render(frame, area),
            Some(Modal::MergePicker(picker)) => picker.render(frame, area),
            Some(Modal::CreateBranch(modal)) => modal.render(frame, area),
            Some(Modal::DeleteBranchConfirmation(modal)) => modal.render(frame, area),
            Some(Modal::SyncRemotePicker(picker)) => picker.render(frame, area),
            Some(Modal::CommitEditor) => {
                crate::diff::render_commit_editor(frame, &self.diff.model, content);
            }
            Some(Modal::GitPrompt(prompt)) => render_prompt(frame, prompt, area),
            Some(Modal::Error(error)) => error.render(frame, area),
            None => {}
        }
    }

    pub(super) fn handle_modal_event(
        &mut self,
        event: &Event,
        area: Rect,
    ) -> Option<WorkbenchCommand> {
        match self.modal.as_ref()? {
            Modal::Help => self.handle_help_event(event),
            Modal::QuickOpen(_) => self.handle_quick_open_event(event, area),
            Modal::CommandPalette(_) => self.handle_palette_event(event, area),
            Modal::CheckoutPicker(_) => self.handle_checkout_picker_event(event, area),
            Modal::MergePicker(_) => self.handle_merge_picker_event(event, area),
            Modal::CreateBranch(_) => self.handle_create_branch_event(event, area),
            Modal::DeleteBranchConfirmation(_) => {
                self.handle_delete_branch_confirmation_event(event, area);
                None
            }
            Modal::SyncRemotePicker(_) => self.handle_sync_remote_event(event, area),
            Modal::CommitEditor => self.handle_commit_editor_event(event, area),
            Modal::GitPrompt(_) => self
                .handle_prompt_event(event, area)
                .map(WorkbenchCommand::Effect),
            Modal::Error(_) => {
                self.handle_error_event(event, area);
                None
            }
        }
    }

    fn handle_quick_open_event(&mut self, event: &Event, area: Rect) -> Option<WorkbenchCommand> {
        let modal_event = match self.modal.as_mut() {
            Some(Modal::QuickOpen(modal)) => modal.handle_event(event, area),
            _ => return None,
        };
        match modal_event {
            QuickOpenEvent::Close => self.close_modal(),
            QuickOpenEvent::Open(path) => {
                self.close_modal();
                self.active = super::Activity::Explorer;
                self.explorer.quick_open(path);
            }
            QuickOpenEvent::Quit => self.should_quit = true,
            QuickOpenEvent::Consumed => {}
        }
        Some(WorkbenchCommand::Redraw)
    }

    fn handle_sync_remote_event(&mut self, event: &Event, area: Rect) -> Option<WorkbenchCommand> {
        let picker_event = match self.modal.as_mut() {
            Some(Modal::SyncRemotePicker(picker)) => picker.handle_event(event, area),
            _ => return None,
        };
        match picker_event {
            SyncRemoteEvent::Close => self.close_modal(),
            SyncRemoteEvent::Quit => self.should_quit = true,
            SyncRemoteEvent::Select(remote) => {
                self.close_modal();
                self.update_diff(crate::diff::Message::ExecuteSyncToRemote(remote));
            }
            SyncRemoteEvent::Consumed => {}
        }
        Some(WorkbenchCommand::Redraw)
    }

    fn handle_create_branch_event(
        &mut self,
        event: &Event,
        area: Rect,
    ) -> Option<WorkbenchCommand> {
        let modal_event = match self.modal.as_mut() {
            Some(Modal::CreateBranch(modal)) => modal.handle_event(event, area),
            _ => return None,
        };
        match modal_event {
            CreateBranchEvent::Close => self.close_modal(),
            CreateBranchEvent::Create(target) => {
                self.close_modal();
                self.commands
                    .enqueue(diffo_core::RepositoryAction::CreateBranch(Box::new(target)));
            }
            CreateBranchEvent::Quit => self.should_quit = true,
            CreateBranchEvent::Consumed => {}
        }
        Some(WorkbenchCommand::Redraw)
    }

    fn handle_checkout_picker_event(
        &mut self,
        event: &Event,
        area: Rect,
    ) -> Option<WorkbenchCommand> {
        let picker_event = match self.modal.as_mut() {
            Some(Modal::CheckoutPicker(picker)) => picker.handle_event(event, area),
            _ => return None,
        };
        match picker_event {
            CheckoutPickerEvent::Close => self.close_modal(),
            CheckoutPickerEvent::Checkout(target) => {
                self.close_modal();
                self.commands
                    .enqueue(diffo_core::RepositoryAction::Checkout(Box::new(target)));
            }
            CheckoutPickerEvent::CreateBranch(target) => {
                let branches = match self.modal.as_ref() {
                    Some(Modal::CheckoutPicker(picker)) => picker.branches(),
                    _ => Vec::new(),
                };
                self.set_modal(Modal::CreateBranch(CreateBranchModal::ready(
                    branches,
                    CreateBranchStartPoint::Branch(target),
                )));
            }
            CheckoutPickerEvent::DeleteBranch(target) => {
                self.close_modal();
                self.commands
                    .enqueue(diffo_core::RepositoryAction::DeleteBranch(Box::new(target)));
            }
            CheckoutPickerEvent::Quit => self.should_quit = true,
            CheckoutPickerEvent::Consumed => {}
        }
        Some(WorkbenchCommand::Redraw)
    }

    fn handle_help_event(&mut self, event: &Event) -> Option<WorkbenchCommand> {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
        {
            if (matches!(key.code, KeyCode::Char('2') | KeyCode::F(2)) || key.code == KeyCode::Esc)
                && key.modifiers == KeyModifiers::NONE
            {
                self.close_modal();
                return Some(WorkbenchCommand::Redraw);
            } else if key.code == KeyCode::Char('c')
                && key.modifiers.contains(KeyModifiers::CONTROL)
            {
                self.should_quit = true;
            }
        }
        None
    }

    fn handle_palette_event(&mut self, event: &Event, area: Rect) -> Option<WorkbenchCommand> {
        let content = super::workbench_areas(area).content;
        let palette_event = match self.modal.as_mut() {
            Some(Modal::CommandPalette(palette)) => palette.handle_event(event, content),
            _ => None,
        };
        let palette_closed = matches!(
            self.modal.as_ref(),
            Some(Modal::CommandPalette(palette)) if !palette.is_open()
        );
        if palette_closed {
            self.close_modal();
        }
        match palette_event {
            Some(PaletteEvent::Execute(command)) => self
                .execute_palette_command(command)
                .map(WorkbenchCommand::Effect),
            Some(PaletteEvent::Quit) => {
                self.should_quit = true;
                None
            }
            Some(PaletteEvent::Consumed) => Some(WorkbenchCommand::Redraw),
            None => None,
        }
    }

    fn handle_commit_editor_event(
        &mut self,
        event: &Event,
        area: Rect,
    ) -> Option<WorkbenchCommand> {
        let content = super::workbench_areas(area).content;
        match crate::diff::map_commit_event(event, &self.diff.model, content) {
            Some(crate::diff::Message::BlurCommitInput) => {
                self.close_modal();
                Some(WorkbenchCommand::Redraw)
            }
            Some(message) => Some(WorkbenchCommand::Diff(message)),
            None => None,
        }
    }

    fn handle_error_event(&mut self, event: &Event, area: Rect) {
        let result = match self.modal.as_ref() {
            Some(Modal::Error(_)) => ErrorDialog::handle_event(event, area),
            _ => return,
        };
        match result {
            ErrorDialogEvent::Consumed => {}
            ErrorDialogEvent::Dismiss => self.dismiss_error(),
            ErrorDialogEvent::Quit => self.should_quit = true,
        }
    }

    pub(super) fn dismiss_error(&mut self) {
        self.modal = None;
        self.request_redraw();
        self.show_next_error();
    }

    pub(super) fn show_next_error(&mut self) {
        if self.modal.is_none()
            && let Some(error) = self.pending_errors.pop_front()
        {
            self.set_modal(Modal::Error(error));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explorer::ExplorerOutcome;
    use crate::workbench::{Activity, ApplicationAction, WorkbenchEffect, workbench_areas};
    use crossterm::event::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};
    use diffo_core::{
        FailureKind, FileDiff, FileState, GitPrompt, OperationFailure, PromptId, RepositoryAction,
        RepositoryQueryId, RepositorySnapshot,
    };
    use diffo_ui::tool_areas;

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn start_repository_command(
        workbench: &mut Workbench,
        action: RepositoryAction,
    ) -> diffo_core::ApplicationCommandId {
        let id = workbench.commands.enqueue(action);
        assert_eq!(
            workbench
                .take_application_command(std::time::Instant::now())
                .map(|command| command.id),
            Some(id)
        );
        id
    }

    #[test]
    fn help_toggles_with_2_and_f2_in_every_activity() {
        let area = Rect::new(0, 0, 100, 30);
        for activity in [Activity::Diff, Activity::Explorer, Activity::Review] {
            for shortcut in [KeyCode::Char('2'), KeyCode::F(2)] {
                let mut workbench = Workbench::new(RepositorySnapshot::default());
                workbench.active = activity;

                let _ = workbench.handle_event(&key(shortcut), area);
                assert!(matches!(workbench.modal, Some(Modal::Help)));
                let _ = workbench.handle_event(&key(shortcut), area);
                assert!(workbench.modal.is_none());
            }
        }
    }

    #[test]
    fn open_help_blocks_activity_switching() {
        let area = Rect::new(0, 0, 100, 30);
        let mut workbench = Workbench::new(RepositorySnapshot::default());

        let _ = workbench.handle_event(&key(KeyCode::F(2)), area);
        assert!(
            workbench
                .active_help_rows()
                .iter()
                .any(|(_, action)| *action == "Toggle inline / side-by-side view")
        );

        let _ = workbench.handle_event(&key(KeyCode::Tab), area);
        assert_eq!(workbench.active, Activity::Diff);
        assert!(matches!(workbench.modal, Some(Modal::Help)));
    }

    #[test]
    fn every_modal_blocks_activity_switching_and_second_modal_shortcuts() {
        let area = Rect::new(0, 0, 100, 30);
        for modal in [
            Modal::Help,
            Modal::command_palette(Vec::new()),
            Modal::CheckoutPicker(CheckoutPicker::loading(RepositoryQueryId(1))),
            Modal::MergePicker(MergePicker::loading(RepositoryQueryId(1))),
            Modal::CreateBranch(CreateBranchModal::loading(RepositoryQueryId(1))),
            Modal::DeleteBranchConfirmation(DeleteBranchConfirmation::new(
                diffo_core::DeleteBranchTarget {
                    name: "topic".to_owned(),
                    full_ref: "refs/heads/topic".to_owned(),
                    object_id: "abc".to_owned(),
                    force: false,
                },
            )),
            Modal::CommitEditor,
        ] {
            let help = matches!(modal, Modal::Help);
            let mut workbench = Workbench::new(RepositorySnapshot::default());
            workbench.set_modal(modal);

            let _ = workbench.handle_event(&key(KeyCode::Tab), area);
            let _ = workbench.handle_event(&key(KeyCode::F(1)), area);
            if !help {
                let _ = workbench.handle_event(&key(KeyCode::F(2)), area);
            }

            assert_eq!(workbench.active, Activity::Diff);
            assert!(workbench.modal.is_some());
        }
    }

    #[test]
    fn git_prompt_replaces_a_modal_without_restoring_it() {
        let area = Rect::new(0, 0, 100, 30);
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        let command_id = start_repository_command(&mut workbench, RepositoryAction::Fetch);
        workbench.set_modal(Modal::Help);

        assert!(workbench.open_prompt(
            command_id,
            PromptId(1),
            GitPrompt::Username {
                host: "example.com".to_owned(),
            }
        ));
        assert!(matches!(workbench.modal, Some(Modal::GitPrompt(_))));

        let effects = workbench.handle_events(&[key(KeyCode::Esc)], area);
        assert!(matches!(
            effects.as_slice(),
            [WorkbenchEffect::Prompt { .. }]
        ));
        assert!(workbench.modal.is_none());
    }

    #[test]
    fn opening_a_system_modal_dismisses_the_active_popover() {
        let area = Rect::new(0, 0, 100, 30);
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        let command_id = start_repository_command(&mut workbench, RepositoryAction::Fetch);
        workbench.active = Activity::Explorer;
        workbench.explorer.accept(ExplorerOutcome::Paths {
            id: 1,
            result: Ok(vec![std::path::PathBuf::from("file.txt")]),
        });
        workbench.prepare_frame(area);
        let tree = workbench
            .pane_split
            .areas(tool_areas(workbench_areas(area).content).content)
            .leading;
        let right_click = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Right),
            column: tree.x.saturating_add(2),
            row: tree.y.saturating_add(1),
            modifiers: KeyModifiers::NONE,
        });
        let _ = workbench.handle_event(&right_click, area);
        assert!(workbench.explorer.has_open_picker_menu());

        assert!(workbench.open_prompt(
            command_id,
            PromptId(1),
            GitPrompt::Username {
                host: "example.com".to_owned(),
            }
        ));

        assert!(!workbench.explorer.has_open_picker_menu());
        assert!(matches!(workbench.modal, Some(Modal::GitPrompt(_))));
    }

    #[test]
    fn modal_shortcuts_do_not_run_global_actions() {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        workbench.set_modal(Modal::CommitEditor);

        assert!(
            workbench
                .handle_events(
                    &[
                        key(KeyCode::Char('1')),
                        key(KeyCode::Char('2')),
                        key(KeyCode::Char('9')),
                        key(KeyCode::F(9)),
                    ],
                    Rect::new(0, 0, 80, 24),
                )
                .is_empty()
        );

        assert_eq!(workbench.diff.model.commit_message, "129");
        assert_eq!(workbench.commands.queued_len(), 0);
        assert!(matches!(workbench.modal, Some(Modal::CommitEditor)));
    }

    #[test]
    fn quick_open_is_global_and_captures_its_own_o_input() {
        let area = Rect::new(0, 0, 100, 30);
        for activity in [Activity::Diff, Activity::Explorer, Activity::Review] {
            let mut workbench = Workbench::new(RepositorySnapshot::default());
            workbench.active = activity;

            let _ = workbench.handle_event(&key(KeyCode::Char('o')), area);
            assert!(matches!(workbench.modal, Some(Modal::QuickOpen(_))));
            let _ = workbench.handle_event(&key(KeyCode::Char('o')), area);
            assert!(matches!(
                workbench.modal,
                Some(Modal::QuickOpen(ref modal)) if modal.query() == "o"
            ));
        }

        let mut workbench = Workbench::new(RepositorySnapshot::default());
        let shifted = Event::Key(KeyEvent::new(KeyCode::Char('O'), KeyModifiers::SHIFT));
        let _ = workbench.handle_event(&shifted, area);
        assert!(workbench.modal.is_none());
    }

    #[test]
    fn quick_open_retains_query_when_paths_arrive() {
        let area = Rect::new(0, 0, 100, 30);
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        let _ = workbench.handle_event(&key(KeyCode::Char('o')), area);
        let _ = workbench.handle_event(&key(KeyCode::Char('r')), area);

        workbench.accept_task_result(super::super::WorkbenchTaskResult::Explorer(
            ExplorerOutcome::Paths {
                id: 1,
                result: Ok(vec!["src/main.rs".into(), ".hidden".into()]),
            },
        ));

        assert!(matches!(
            workbench.modal,
            Some(Modal::QuickOpen(ref modal)) if modal.query() == "r"
        ));

        let _ = workbench.handle_event(&key(KeyCode::Enter), area);
        assert!(workbench.modal.is_none());
        assert_eq!(workbench.active, Activity::Explorer);
    }

    #[test]
    fn explorer_failures_use_the_shared_error_dialog() {
        let mut workbench = Workbench::new(RepositorySnapshot::default());

        workbench.accept_task_result(super::super::WorkbenchTaskResult::Explorer(
            ExplorerOutcome::Paths {
                id: 1,
                result: Err("permission denied".to_owned()),
            },
        ));

        assert!(matches!(
            workbench.modal,
            Some(Modal::Error(ref error))
                if error.title == "Explorer refresh failed"
                    && error.detail == "permission denied"
        ));
        assert!(workbench.toasts.as_slice().is_empty());
    }

    #[test]
    fn commit_failure_opens_error_and_preserves_the_closed_editor_draft() {
        let snapshot = RepositorySnapshot {
            files: vec![FileState {
                path: "src/main.rs".into(),
                old_path: None,
                kind: diffo_core::ChangeKind::Modified,
                staged: Some(FileDiff {
                    text: "@@ -1 +1 @@\n-old\n+new\n".to_owned(),
                }),
                unstaged: None,
            }],
            ..RepositorySnapshot::default()
        };
        let area = Rect::new(0, 0, 100, 30);
        let mut workbench = Workbench::new(snapshot);
        workbench.set_modal(Modal::CommitEditor);
        let _ = workbench.handle_events(&[key(KeyCode::Char('x')), key(KeyCode::Enter)], area);

        assert!(workbench.modal.is_none());
        assert_eq!(workbench.diff.model.commit_message, "x");
        let command = workbench
            .take_application_command(std::time::Instant::now())
            .expect("commit should be queued");
        workbench.action_failed(
            command.id,
            OperationFailure {
                action: match command.action {
                    ApplicationAction::Repository(action) => action,
                    ApplicationAction::AiCommit(_) => panic!("commit queued an AI commit"),
                    ApplicationAction::AiReview(_) => panic!("commit queued an AI review"),
                    ApplicationAction::Update => panic!("commit queued an update"),
                },
                kind: FailureKind::Network,
                detail: "commit failed".to_owned(),
            },
        );

        assert!(matches!(
            workbench.modal,
            Some(Modal::Error(ref error))
                if error.title == "Commit failed" && error.detail == "commit failed"
        ));
        assert_eq!(workbench.diff.model.commit_message, "x");

        let _ = workbench.handle_event(&key(KeyCode::Esc), area);
        assert!(workbench.modal.is_none());
        let _ = workbench.handle_events(&[key(KeyCode::Char('m'))], area);
        assert!(matches!(workbench.modal, Some(Modal::CommitEditor)));
        assert_eq!(workbench.diff.model.commit_message, "x");
    }

    #[test]
    fn errors_queue_fifo_and_coalesce_identical_pending_entries() {
        let area = Rect::new(0, 0, 100, 30);
        let mut workbench = Workbench::new(RepositorySnapshot::default());

        workbench.show_error("First failed", "first detail");
        workbench.show_error("Second failed", "second detail");
        workbench.show_error("Second failed", "second detail");

        assert!(matches!(
            workbench.modal,
            Some(Modal::Error(ref error)) if error.title == "First failed"
        ));
        assert_eq!(workbench.pending_errors.len(), 1);

        let _ = workbench.handle_event(&key(KeyCode::Enter), area);
        assert!(matches!(
            workbench.modal,
            Some(Modal::Error(ref error)) if error.title == "Second failed"
        ));
        let _ = workbench.handle_event(&key(KeyCode::Esc), area);
        assert!(workbench.modal.is_none());
    }

    #[test]
    fn git_prompt_defers_a_visible_error_until_the_prompt_closes() {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        let command_id = start_repository_command(&mut workbench, RepositoryAction::Fetch);
        workbench.show_error("Existing error", "detail");

        assert!(workbench.open_prompt(
            command_id,
            PromptId(1),
            GitPrompt::Username {
                host: "example.com".to_owned(),
            }
        ));
        assert!(matches!(workbench.modal, Some(Modal::GitPrompt(_))));

        workbench.close_prompt(command_id);
        assert!(matches!(
            workbench.modal,
            Some(Modal::Error(ref error)) if error.title == "Existing error"
        ));
    }

    #[test]
    fn error_arriving_during_git_prompt_appears_after_the_prompt_answer() {
        let area = Rect::new(0, 0, 100, 30);
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        let command_id = start_repository_command(&mut workbench, RepositoryAction::Fetch);
        assert!(workbench.open_prompt(
            command_id,
            PromptId(1),
            GitPrompt::Username {
                host: "example.com".to_owned(),
            }
        ));
        workbench.show_error("Deferred error", "detail");
        assert!(matches!(workbench.modal, Some(Modal::GitPrompt(_))));

        let _ = workbench.handle_event(&key(KeyCode::Esc), area);
        assert!(matches!(
            workbench.modal,
            Some(Modal::Error(ref error)) if error.title == "Deferred error"
        ));
    }

    #[test]
    fn command_palette_blocks_activity_switching_and_does_not_hide_state() {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        let area = Rect::new(0, 0, 100, 30);

        let _ = workbench.handle_event(&key(KeyCode::Char('1')), area);
        let _ = workbench.handle_event(&key(KeyCode::Char('p')), area);
        let _ = workbench.handle_event(&key(KeyCode::Tab), area);

        assert_eq!(workbench.active, Activity::Diff);
        assert!(matches!(
            workbench.modal,
            Some(Modal::CommandPalette(ref palette)) if palette.query() == "p"
        ));
        let _ = workbench.handle_event(&key(KeyCode::Esc), area);
        assert!(workbench.modal.is_none());
        let _ = workbench.handle_event(&key(KeyCode::Tab), area);
        assert_eq!(workbench.active, Activity::Explorer);
    }
}
