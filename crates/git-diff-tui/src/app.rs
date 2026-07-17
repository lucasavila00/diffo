use crate::repository::RepositorySnapshot;

const PAGE_SIZE: usize = 20;

pub struct App {
    pub snapshot: RepositorySnapshot,
    pub scroll: usize,
    pub should_quit: bool,
    line_count: usize,
}

impl App {
    #[must_use]
    pub fn new(snapshot: RepositorySnapshot) -> Self {
        let line_count = snapshot
            .files
            .iter()
            .flat_map(|file| [file.staged.as_ref(), file.unstaged.as_ref()])
            .flatten()
            .map(|diff| diff.text.lines().count())
            .sum();
        Self {
            snapshot,
            scroll: 0,
            should_quit: false,
            line_count,
        }
    }

    pub fn scroll_down(&mut self) {
        self.scroll = self.scroll.saturating_add(1).min(self.max_scroll());
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub fn page_down(&mut self) {
        self.scroll = self.scroll.saturating_add(PAGE_SIZE).min(self.max_scroll());
    }

    pub fn page_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(PAGE_SIZE);
    }

    pub fn scroll_to_top(&mut self) {
        self.scroll = 0;
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll = self.max_scroll();
    }

    fn max_scroll(&self) -> usize {
        self.line_count.saturating_sub(1)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::repository::{ChangeKind, FileDiff, FileState, RepositorySnapshot};

    use super::App;

    #[test]
    fn scrolling_stays_within_diff() {
        let mut app = App::new(RepositorySnapshot {
            files: vec![FileState {
                path: PathBuf::from("file.txt"),
                old_path: None,
                kind: ChangeKind::Modified,
                staged: None,
                unstaged: Some(FileDiff {
                    text: "one\ntwo\nthree".into(),
                }),
            }],
            ..RepositorySnapshot::default()
        });

        app.page_down();
        assert_eq!(app.scroll, 2);

        app.page_up();
        assert_eq!(app.scroll, 0);
    }
}
