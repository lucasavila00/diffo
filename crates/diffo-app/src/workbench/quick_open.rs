use std::path::PathBuf;

use crossterm::event::{Event, KeyCode, KeyEventKind};
use diffo_ui::search_picker::{SearchItem, SearchPicker, SearchPickerEvent};
use ratatui::{Frame, layout::Rect};

use super::{Activity, Workbench, modal::Modal};

pub(super) enum QuickOpenEvent {
    Consumed,
    Close,
    Open(PathBuf),
    Quit,
}

pub(super) struct QuickOpen {
    picker: SearchPicker<PathBuf, PathBuf>,
    loading: bool,
    activate_when_ready: bool,
}

impl QuickOpen {
    pub(super) fn new(paths: &[PathBuf], loading: bool) -> Self {
        let mut modal = Self {
            picker: SearchPicker::new("Quick Open", "Loading files..."),
            loading,
            activate_when_ready: false,
        };
        modal.install(paths, loading);
        modal
    }

    pub(super) fn install(&mut self, paths: &[PathBuf], loading: bool) -> Option<PathBuf> {
        self.loading = loading;
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
        if self.activate_when_ready && !loading {
            self.activate_when_ready = false;
            return self.picker.selected_identity().cloned();
        }
        None
    }

    pub(super) fn handle_event(&mut self, event: &Event, area: Rect) -> QuickOpenEvent {
        if self.loading
            && let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
            && key.code == KeyCode::Enter
        {
            self.activate_when_ready = true;
            return QuickOpenEvent::Consumed;
        }
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

impl Workbench {
    pub(super) fn open_quick_open(&mut self) {
        self.explorer.request_quick_open_paths();
        let (paths, loading) = self.explorer.quick_open_paths();
        self.set_modal(Modal::QuickOpen(QuickOpen::new(paths, loading)));
    }

    pub(super) fn refresh_quick_open(&mut self) {
        let (paths, loading) = self.explorer.quick_open_paths();
        let path = if let Some(Modal::QuickOpen(modal)) = self.modal.as_mut() {
            modal.install(paths, loading)
        } else {
            None
        };
        if let Some(path) = path {
            self.close_modal();
            self.active = Activity::Explorer;
            self.explorer.quick_open(path);
        }
        if self.modal.is_some() {
            self.request_redraw();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};

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

    #[test]
    fn activates_the_selection_when_paths_arrive_after_enter() {
        let mut modal = QuickOpen::new(&[], true);
        for character in "query".chars() {
            let event = Event::Key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
            assert!(matches!(
                modal.handle_event(&event, Rect::new(0, 0, 100, 30)),
                QuickOpenEvent::Consumed
            ));
        }
        assert!(matches!(
            modal.handle_event(
                &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                Rect::new(0, 0, 100, 30)
            ),
            QuickOpenEvent::Consumed
        ));

        assert_eq!(
            modal.install(&[PathBuf::from("src/query.rs")], false),
            Some(PathBuf::from("src/query.rs"))
        );
    }
}
