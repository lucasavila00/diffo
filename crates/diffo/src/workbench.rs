use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use diffo_app::{Activity, Model};
use diffo_core::RepositorySnapshot;
use diffo_tui::{
    FramePreparation, Renderer, activity_at_position, render_activity_bar, workbench_areas,
};
use ratatui::{Frame, layout::Rect, widgets::Clear};

pub(crate) struct Workbench {
    active: Activity,
    diff: DiffActivity,
    explorer: ExplorerActivity,
    search: SearchActivity,
    should_quit: bool,
}

struct DiffActivity {
    model: Model,
    renderer: Renderer,
}

struct ExplorerActivity;
struct SearchActivity;

trait EmptyActivity {
    fn render(&mut self, frame: &mut Frame, area: Rect);
}

impl Workbench {
    pub(crate) fn new(snapshot: RepositorySnapshot) -> Self {
        Self {
            active: Activity::Diff,
            diff: DiffActivity {
                model: Model::new(snapshot),
                renderer: Renderer::new(),
            },
            explorer: ExplorerActivity,
            search: SearchActivity,
            should_quit: false,
        }
    }

    pub(crate) fn should_quit(&self) -> bool {
        self.should_quit || self.diff.model.should_quit
    }

    pub(crate) const fn active(&self) -> Activity {
        self.active
    }

    pub(crate) const fn diff_model(&self) -> &Model {
        &self.diff.model
    }

    pub(crate) fn diff_model_mut(&mut self) -> &mut Model {
        &mut self.diff.model
    }

    pub(crate) fn is_preparing(&self) -> bool {
        self.active == Activity::Diff && self.diff.renderer.is_preparing()
    }

    pub(crate) fn prepare_frame(&mut self, area: Rect) -> FramePreparation {
        if self.active != Activity::Diff {
            return FramePreparation::default();
        }
        let content = workbench_areas(area).content;
        self.diff.prepare_frame(content)
    }

    pub(crate) fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let content = workbench_areas(area).content;
        match self.active {
            Activity::Diff => self.diff.render(frame, content),
            Activity::Explorer => self.explorer.render(frame, content),
            Activity::Search => self.search.render(frame, content),
        }
        render_activity_bar(frame, area, self.active);
    }

    pub(crate) fn handle_workbench_event(&mut self, event: &Event, area: Rect) -> bool {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
            && key.code == KeyCode::Tab
            && key.modifiers == KeyModifiers::NONE
        {
            self.active = self.active.next();
            return true;
        }
        if let Event::Mouse(mouse) = event
            && mouse.kind == MouseEventKind::Down(MouseButton::Left)
            && let Some(activity) = activity_at_position(area, mouse.column, mouse.row)
        {
            self.active = activity;
            return true;
        }
        if self.active != Activity::Diff
            && let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
            && (matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
                || (key.code == KeyCode::Char('c')
                    && key.modifiers.contains(KeyModifiers::CONTROL)))
        {
            self.should_quit = true;
            return true;
        }
        false
    }

    pub(crate) fn map_diff_event(
        &mut self,
        event: &Event,
        area: Rect,
    ) -> Option<diffo_app::Message> {
        self.diff.renderer.map_event(event, &self.diff.model, area)
    }
}

impl DiffActivity {
    fn prepare_frame(&mut self, area: Rect) -> FramePreparation {
        let preparation = self.renderer.prepare_frame(&self.model, area);
        if let Some(viewport) = preparation.viewport_transition {
            self.model
                .set_diff_viewport(viewport.vertical, viewport.horizontal);
        }
        self.model.clamp_diff_scroll(
            preparation.maximum_vertical_scroll,
            preparation.maximum_horizontal_scroll,
        );
        self.model
            .set_file_list_scrolls(preparation.file_list_scroll);
        preparation
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        self.renderer.render_in(frame, &self.model, area);
    }
}

impl EmptyActivity for ExplorerActivity {
    fn render(&mut self, frame: &mut Frame, area: Rect) {
        frame.render_widget(Clear, area);
    }
}

impl EmptyActivity for SearchActivity {
    fn render(&mut self, frame: &mut Frame, area: Rect) {
        frame.render_widget(Clear, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyEventState, MouseEvent};
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn tab_cycles_activities_without_changing_diff_state() {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        workbench.diff.model.diff_scroll = 17;
        let tab = Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        let area = Rect::new(0, 0, 100, 30);

        assert!(workbench.handle_workbench_event(&tab, area));
        assert_eq!(workbench.active, Activity::Explorer);
        assert!(workbench.handle_workbench_event(&tab, area));
        assert_eq!(workbench.active, Activity::Search);
        assert!(workbench.handle_workbench_event(&tab, area));
        assert_eq!(workbench.active, Activity::Diff);
        assert_eq!(workbench.diff.model.diff_scroll, 17);
    }

    #[test]
    fn activity_bar_click_selects_and_consumes_the_activity() {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        let click = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 2,
            row: 4,
            modifiers: KeyModifiers::NONE,
        });

        assert!(workbench.handle_workbench_event(&click, Rect::new(0, 0, 100, 30)));
        assert_eq!(workbench.active, Activity::Search);
    }

    #[test]
    fn tab_requires_an_unmodified_key_press() {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        let repeat = Event::Key(crossterm::event::KeyEvent {
            code: KeyCode::Tab,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Repeat,
            state: KeyEventState::NONE,
        });
        let modified = Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT));

        assert!(!workbench.handle_workbench_event(&repeat, Rect::default()));
        assert!(!workbench.handle_workbench_event(&modified, Rect::default()));
        assert_eq!(workbench.active, Activity::Diff);
    }

    #[test]
    fn empty_activities_keep_quit_available() {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        workbench.active = Activity::Explorer;
        let quit = Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));

        assert!(workbench.handle_workbench_event(&quit, Rect::default()));
        assert!(workbench.should_quit());
    }

    #[test]
    fn empty_activities_draw_only_the_activity_bar() {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        workbench.active = Activity::Explorer;
        let backend = TestBackend::new(20, 12);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| workbench.render(frame)).unwrap();

        assert!(
            terminal
                .backend()
                .buffer()
                .content
                .iter()
                .enumerate()
                .all(|(index, cell)| {
                    let column = index % 20;
                    column < usize::from(diffo_tui::ACTIVITY_BAR_WIDTH) || cell.symbol() == " "
                })
        );
    }
}
