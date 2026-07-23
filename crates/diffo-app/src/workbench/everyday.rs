use std::collections::HashSet;

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use diffo_core::{
    AmendTarget, BranchKind, BranchRef, Commit, DiscardAllTarget, DiscardTarget, HeadState,
    RenameBranchTarget, RepositoryAction, RepositoryQueryId, StashEntry, UndoCommitTarget,
};
use diffo_ui::{
    command_palette::{Command, CommandId},
    design, modal_block, terminal_safe_text, theme,
};
use diffo_ui::search_picker::{SearchItem, SearchPicker, SearchPickerEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    widgets::{Block, Borders, Clear, Paragraph},
};

use super::{
    ConfirmChoice, Modal, ToastKind, Workbench,
    create_branch::{sanitize_branch_name, valid_branch_name},
    prompt_button_style, prompt_layout,
};

mod workbench;

pub(super) const DISCARD_ALL_COMMAND: CommandId = CommandId::new("git.discard_all");
pub(super) const STASH_COMMAND: CommandId = CommandId::new("git.stash");
pub(super) const APPLY_STASH_COMMAND: CommandId = CommandId::new("git.apply_stash");
pub(super) const DROP_STASH_COMMAND: CommandId = CommandId::new("git.drop_stash");
pub(super) const AMEND_COMMAND: CommandId = CommandId::new("git.amend");
pub(super) const UNDO_COMMIT_COMMAND: CommandId = CommandId::new("git.undo_last_commit");
pub(super) const REVERT_COMMAND: CommandId = CommandId::new("git.revert");
pub(super) const RENAME_BRANCH_COMMAND: CommandId = CommandId::new("git.rename_branch");

pub(super) const COMMANDS: [Command; 8] = [
    Command {
        id: DISCARD_ALL_COMMAND,
        label: "Git: Discard All Changes...",
    },
    Command {
        id: STASH_COMMAND,
        label: "Git: Stash Changes...",
    },
    Command {
        id: APPLY_STASH_COMMAND,
        label: "Git: Apply Stash...",
    },
    Command {
        id: DROP_STASH_COMMAND,
        label: "Git: Drop Stash...",
    },
    Command {
        id: AMEND_COMMAND,
        label: "Git: Amend Last Commit...",
    },
    Command {
        id: UNDO_COMMIT_COMMAND,
        label: "Git: Undo Last Commit...",
    },
    Command {
        id: REVERT_COMMAND,
        label: "Git: Revert Commit...",
    },
    Command {
        id: RENAME_BRANCH_COMMAND,
        label: "Git: Rename Branch...",
    },
];

pub(super) enum EverydayModal {
    Input(InputModal),
    Picker(EverydayPicker),
    Confirmation(ConfirmationModal),
}

impl EverydayModal {
    pub(super) fn render(&self, frame: &mut Frame, area: Rect) {
        match self {
            Self::Input(modal) => modal.render(frame, area),
            Self::Picker(modal) => modal.render(frame, area),
            Self::Confirmation(modal) => modal.render(frame, area),
        }
    }

    pub(super) fn handle_event(&mut self, event: &Event, area: Rect) -> EverydayEvent {
        match self {
            Self::Input(modal) => modal.handle_event(event, area),
            Self::Picker(modal) => modal.handle_event(event, area),
            Self::Confirmation(modal) => modal.handle_event(event, area),
        }
    }

    pub(super) fn branch_query_id(&self) -> Option<RepositoryQueryId> {
        match self {
            Self::Input(InputModal {
                purpose: InputPurpose::Rename { query_id, .. },
                ..
            }) => Some(*query_id),
            _ => None,
        }
    }

    pub(super) fn stash_query_id(&self) -> Option<RepositoryQueryId> {
        match self {
            Self::Picker(EverydayPicker {
                query_id: Some(query_id),
                purpose: PickerPurpose::ApplyStash | PickerPurpose::DropStash,
                ..
            }) => Some(*query_id),
            _ => None,
        }
    }

}

pub(super) enum EverydayEvent {
    Consumed,
    Close,
    Quit,
    Run(RepositoryAction),
    Confirm(ConfirmationModal),
}

