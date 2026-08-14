use std::collections::HashSet;

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use diffo_core::{
    BranchKind, BranchRef, CreateBranchStartPoint, CreateBranchTarget, HeadState, RepositoryQueryId,
};
use diffo_ui::{
    command_palette::{Command, CommandId},
    design, modal_block, terminal_safe_text, theme,
};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    widgets::{Block, Borders, Clear, Paragraph},
};

pub(super) const CREATE_BRANCH_COMMAND: CommandId = CommandId::new("git.create_branch");
pub(super) const CREATE_BRANCH_FROM_COMMAND: CommandId = CommandId::new("git.create_branch_from");
pub(super) const CREATE_BRANCH_PALETTE_COMMAND: Command = Command {
    id: CREATE_BRANCH_COMMAND,
    label: "Git: Create Branch...",
};
pub(super) const CREATE_BRANCH_FROM_PALETTE_COMMAND: Command = Command {
    id: CREATE_BRANCH_FROM_COMMAND,
    label: "Git: Create Branch From...",
};

impl super::Workbench {
    pub(super) fn execute_create_branch_command(&mut self, command: CommandId) -> bool {
        if command == CREATE_BRANCH_COMMAND {
            self.open_create_branch();
        } else if command == CREATE_BRANCH_FROM_COMMAND {
            self.open_create_branch_from();
        } else {
            return false;
        }
        true
    }
}

pub(super) enum CreateBranchEvent {
    Close,
    Consumed,
    Create(CreateBranchTarget),
    Quit,
}

pub(super) struct CreateBranchModal {
    pub(super) query_id: Option<RepositoryQueryId>,
    input: String,
    cursor: usize,
    local_names: HashSet<String>,
    start_point: Option<CreateBranchStartPoint>,
    loading: bool,
}

impl CreateBranchModal {
    pub(super) fn loading(query_id: RepositoryQueryId) -> Self {
        Self {
            query_id: Some(query_id),
            input: String::new(),
            cursor: 0,
            local_names: HashSet::new(),
            start_point: None,
            loading: true,
        }
    }

    pub(super) fn install(&mut self, branches: Vec<BranchRef>, head: HeadState) {
        self.install_branches(branches);
        self.start_point = Some(CreateBranchStartPoint::Head(head));
        self.loading = false;
    }

    pub(super) fn ready(branches: Vec<BranchRef>, start_point: CreateBranchStartPoint) -> Self {
        let mut modal = Self {
            query_id: None,
            input: String::new(),
            cursor: 0,
            local_names: HashSet::new(),
            start_point: Some(start_point),
            loading: false,
        };
        modal.install_branches(branches);
        modal
    }

    fn install_branches(&mut self, branches: Vec<BranchRef>) {
        self.local_names = branches
            .into_iter()
            .filter(|branch| branch.kind == BranchKind::Local)
            .map(|branch| branch.name)
            .collect();
    }

    fn candidate(&self) -> String {
        sanitize_branch_name(&self.input)
    }

    fn validation(&self) -> Validation {
        if self.loading {
            return Validation::Error("Loading branches...".to_owned());
        }
        if matches!(
            self.start_point,
            Some(CreateBranchStartPoint::Head(HeadState::Unborn { .. }))
        ) {
            return Validation::Error("Create branch requires a commit".to_owned());
        }
        let candidate = self.candidate();
        if candidate.is_empty() {
            return Validation::Empty;
        }
        if self.local_names.contains(&candidate) {
            return Validation::Error(format!("Branch {candidate} already exists"));
        }
        if !valid_branch_name(&candidate) {
            return Validation::Error("Invalid branch name".to_owned());
        }
        if candidate == self.input {
            Validation::Ready(None)
        } else {
            Validation::Ready(Some(format!("The new branch will be {candidate}")))
        }
    }

    fn create_target(&self) -> Option<CreateBranchTarget> {
        if !matches!(self.validation(), Validation::Ready(_)) {
            return None;
        }
        Some(CreateBranchTarget {
            name: self.candidate(),
            start_point: self.start_point.clone()?,
        })
    }

