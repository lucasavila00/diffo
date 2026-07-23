use super::*;

impl Workbench {
    pub(super) fn execute_everyday_command(&mut self, command: CommandId) -> bool {
        match command {
            DISCARD_ALL_COMMAND => self.open_discard_all(),
            STASH_COMMAND => self.set_modal(Modal::Everyday(EverydayModal::Input(
                InputModal::stash(),
            ))),
            APPLY_STASH_COMMAND => self.open_stash_picker(PickerPurpose::ApplyStash),
            DROP_STASH_COMMAND => self.open_stash_picker(PickerPurpose::DropStash),
            AMEND_COMMAND => self.open_amend(),
            UNDO_COMMIT_COMMAND => self.open_undo(),
            REVERT_COMMAND => self.open_revert(),
            RENAME_BRANCH_COMMAND => self.open_rename(),
            _ => return false,
        }
        true
    }

    pub(super) fn open_discard_file(&mut self, path: &std::path::Path) {
        let Some(file) = self
            .diff_model()
            .snapshot
            .files
            .iter()
            .find(|file| {
                file.path == path
                    && (file.unstaged.is_some() || file.kind == diffo_core::ChangeKind::Untracked)
            })
            .cloned()
        else {
            self.show_error(
                "Discard failed",
                "The selected file no longer has changes to discard",
            );
            return;
        };
        self.set_modal(Modal::Everyday(EverydayModal::Confirmation(
            ConfirmationModal::new(
                format!("Discard changes in {}?", file.path.display()),
                "Discard changes",
                RepositoryAction::Discard(Box::new(DiscardTarget { file })),
            ),
        )));
    }

    fn open_discard_all(&mut self) {
        let files = self
            .diff_model()
            .snapshot
            .files
            .iter()
            .filter(|file| {
                file.unstaged.is_some() || file.kind == diffo_core::ChangeKind::Untracked
            })
            .cloned()
            .collect::<Vec<_>>();
        if files.is_empty() {
            self.show_error(
                "Discard failed",
                "There are no unstaged or untracked changes",
            );
            return;
        }
        let untracked = files
            .iter()
            .filter(|file| file.kind == diffo_core::ChangeKind::Untracked)
            .count();
        let tracked = files.len().saturating_sub(untracked);
        self.set_modal(Modal::Everyday(EverydayModal::Confirmation(
            ConfirmationModal::new(
                format!(
                    "Discard {tracked} tracked and {untracked} untracked paths? Staged and ignored files are preserved."
                ),
                "Discard all",
                RepositoryAction::DiscardAll(Box::new(DiscardAllTarget { files })),
            ),
        )));
    }

    fn open_stash_picker(&mut self, purpose: PickerPurpose) {
        let query_id = self.next_query_id();
        self.set_modal(Modal::Everyday(EverydayModal::Picker(
            EverydayPicker::loading_stashes(query_id, purpose),
        )));
        self.pending_stash_query = Some(query_id);
    }

    fn open_amend(&mut self) {
        let (HeadState::Named { commit, .. }, Some(latest)) = (
            &self.diff_model().snapshot.head,
            self.diff_model().snapshot.recent_commits.first(),
        ) else {
            self.show_error(
                "Amend failed",
                "Amend requires an existing local branch commit",
            );
            return;
        };
        self.set_modal(Modal::Everyday(EverydayModal::Input(InputModal::amend(
            commit.clone(),
            latest.summary.clone(),
        ))));
    }

    fn open_undo(&mut self) {
        let HeadState::Named { commit, .. } = &self.diff_model().snapshot.head else {
            self.show_error(
                "Undo failed",
                "Undo requires an existing local branch commit",
            );
            return;
        };
        self.set_modal(Modal::Everyday(EverydayModal::Confirmation(
            ConfirmationModal::new(
                "Undo the last commit and keep its changes staged?".to_owned(),
                "Undo commit",
                RepositoryAction::UndoLastCommit(Box::new(UndoCommitTarget {
                    expected_head: commit.clone(),
                })),
            ),
        )));
    }

    fn open_revert(&mut self) {
        let commits = self.diff_model().snapshot.recent_commits.clone();
        self.set_modal(Modal::Everyday(EverydayModal::Picker(
            EverydayPicker::revert(&commits),
        )));
    }

    fn open_rename(&mut self) {
        let HeadState::Named { name, commit } = &self.diff_model().snapshot.head else {
            self.show_error(
                "Rename failed",
                "Rename requires an existing local branch",
            );
            return;
        };
        let query_id = self.next_query_id();
        let target = RenameBranchTarget {
            old_name: name.clone(),
            old_full_ref: format!("refs/heads/{name}"),
            object_id: commit.clone(),
            new_name: String::new(),
            had_upstream: self.diff_model().snapshot.upstream.is_some(),
        };
        self.set_modal(Modal::Everyday(EverydayModal::Input(InputModal::rename(
            query_id, target,
        ))));
        self.pending_branch_query = Some(query_id);
    }

    pub(super) fn handle_everyday_modal_event(&mut self, event: &Event, area: Rect) {
        let result = match self.modal.as_mut() {
            Some(Modal::Everyday(modal)) => modal.handle_event(event, area),
            _ => return,
        };
        match result {
            EverydayEvent::Consumed => {}
            EverydayEvent::Close => self.close_modal(),
            EverydayEvent::Quit => {
                self.should_quit = true;
                self.close_modal();
            }
            EverydayEvent::Run(action) => {
                self.close_modal();
                self.commands.enqueue(action);
            }
            EverydayEvent::Confirm(modal) => {
                self.set_modal(Modal::Everyday(EverydayModal::Confirmation(modal)));
            }
        }
    }

    pub fn take_stash_query(&mut self) -> Option<RepositoryQueryId> {
        self.pending_stash_query.take()
    }

    pub fn stashes_loaded(&mut self, query_id: RepositoryQueryId, stashes: Vec<StashEntry>) {
        if let Some(Modal::Everyday(EverydayModal::Picker(picker))) = self.modal.as_mut()
            && picker.query_id == Some(query_id)
            && matches!(
                picker.purpose,
                PickerPurpose::ApplyStash | PickerPurpose::DropStash
            )
        {
            picker.install_stashes(stashes);
        }
    }

    pub fn stashes_load_failed(&mut self, query_id: RepositoryQueryId, message: &str) {
        if self.modal.as_ref().is_some_and(
            |modal| matches!(modal, Modal::Everyday(everyday) if everyday.stash_query_id() == Some(query_id)),
        ) {
            self.close_modal();
            self.show_error("Could not load stashes", message);
        }
    }

    pub(super) fn everyday_branches_loaded(
        &mut self,
        query_id: RepositoryQueryId,
        branches: Vec<BranchRef>,
    ) -> bool {
        let Some(Modal::Everyday(EverydayModal::Input(input))) = self.modal.as_mut() else {
            return false;
        };
        let InputPurpose::Rename {
            query_id: expected, ..
        } = &input.purpose
        else {
            return false;
        };
        if *expected != query_id {
            return false;
        }
        input.install_branches(branches);
        true
    }

    fn next_query_id(&mut self) -> RepositoryQueryId {
        let query_id = RepositoryQueryId(self.next_query_id);
        self.next_query_id = self.next_query_id.saturating_add(1);
        query_id
    }
}