struct InputModal {
    title: &'static str,
    placeholder: &'static str,
    input: String,
    cursor: usize,
    purpose: InputPurpose,
}

enum InputPurpose {
    Stash,
    Amend { expected_head: String },
    Rename {
        query_id: RepositoryQueryId,
        target: RenameBranchTarget,
        local_names: HashSet<String>,
        loading: bool,
    },
}

impl InputModal {
    fn stash() -> Self {
        Self {
            title: "Stash changes",
            placeholder: "Optional stash message",
            input: String::new(),
            cursor: 0,
            purpose: InputPurpose::Stash,
        }
    }

    fn amend(expected_head: String, message: String) -> Self {
        let cursor = message.chars().count();
        Self {
            title: "Amend last commit",
            placeholder: "Commit message",
            input: message,
            cursor,
            purpose: InputPurpose::Amend { expected_head },
        }
    }

    fn rename(query_id: RepositoryQueryId, target: RenameBranchTarget) -> Self {
        let input = target.old_name.clone();
        let cursor = input.chars().count();
        Self {
            title: "Rename branch",
            placeholder: "Branch name",
            input,
            cursor,
            purpose: InputPurpose::Rename {
                query_id,
                target,
                local_names: HashSet::new(),
                loading: true,
            },
        }
    }

    fn install_branches(&mut self, branches: Vec<BranchRef>) {
        let InputPurpose::Rename {
            local_names,
            loading,
            ..
        } = &mut self.purpose
        else {
            return;
        };
        *local_names = branches
            .into_iter()
            .filter(|branch| branch.kind == BranchKind::Local)
            .map(|branch| branch.name)
            .collect();
        *loading = false;
    }

    fn action(&self) -> Result<RepositoryAction, String> {
        match &self.purpose {
            InputPurpose::Stash => Ok(RepositoryAction::Stash {
                message: self.input.trim().to_owned(),
            }),
            InputPurpose::Amend { expected_head } => {
                let message = self.input.trim();
                if message.is_empty() {
                    return Err("Commit message cannot be empty".to_owned());
                }
                Ok(RepositoryAction::Amend(Box::new(AmendTarget {
                    expected_head: expected_head.clone(),
                    message: message.to_owned(),
                })))
            }
            InputPurpose::Rename {
                target,
                local_names,
                loading,
                ..
            } => {
                if *loading {
                    return Err("Loading branches...".to_owned());
                }
                let name = sanitize_branch_name(&self.input);
                if name.is_empty() || !valid_branch_name(&name) {
                    return Err("Invalid branch name".to_owned());
                }
                if name == target.old_name {
                    return Err("Enter a different branch name".to_owned());
                }
                if local_names.contains(&name) {
                    return Err(format!("Branch {name} already exists"));
                }
                Ok(RepositoryAction::RenameBranch(Box::new(
                    RenameBranchTarget {
                        new_name: name,
                        ..target.clone()
                    },
                )))
            }
        }
    }

    fn validation(&self) -> Option<String> {
        self.action().err().filter(|message| {
            !matches!(self.purpose, InputPurpose::Stash)
                && !(matches!(self.purpose, InputPurpose::Amend { .. })
                    && self.input.trim().is_empty())
        })
    }

