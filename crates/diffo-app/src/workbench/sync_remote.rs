use crossterm::event::Event;
use diffo_core::RepositoryQueryId;
use diffo_ui::search_picker::{SearchItem, SearchPicker, SearchPickerEvent};
use ratatui::{Frame, layout::Rect};

use super::{Message, Modal, Workbench};

pub(super) enum SyncRemoteEvent {
    Close,
    Quit,
    Select(String),
    Consumed,
}

impl Workbench {
    pub fn take_sync_remote_query(&mut self) -> Option<RepositoryQueryId> {
        self.pending_sync_remote_query.take()
    }

    pub fn sync_remotes_loaded(&mut self, query_id: RepositoryQueryId, remotes: Vec<String>) {
        let Some(Modal::SyncRemotePicker(picker)) = self.modal.as_mut() else {
            return;
        };
        if picker.query_id != Some(query_id) {
            return;
        }
        let selected = remotes
            .iter()
            .find(|remote| remote.as_str() == "origin")
            .cloned()
            .or_else(|| match remotes.as_slice() {
                [remote] => Some(remote.clone()),
                _ => None,
            });
        if let Some(remote) = selected {
            self.close_modal();
            self.update_diff(Message::ExecuteSyncToRemote(remote));
        } else if remotes.is_empty() {
            self.close_modal();
            self.show_error(
                "Sync failed",
                "No remotes are configured; Sync does not create remotes",
            );
        } else if let Some(Modal::SyncRemotePicker(picker)) = self.modal.as_mut() {
            picker.install(remotes);
        }
    }

    pub fn sync_remotes_load_failed(&mut self, query_id: RepositoryQueryId, message: &str) {
        if matches!(
            self.modal.as_ref(),
            Some(Modal::SyncRemotePicker(picker)) if picker.query_id == Some(query_id)
        ) {
            self.close_modal();
            self.show_error("Could not load remotes", message);
        }
    }
}

pub(super) struct SyncRemotePicker {
    pub(super) query_id: Option<RepositoryQueryId>,
    picker: SearchPicker<String, String>,
}

impl SyncRemotePicker {
    pub(super) fn loading(query_id: RepositoryQueryId) -> Self {
        Self {
            query_id: Some(query_id),
            picker: SearchPicker::new("Sync branch", "Loading remotes..."),
        }
    }

    pub(super) fn install(&mut self, remotes: Vec<String>) {
        self.query_id = None;
        self.picker.set_empty_message("No remotes");
        self.picker.set_items(
            remotes
                .into_iter()
                .map(|remote| SearchItem {
                    identity: remote.clone(),
                    payload: remote.clone(),
                    label: remote,
                    preferred_match: None,
                    trailing: None,
                    aliases: Vec::new(),
                    enabled: true,
                })
                .collect(),
        );
    }

    pub(super) fn handle_event(&mut self, event: &Event, area: Rect) -> SyncRemoteEvent {
        match self.picker.handle_event(event, area) {
            SearchPickerEvent::Cancel => SyncRemoteEvent::Close,
            SearchPickerEvent::Quit => SyncRemoteEvent::Quit,
            SearchPickerEvent::Activate(remote) => SyncRemoteEvent::Select(remote),
            SearchPickerEvent::Consumed => SyncRemoteEvent::Consumed,
        }
    }

    pub(super) fn render(&self, frame: &mut Frame, area: Rect) {
        self.picker.render(frame, area);
    }
}
