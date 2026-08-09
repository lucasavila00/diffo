# ADR 0071: Modal Commit and global Sync controls

## Problem

Commit and Sync are different actions.

One changing primary button hides Sync. Commit + Push mixes local and remote
work. Commit-message editing has no key. Sync has no direct global key.

## Proposal

Remove the changing primary button.

Commit stays local and fixed:

- show one `[ Commit (Enter) ]` button beneath the commit-message field;
- the button never changes into Sync;
- show `m` beside the commit-message field and use it to open the commit-message
  modal;
- the modal has `[ Commit (Enter) ]` and `[ Cancel (Esc) ]`;
- `Enter` commits inside the modal;
- `Enter` also commits from normal Diff mode, using the draft or generated
  message;
- the fixed button uses the same draft or generated message.

Sync is global:

- end the shared footer with `[ Commands (1 / F1) ]`, `[ Help (2 / F2) ]`, and
  `[ Sync (9 / F9) ]` buttons, with Sync at the far right;
- show it in every activity;
- `9` and `F9` run the same action from every activity;
- open modals keep input priority; `9` and `F9` do not bypass them;
- disable button and key together when Sync cannot run.

Remove file-navigation `w` and `s`. Keep `j` for previous file. Keep `k` / `l`
for next file.

Feature letter keys belong to the active activity. Keep global action keys in
number and function-key pairs: `1` / `F1`, `2` / `F2`, and `9` / `F9`.

All character shortcuts stay lowercase. Help comes from the real binding
registry.

## Boundary

Commit creates a local commit. Sync reconciles and publishes under
[ADR 0070](0070-rebase-unpushed-work-when-syncing.md). No Commit + Push action.

If accepted, this replaces the one-primary-button part of
[ADR 0017](0017-commit-composer-and-primary-action.md).