    fn handle_event(&mut self, event: &Event, area: Rect) -> EverydayEvent {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Esc => EverydayEvent::Close,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    EverydayEvent::Quit
                }
                KeyCode::Enter => match self.action() {
                    Ok(action @ RepositoryAction::RenameBranch(ref target))
                        if target.had_upstream => EverydayEvent::Confirm(
                            ConfirmationModal::new(
                                format!(
                                    "Rename {} to {} and clear its upstream?",
                                    target.old_name, target.new_name
                                ),
                                "Rename branch",
                                action,
                            ),
                        ),
                    Ok(action) => EverydayEvent::Run(action),
                    Err(_) => EverydayEvent::Consumed,
                },
                KeyCode::Left => {
                    self.cursor = self.cursor.saturating_sub(1);
                    EverydayEvent::Consumed
                }
                KeyCode::Right => {
                    self.cursor = self.cursor.saturating_add(1).min(self.input.chars().count());
                    EverydayEvent::Consumed
                }
                KeyCode::Backspace => {
                    if self.cursor > 0 {
                        let start = byte_index(&self.input, self.cursor.saturating_sub(1));
                        let end = byte_index(&self.input, self.cursor);
                        self.input.replace_range(start..end, "");
                        self.cursor = self.cursor.saturating_sub(1);
                    }
                    EverydayEvent::Consumed
                }
                KeyCode::Char(character)
                    if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                        && !character.is_control() =>
                {
                    let byte = byte_index(&self.input, self.cursor);
                    self.input.insert(byte, character);
                    self.cursor = self.cursor.saturating_add(1);
                    EverydayEvent::Consumed
                }
                _ => EverydayEvent::Consumed,
            },
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
                if input_layout(area).modal.contains((mouse.column, mouse.row).into()) {
                    EverydayEvent::Consumed
                } else {
                    EverydayEvent::Close
                }
            }
            _ => EverydayEvent::Consumed,
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        let layout = input_layout(area);
        frame.render_widget(Clear, layout.modal);
        frame.render_widget(modal_block(self.title), layout.modal);
        if let Some(message) = self.validation() {
            frame.render_widget(
                Paragraph::new(terminal_safe_text(&message))
                    .style(Style::default().fg(theme::DANGER)),
                layout.message,
            );
        }
        let field = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::CHROME));
        let inner = field.inner(layout.input);
        let width = usize::from(inner.width);
        let empty = self.input.is_empty();
        let start = self.cursor.saturating_sub(width.saturating_sub(1));
        let value = if empty {
            self.placeholder.to_owned()
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
            inner.x.saturating_add(u16::try_from(cursor).unwrap_or(u16::MAX)),
            inner.y,
        ));
        frame.render_widget(
            Paragraph::new("Enter: continue · Esc: cancel")
                .alignment(Alignment::Center)
                .style(Style::default().fg(theme::CHROME)),
            layout.footer,
        );
    }
}

#[derive(Clone)]
enum PickerPayload {
    Stash(StashEntry),
    Commit(Commit),
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum PickerPurpose {
    ApplyStash,
    DropStash,
    Revert,
}

struct EverydayPicker {
    query_id: Option<RepositoryQueryId>,
    purpose: PickerPurpose,
    picker: SearchPicker<String, PickerPayload>,
}

impl EverydayPicker {
    fn loading_stashes(query_id: RepositoryQueryId, purpose: PickerPurpose) -> Self {
        let title = match purpose {
            PickerPurpose::ApplyStash => "Apply stash",
            PickerPurpose::DropStash => "Drop stash",
            _ => unreachable!(),
        };
        Self {
            query_id: Some(query_id),
            purpose,
            picker: SearchPicker::new(title, "Loading stashes..."),
        }
    }

    fn revert(commits: &[Commit]) -> Self {
        let mut picker = SearchPicker::new("Revert commit", "No commits to revert");
        picker.set_items(
            commits
                .iter()
                .map(|commit| SearchItem {
                    identity: commit.id.clone(),
                    payload: PickerPayload::Commit(commit.clone()),
                    label: commit.summary.clone(),
                    preferred_match: None,
                    trailing: Some(commit.id.chars().take(7).collect()),
                    aliases: vec![commit.id.clone()],
                    enabled: true,
                })
                .collect(),
        );
        Self {
            query_id: None,
            purpose: PickerPurpose::Revert,
            picker,
        }
    }

    fn install_stashes(&mut self, stashes: Vec<StashEntry>) {
        self.query_id = None;
        self.picker.set_empty_message("No stashes");
        self.picker.set_items(
            stashes
                .into_iter()
                .map(|stash| SearchItem {
                    identity: stash.object_id.clone(),
                    label: format!("{} {}", stash.name, stash.summary),
                    payload: PickerPayload::Stash(stash),
                    preferred_match: None,
                    trailing: None,
                    aliases: Vec::new(),
                    enabled: true,
                })
                .collect(),
        );
    }