    pub(super) fn handle_event(&mut self, event: &Event, area: Rect) -> CreateBranchEvent {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Esc => CreateBranchEvent::Close,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    CreateBranchEvent::Quit
                }
                KeyCode::Enter if self.input.trim().is_empty() => CreateBranchEvent::Close,
                KeyCode::Enter => self
                    .create_target()
                    .map_or(CreateBranchEvent::Consumed, CreateBranchEvent::Create),
                KeyCode::Left => {
                    self.cursor = self.cursor.saturating_sub(1);
                    CreateBranchEvent::Consumed
                }
                KeyCode::Right => {
                    self.cursor = self
                        .cursor
                        .saturating_add(1)
                        .min(self.input.chars().count());
                    CreateBranchEvent::Consumed
                }
                KeyCode::Backspace => {
                    if self.cursor > 0 {
                        let start = byte_index_at_char(&self.input, self.cursor.saturating_sub(1));
                        let end = byte_index_at_char(&self.input, self.cursor);
                        self.input.replace_range(start..end, "");
                        self.cursor = self.cursor.saturating_sub(1);
                    }
                    CreateBranchEvent::Consumed
                }
                KeyCode::Char(character)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                        && !character.is_control() =>
                {
                    let byte = byte_index_at_char(&self.input, self.cursor);
                    self.input.insert(byte, character);
                    self.cursor = self.cursor.saturating_add(1);
                    CreateBranchEvent::Consumed
                }
                _ => CreateBranchEvent::Consumed,
            },
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
                if create_branch_layout(area)
                    .modal
                    .contains((mouse.column, mouse.row).into())
                {
                    CreateBranchEvent::Consumed
                } else {
                    CreateBranchEvent::Close
                }
            }
            _ => CreateBranchEvent::Consumed,
        }
    }

    pub(super) fn render(&self, frame: &mut Frame, area: Rect) {
        let layout = create_branch_layout(area);
        frame.render_widget(Clear, layout.modal);
        frame.render_widget(modal_block("Create branch"), layout.modal);

        let validation = self.validation();
        let (message, style) = match validation {
            Validation::Empty | Validation::Ready(None) => {
                (String::new(), Style::default().fg(theme::CHROME))
            }
            Validation::Ready(Some(message)) => (
                terminal_safe_text(&message),
                Style::default().fg(theme::INFORMATION),
            ),
            Validation::Error(message) => (
                terminal_safe_text(&message),
                Style::default().fg(theme::DANGER),
            ),
        };
        frame.render_widget(Paragraph::new(message).style(style), layout.message);

        let field = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::CHROME));
        let inner = field.inner(layout.input);
        let width = usize::from(inner.width);
        let empty = self.input.is_empty();
        let start = self.cursor.saturating_sub(width.saturating_sub(1));
        let value = if empty {
            "Branch name".to_owned()
        } else {
            self.input.chars().skip(start).take(width).collect()
        };
        frame.render_widget(
            Paragraph::new(terminal_safe_text(&value)).style(Style::default().fg(if empty {
                theme::CHROME
            } else {
                theme::TEXT
            })),
            inner,
        );
        frame.render_widget(field, layout.input);
        let cursor = self
            .cursor
            .saturating_sub(start)
            .min(usize::from(inner.width.saturating_sub(1)));
        frame.set_cursor_position((
            inner
                .x
                .saturating_add(u16::try_from(cursor).unwrap_or(u16::MAX)),
            inner.y,
        ));

        frame.render_widget(
            Paragraph::new("Enter: create and checkout · Esc: cancel")
                .alignment(Alignment::Center)
                .style(Style::default().fg(theme::CHROME)),
            layout.footer,
        );
    }
}

enum Validation {
    Empty,
    Ready(Option<String>),
    Error(String),
}

struct CreateBranchLayout {
    modal: Rect,
    message: Rect,
    input: Rect,
    footer: Rect,
}

fn create_branch_layout(area: Rect) -> CreateBranchLayout {
    let width = design::COMMIT_EDITOR_WIDTH.resolve(area.width);
    let height = design::COMMIT_EDITOR_MAX_HEIGHT.min(area.height);
    let modal = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let rows = Layout::vertical([
        Constraint::Length(design::PROMPT_MESSAGE_HEIGHT),
        Constraint::Length(design::COMMIT_FIELD_HEIGHT),
        Constraint::Min(0),
        Constraint::Length(design::SINGLE_LINE_HEIGHT),
    ])
    .split(modal.inner(design::DIALOG_INSET));
    CreateBranchLayout {
        modal,
        message: rows[0],
        input: rows[1],
        footer: rows[3],
    }
}

fn byte_index_at_char(text: &str, character: usize) -> usize {
    text.char_indices()
        .nth(character)
        .map_or(text.len(), |(index, _)| index)
}

pub(super) fn sanitize_branch_name(input: &str) -> String {
    let trimmed = input.trim().trim_start_matches('-');
    let mut sanitized = String::with_capacity(trimmed.len());
    for character in trimmed.chars() {
        if character.is_control()
            || character.is_whitespace()
            || matches!(character, '~' | '^' | ':' | '?' | '*' | '[' | '\\')
        {
            sanitized.push('-');
        } else {
            sanitized.push(character);
        }
    }
    while sanitized.contains("..") {
        sanitized = sanitized.replace("..", "-");
    }
    while sanitized.contains("@{") {
        sanitized = sanitized.replace("@{", "@-");
    }
    sanitized
        .split('/')
        .map(|component| {
            let mut component = component.to_owned();
            if component.starts_with('.') {
                component.replace_range(..1, "-");
            }
            while has_lock_suffix(&component) {
                component.truncate(component.len().saturating_sub(".lock".len()));
                component.push('-');
            }
            if component.ends_with('.') {
                component.pop();
                component.push('-');
            }
            if component.is_empty() {
                component.push('-');
            }
            component
        })
        .collect::<Vec<_>>()
        .join("/")
}

