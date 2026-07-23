use std::path::PathBuf;

use crossterm::event::Event;
use diffo_ui::search_picker::{SearchItem, SearchPicker, SearchPickerEvent};
use ratatui::{Frame, layout::Rect};

pub(super) enum QuickOpenEvent {
    Consumed,
    Close,
    Open(PathBuf),
    Quit,
}

pub(super) struct QuickOpen {
    picker: SearchPicker<PathBuf, PathBuf>,
}

impl QuickOpen {
    pub(super) fn new(paths: &[PathBuf], loading: bool) -> Self {
        let mut modal = Self {
            picker: SearchPicker::new("Quick Open", "Loading files..."),
        };
        modal.install(paths, loading);
        modal
    }

    pub(super) fn install(&mut self, paths: &[PathBuf], loading: bool) {
        self.picker.set_empty_message(if loading {
            "Loading files..."
        } else {
            "No matching files"
        });
        self.picker.reconcile_items(
            paths
                .iter()
                .map(|path| SearchItem {
                    identity: path.clone(),
                    payload: path.clone(),
                    label: path.to_string_lossy().into_owned(),
                    preferred_match: path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned()),
                    trailing: None,
                    aliases: Vec::new(),
                    enabled: true,
                })
                .collect(),
        );
    }

    pub(super) fn handle_event(&mut self, event: &Event, area: Rect) -> QuickOpenEvent {
        match self.picker.handle_event(event, area) {
            SearchPickerEvent::Consumed => QuickOpenEvent::Consumed,
            SearchPickerEvent::Cancel => QuickOpenEvent::Close,
            SearchPickerEvent::Activate(path) => QuickOpenEvent::Open(path),
            SearchPickerEvent::Quit => QuickOpenEvent::Quit,
        }
    }

    pub(super) fn render(&self, frame: &mut Frame, area: Rect) {
        self.picker.render(frame, area);
    }

    #[cfg(test)]
    pub(super) fn query(&self) -> &str {
        self.picker.query()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn file_name_matches_rank_above_folder_only_matches() {
        let mut modal = QuickOpen::new(
            &[
                PathBuf::from("query/unrelated.rs"),
                PathBuf::from("src/query.rs"),
            ],
            false,
        );
        for character in "query".chars() {
            let event = Event::Key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
            assert!(matches!(
                modal.handle_event(&event, Rect::new(0, 0, 100, 30)),
                QuickOpenEvent::Consumed
            ));
        }

        assert_eq!(
            modal.picker.selected_identity(),
            Some(&PathBuf::from("src/query.rs"))
        );
    }
}