    fn handle_event(&mut self, event: &Event, area: Rect) -> EverydayEvent {
        match self.picker.handle_event(event, area) {
            SearchPickerEvent::Consumed => EverydayEvent::Consumed,
            SearchPickerEvent::Cancel => EverydayEvent::Close,
            SearchPickerEvent::Quit => EverydayEvent::Quit,
            SearchPickerEvent::Activate(PickerPayload::Stash(stash)) => match self.purpose {
                PickerPurpose::ApplyStash => {
                    EverydayEvent::Run(RepositoryAction::ApplyStash(Box::new(stash)))
                }
                PickerPurpose::DropStash => EverydayEvent::Confirm(ConfirmationModal::new(
                    format!("Drop {}? This saved state will be deleted.", stash.name),
                    "Drop stash",
                    RepositoryAction::DropStash(Box::new(stash)),
                )),
                _ => EverydayEvent::Consumed,
            },
            SearchPickerEvent::Activate(PickerPayload::Commit(commit)) => {
                EverydayEvent::Run(RepositoryAction::Revert(Box::new(commit)))
            }
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        self.picker.render(frame, area);
    }
}

pub(super) struct ConfirmationModal {
    message: String,
    continue_label: &'static str,
    action: RepositoryAction,
    choice: ConfirmChoice,
}

impl ConfirmationModal {
    fn new(
        message: String,
        continue_label: &'static str,
        action: RepositoryAction,
    ) -> Self {
        Self {
            message,
            continue_label,
            action,
            choice: ConfirmChoice::Cancel,
        }
    }

    fn handle_event(&mut self, event: &Event, area: Rect) -> EverydayEvent {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Esc => EverydayEvent::Close,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    EverydayEvent::Quit
                }
                KeyCode::Enter if self.choice == ConfirmChoice::Continue => {
                    EverydayEvent::Run(self.action.clone())
                }
                KeyCode::Enter => EverydayEvent::Close,
                KeyCode::Left | KeyCode::Up => {
                    self.choice = ConfirmChoice::Cancel;
                    EverydayEvent::Consumed
                }
                KeyCode::Right | KeyCode::Down => {
                    self.choice = ConfirmChoice::Continue;
                    EverydayEvent::Consumed
                }
                _ => EverydayEvent::Consumed,
            },
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
                let layout = prompt_layout(area, true);
                let point = (mouse.column, mouse.row).into();
                if layout.cancel.contains(point) {
                    EverydayEvent::Close
                } else if layout.continue_button.contains(point) {
                    EverydayEvent::Run(self.action.clone())
                } else {
                    EverydayEvent::Consumed
                }
            }
            _ => EverydayEvent::Consumed,
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        let layout = prompt_layout(area, true);
        frame.render_widget(Clear, layout.modal);
        frame.render_widget(modal_block("Confirm Git operation"), layout.modal);
        frame.render_widget(
            Paragraph::new(terminal_safe_text(&self.message)),
            layout.message,
        );
        frame.render_widget(
            Paragraph::new("[ Cancel ]")
                .alignment(Alignment::Center)
                .style(prompt_button_style(
                    self.choice == ConfirmChoice::Cancel,
                    true,
                )),
            layout.cancel,
        );
        frame.render_widget(
            Paragraph::new(format!("[ {} ]", self.continue_label))
                .alignment(Alignment::Center)
                .style(prompt_button_style(
                    self.choice == ConfirmChoice::Continue,
                    true,
                )),
            layout.continue_button,
        );
        frame.render_widget(
            Paragraph::new("Arrows: select · Enter: choose · Esc: cancel")
                .alignment(Alignment::Center)
                .style(Style::default().fg(theme::CHROME)),
            layout.footer,
        );
    }
}

struct InputLayout {
    modal: Rect,
    message: Rect,
    input: Rect,
    footer: Rect,
}

fn input_layout(area: Rect) -> InputLayout {
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
    InputLayout {
        modal,
        message: rows[0],
        input: rows[1],
        footer: rows[3],
    }
}

fn byte_index(text: &str, character: usize) -> usize {
    text.char_indices()
        .nth(character)
        .map_or(text.len(), |(index, _)| index)
}