pub(super) fn valid_branch_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && !name.contains("..")
        && !name.contains("@{")
        && !name.chars().any(|character| {
            character.is_control()
                || character.is_whitespace()
                || matches!(character, '~' | '^' | ':' | '?' | '*' | '[' | '\\')
        })
        && name.split('/').all(|component| {
            !component.is_empty()
                && component != "."
                && component != ".."
                && !component.starts_with('.')
                && !component.ends_with('.')
                && !has_lock_suffix(component)
        })
}

fn has_lock_suffix(value: &str) -> bool {
    value.strip_suffix(".lock").is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workbench::{Activity, ApplicationAction, Modal, Workbench};
    use crossterm::event::KeyEvent;
    use diffo_core::{RepositoryAction, RepositorySnapshot};

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn sanitizes_vscode_style_names_and_rejects_remaining_invalid_names() {
        assert_eq!(sanitize_branch_name("  --topic name  "), "topic-name");
        assert_eq!(
            sanitize_branch_name("feature/.nested.lock"),
            "feature/-nested-"
        );
        assert_eq!(sanitize_branch_name("topic..next@{x"), "topic-next@-x");
        assert!(valid_branch_name("feature/nested"));
        assert!(!valid_branch_name("-invalid"));
    }

    #[test]
    fn edits_at_the_cursor_and_submits_the_cleaned_name() {
        let mut modal = CreateBranchModal::loading(RepositoryQueryId(1));
        modal.install(
            Vec::new(),
            HeadState::Named {
                name: "main".to_owned(),
                commit: "abc".to_owned(),
            },
        );
        let area = Rect::new(0, 0, 100, 30);
        for character in "topc name".chars() {
            let _ = modal.handle_event(&key(KeyCode::Char(character)), area);
        }
        for _ in 0..6 {
            let _ = modal.handle_event(&key(KeyCode::Left), area);
        }
        let _ = modal.handle_event(&key(KeyCode::Char('i')), area);

        assert!(matches!(
            modal.handle_event(&key(KeyCode::Enter), area),
            CreateBranchEvent::Create(CreateBranchTarget { name, .. }) if name == "topic-name"
        ));
    }

    #[test]
    fn blocks_existing_and_unborn_branches() {
        let branch = BranchRef {
            kind: BranchKind::Local,
            name: "topic".to_owned(),
            full_ref: "refs/heads/topic".to_owned(),
            object_id: "abc".to_owned(),
            tip_commit_unix_seconds: None,
        };
        let mut modal = CreateBranchModal::loading(RepositoryQueryId(1));
        modal.input = "topic".to_owned();
        modal.cursor = 5;
        modal.install(
            vec![branch],
            HeadState::Named {
                name: "main".to_owned(),
                commit: "abc".to_owned(),
            },
        );
        assert!(
            matches!(modal.validation(), Validation::Error(message) if message.contains("already exists"))
        );

        modal.install(
            Vec::new(),
            HeadState::Unborn {
                name: "main".to_owned(),
            },
        );
        assert!(
            matches!(modal.validation(), Validation::Error(message) if message.contains("requires a commit"))
        );
    }

    #[test]
    fn shared_palette_command_ignores_stale_loads_and_captures_ready_head() {
        let snapshot = RepositorySnapshot {
            head: HeadState::Named {
                name: "main".to_owned(),
                commit: "abc".to_owned(),
            },
            ..RepositorySnapshot::default()
        };
        let area = Rect::new(0, 0, 100, 30);
        for activity in [Activity::Diff, Activity::Explorer, Activity::History] {
            let mut workbench = Workbench::new(snapshot.clone());
            workbench.active = activity;
            let _ = workbench.execute_palette_command(CREATE_BRANCH_COMMAND);
            let stale = workbench.take_branch_query().unwrap();
            let _ = workbench.handle_event(&key(KeyCode::Esc), area);
            let _ = workbench.execute_palette_command(CREATE_BRANCH_COMMAND);
            let current = workbench.take_branch_query().unwrap();
            workbench.branches_loaded(stale, Vec::new());
            assert!(matches!(
                workbench.modal,
                Some(Modal::CreateBranch(ref modal)) if modal.loading
            ));
            workbench.branches_loaded(current, Vec::new());
            workbench.repository_changed(RepositorySnapshot {
                head: HeadState::Named {
                    name: "main".to_owned(),
                    commit: "def".to_owned(),
                },
                ..RepositorySnapshot::default()
            });
            for character in "Topic Name".chars() {
                let _ = workbench.handle_event(&key(KeyCode::Char(character)), area);
            }
            let _ = workbench.handle_event(&key(KeyCode::Enter), area);
            let command = workbench
                .take_application_command(std::time::Instant::now())
                .expect("create branch queued");
            assert!(matches!(
                command.action,
                ApplicationAction::Repository(RepositoryAction::CreateBranch(target))
                    if target.name == "Topic-Name"
                        && matches!(&target.start_point, CreateBranchStartPoint::Head(
                            HeadState::Named { commit, .. }
                        ) if commit == "abc")
            ));
        }
    }
}
