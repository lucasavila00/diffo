use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use diffo_core::RepositoryAction;

use super::{Activity, CommandIntent, Message, Modal, Tool, Workbench, WorkbenchCommand};
use crate::workbench::sync_remote::SyncRemotePicker;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum GlobalAction {
    OpenCommandPalette,
    ToggleHelp,
    Sync,
    QuickOpen,
}

struct Binding {
    keys: &'static [KeyCode],
    action: GlobalAction,
    label: &'static str,
    description: &'static str,
}

static BINDINGS: &[Binding] = &[
    Binding {
        keys: &[KeyCode::Char('o')],
        action: GlobalAction::QuickOpen,
        label: "o",
        description: "Quick Open",
    },
    Binding {
        keys: &[KeyCode::Char('1'), KeyCode::F(1)],
        action: GlobalAction::OpenCommandPalette,
        label: "1 / F1",
        description: "Open command palette",
    },
    Binding {
        keys: &[KeyCode::Char('2'), KeyCode::F(2)],
        action: GlobalAction::ToggleHelp,
        label: "2 / F2",
        description: "Toggle help",
    },
    Binding {
        keys: &[KeyCode::Char('9'), KeyCode::F(9)],
        action: GlobalAction::Sync,
        label: "9 / F9",
        description: "Sync",
    },
];

pub(super) fn action(event: &Event) -> Option<GlobalAction> {
    let Event::Key(key) = event else {
        return None;
    };
    if key.kind != KeyEventKind::Press || key.modifiers != KeyModifiers::NONE {
        return None;
    }
    BINDINGS
        .iter()
        .find(|binding| binding.keys.contains(&key.code))
        .map(|binding| binding.action)
}

pub(super) fn help_rows() -> impl Iterator<Item = (String, &'static str)> {
    BINDINGS
        .iter()
        .map(|binding| (binding.label.to_owned(), binding.description))
}

impl Workbench {
    pub(super) fn execute_sync(&mut self) -> Option<WorkbenchCommand> {
        if self.commands.has_sync() {
            return None;
        }
        if !self.commands.has_work() && !self.diff_model().sync_enabled() {
            return None;
        }
        if self.diff_model().snapshot.upstream.is_none()
            && matches!(
                &self.diff_model().snapshot.head,
                diffo_core::HeadState::Named { .. }
            )
        {
            let query_id = diffo_core::RepositoryQueryId(self.next_query_id);
            self.next_query_id = self.next_query_id.saturating_add(1);
            self.set_modal(Modal::SyncRemotePicker(SyncRemotePicker::loading(query_id)));
            self.pending_sync_remote_query = Some(query_id);
            return None;
        }
        if self.commands.has_work() {
            self.commands
                .enqueue_intent(CommandIntent::Repository(RepositoryAction::Sync));
            self.request_redraw();
            return None;
        }
        self.update_diff(Message::ExecuteSync)
            .map(WorkbenchCommand::Effect)
    }

    pub(super) fn active_help_rows(&self) -> Vec<(String, &'static str)> {
        let activity_rows = match self.active {
            Activity::Diff => self.diff.help_rows(),
            Activity::Explorer => self.explorer.help_rows(),
        };
        std::iter::once(("Tab".to_owned(), "Next activity"))
            .chain(help_rows())
            .chain(activity_rows)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEvent;

    #[test]
    fn bindings_are_unique_lowercase_and_drive_help() {
        for (index, binding) in BINDINGS.iter().enumerate() {
            assert!(
                binding.keys.iter().all(
                    |key| !matches!(key, KeyCode::Char(character) if character.is_uppercase())
                )
            );
            for other in &BINDINGS[index + 1..] {
                assert!(!binding.keys.iter().any(|key| other.keys.contains(key)));
            }
            for key in binding.keys {
                assert_eq!(
                    action(&Event::Key(KeyEvent::new(*key, KeyModifiers::NONE))),
                    Some(binding.action)
                );
            }
        }

        assert_eq!(
            help_rows().collect::<Vec<_>>(),
            vec![
                ("o".to_owned(), "Quick Open"),
                ("1 / F1".to_owned(), "Open command palette"),
                ("2 / F2".to_owned(), "Toggle help"),
                ("9 / F9".to_owned(), "Sync"),
            ]
        );
    }
}
