use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use diffo_ui::command_palette::{Command, CommandPalette, PaletteEvent};
use ratatui::{Frame, layout::Rect};

use super::{PromptModal, Workbench, WorkbenchCommand, help, render_prompt};

pub(super) enum Modal {
    Help,
    CommandPalette(CommandPalette),
    CommitEditor,
    GitPrompt(PromptModal),
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
        self.modal = Some(modal);
    }

    pub(super) fn close_modal(&mut self) {
        self.modal = None;
    }

    pub(super) fn render_modal(&self, frame: &mut Frame, content: Rect, area: Rect) {
        match self.modal.as_ref() {
            Some(Modal::Help) => help::render(frame, content, self.active_help_rows()),
            Some(Modal::CommandPalette(palette)) => palette.render(frame, content),
            Some(Modal::CommitEditor) => {
                crate::diff::render_commit_editor(frame, &self.diff.model, content);
            }
            Some(Modal::GitPrompt(prompt)) => render_prompt(frame, prompt, area),
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
            Modal::CommandPalette(_) => self.handle_palette_event(event, area),
            Modal::CommitEditor => self.handle_commit_editor_event(event, area),
            Modal::GitPrompt(_) => self
                .handle_prompt_event(event, area)
                .map(WorkbenchCommand::Effect),
        }
    }

    fn handle_help_event(&mut self, event: &Event) -> Option<WorkbenchCommand> {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
        {
            if (matches!(key.code, KeyCode::Char('2') | KeyCode::F(2)) || key.code == KeyCode::Esc)
                && key.modifiers == KeyModifiers::NONE
            {
                self.close_modal();
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
            Some(PaletteEvent::Consumed) | None => None,
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
                None
            }
            Some(message) => Some(WorkbenchCommand::Diff(message)),
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explorer::ExplorerOutcome;
    use crate::workbench::{Activity, WorkbenchEffect, workbench_areas};
    use crossterm::event::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};
    use diffo_core::{
        FailureKind, FileDiff, FileState, GitPrompt, OperationFailure, PromptId, RepositoryAction,
        RepositorySnapshot,
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
                .take_repository_command()
                .map(|command| command.id),
            Some(id)
        );
        id
    }

    #[test]
    fn help_toggles_with_2_and_f2_in_every_activity() {
        let area = Rect::new(0, 0, 100, 30);
        for activity in [Activity::Diff, Activity::Explorer, Activity::Search] {
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
    fn modal_shortcuts_remain_commit_message_text() {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        workbench.set_modal(Modal::CommitEditor);

        assert!(
            workbench
                .handle_events(
                    &[key(KeyCode::Char('1')), key(KeyCode::Char('2'))],
                    Rect::new(0, 0, 80, 24),
                )
                .is_empty()
        );

        assert_eq!(workbench.diff.model.commit_message, "12");
        assert!(matches!(workbench.modal, Some(Modal::CommitEditor)));
    }

    #[test]
    fn commit_submission_closes_the_editor_and_failure_reopens_the_draft() {
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
            .take_repository_command()
            .expect("commit should be queued");
        workbench.action_failed(
            command.id,
            OperationFailure {
                action: command.action,
                kind: FailureKind::Network,
                detail: "commit failed".to_owned(),
            },
        );

        assert!(matches!(workbench.modal, Some(Modal::CommitEditor)));
        assert_eq!(workbench.diff.model.commit_message, "x");
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
