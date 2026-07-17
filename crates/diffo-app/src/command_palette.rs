#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandId {
    Pull,
    PullFrom,
    Push,
    PushTo,
    Fetch,
    FetchPrune,
    Sync,
    Commit,
    CommitStaged,
    CommitAll,
    StageAll,
    UnstageAll,
    Refresh,
    ViewHistory,
    Checkout,
    CreateBranch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Command {
    pub id: CommandId,
    pub label: &'static str,
}

pub static COMMANDS: &[Command] = &[
    Command {
        id: CommandId::Pull,
        label: "Git: Pull",
    },
    Command {
        id: CommandId::PullFrom,
        label: "Git: Pull from...",
    },
    Command {
        id: CommandId::Push,
        label: "Git: Push",
    },
    Command {
        id: CommandId::PushTo,
        label: "Git: Push to...",
    },
    Command {
        id: CommandId::Fetch,
        label: "Git: Fetch",
    },
    Command {
        id: CommandId::FetchPrune,
        label: "Git: Fetch (Prune)",
    },
    Command {
        id: CommandId::Sync,
        label: "Git: Sync",
    },
    Command {
        id: CommandId::Commit,
        label: "Git: Commit",
    },
    Command {
        id: CommandId::CommitStaged,
        label: "Git: Commit Staged",
    },
    Command {
        id: CommandId::CommitAll,
        label: "Git: Commit All",
    },
    Command {
        id: CommandId::StageAll,
        label: "Git: Stage All Changes",
    },
    Command {
        id: CommandId::UnstageAll,
        label: "Git: Unstage All Changes",
    },
    Command {
        id: CommandId::Refresh,
        label: "Git: Refresh",
    },
    Command {
        id: CommandId::ViewHistory,
        label: "Git: View History",
    },
    Command {
        id: CommandId::Checkout,
        label: "Git: Checkout...",
    },
    Command {
        id: CommandId::CreateBranch,
        label: "Git: Create Branch...",
    },
];

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommandPalette {
    pub query: String,
    pub selected: usize,
}

impl CommandPalette {
    #[must_use]
    pub fn matches(&self) -> Vec<&'static Command> {
        let mut matches = COMMANDS
            .iter()
            .enumerate()
            .filter_map(|(order, command)| {
                fuzzy_score(command.label, &self.query).map(|score| (command, score, order))
            })
            .collect::<Vec<_>>();
        matches.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.2.cmp(&right.2)));
        matches.into_iter().map(|(command, _, _)| command).collect()
    }

    pub fn push(&mut self, character: char) {
        self.query.push(character);
        self.selected = 0;
    }

    pub fn backspace(&mut self) {
        self.query.pop();
        self.selected = 0;
    }

    pub fn select_next(&mut self) {
        let count = self.matches().len();
        self.selected = self.selected.saturating_add(1).min(count.saturating_sub(1));
    }

    pub fn select_previous(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }
}

fn fuzzy_score(candidate: &str, query: &str) -> Option<i64> {
    if query.is_empty() {
        return Some(0);
    }
    let candidate = candidate.as_bytes();
    let mut cursor = 0;
    let mut previous_match = None;
    let mut score = 0_i64;
    for needle in query.bytes().map(|byte| byte.to_ascii_lowercase()) {
        let offset = candidate[cursor..]
            .iter()
            .position(|byte| byte.to_ascii_lowercase() == needle)?;
        let index = cursor + offset;
        let boundary = index == 0 || !candidate[index - 1].is_ascii_alphanumeric();
        score += if previous_match == Some(index.saturating_sub(1)) {
            100
        } else if boundary {
            40
        } else {
            10
        };
        score -= i64::try_from(offset).unwrap_or(i64::MAX);
        previous_match = Some(index);
        cursor = index + 1;
    }
    Some(score - i64::try_from(candidate.len()).unwrap_or(i64::MAX) / 10)
}

#[cfg(test)]
mod tests {
    use super::CommandPalette;

    #[test]
    fn fuzzy_search_prefers_consecutive_and_word_start_matches() {
        let mut palette = CommandPalette::default();
        for character in "gfp".chars() {
            palette.push(character);
        }

        assert_eq!(palette.matches()[0].label, "Git: Fetch (Prune)");
    }

    #[test]
    fn search_is_case_insensitive_and_resets_selection() {
        let mut palette = CommandPalette {
            query: String::new(),
            selected: 4,
        };
        palette.push('P');
        palette.push('U');

        assert!(
            palette
                .matches()
                .iter()
                .all(|command| command.label.to_ascii_lowercase().contains('u'))
        );
        assert_eq!(palette.selected, 0);
    }
}
