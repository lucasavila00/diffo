# ADR 0084: Acknowledge every error in one modal

Refines [ADR 0019](0019-commit-message-modal.md),
[ADR 0020](0020-operation-toasts.md),
[ADR 0051](0051-workbench-operation-toasts.md), and
[ADR 0071](0071-separate-commit-and-sync-controls.md).

## Problem

Diffo reports errors as persistent toasts. A toast is appropriate for a
transient success or informational result, but an error requires the user to
notice it, understand it, and acknowledge it before continuing.

The failed-commit flow also reopens the commit-message editor. Commit failures
can come from the repository, environment, or hooks and do not reliably mean the
message should be edited. Reopening the editor presents the wrong recovery
action and can obscure the error.

## Decision

Use one error UX throughout Diffo: every user-visible error opens the same
acknowledgement modal. Do not render error toasts.

The modal contains:

- the action or context in its title;
- terminal-safe summary and detail text;
- one `[ OK ]` button; and
- visible `Esc: close` help and a top-right close button.

`Enter`, Esc, clicking OK, and clicking the close button all dismiss the modal.
Clicking outside does not dismiss it. Global quit remains available. All other
input is captured while the modal is open.

Keep success and informational results as non-blocking, automatically expiring
toasts. Remove `Error` from the toast kinds and route every existing error-toast
producer through the shared error-modal state. This includes repository command
failures, application-update failures, Explorer and picker failures, validation
failures that currently become toasts, and other workbench errors. Inline
validation that is part of an open form remains inline until submission; it is
not a separate reported error.

Keep at most one error modal visible. If another error arrives before it is
dismissed, append it to a FIFO pending-error queue. Dismissing an error
immediately shows the next one. Identical pending errors are coalesced. Do not
replace or lose an error that the user has not acknowledged.

An error result replaces any product modal whose submitted work produced that
error. It does not replace an active Git authentication, host confirmation, or
other system prompt; queue the error until that prompt closes. Opening another
product modal is blocked while an error is visible.

## Failed commits

Submitting a commit closes the commit-message editor while Git runs. If the
commit fails, open the shared error modal and keep the commit editor closed.

Preserve the submitted draft and cursor position. After acknowledging the error,
return to normal Diff input. The user may explicitly reopen the preserved draft
with `m` or the commit-message field. Do not infer that editing or retrying the
commit is the required recovery action.

A successful commit continues to clear the draft after the successful repository
snapshot is installed. Repository refreshes and unrelated results must never
reopen the commit editor.

## Alternatives

- Keep persistent error toasts. Rejected because they allow interaction to
  continue before the error has been acknowledged and compete with the affected
  content for attention.
- Reopen the commit editor only for hook rejection. Rejected because a hook can
  enforce worktree, signature, or repository policy requirements; rejection does
  not reliably mean the message should change.
- Add error-specific dialogs for each feature. Rejected because one
  presentation, input, queuing, and dismissal path is simpler and keeps error
  behavior consistent.

## Consequences

Every error interrupts the current product interaction until acknowledged. This
is deliberately more intrusive than a toast, but predictable: users learn one
error surface and errors cannot disappear or be overlooked behind other work.

Success and informational feedback remain lightweight. Failed commits keep their
message without forcing message editing into the recovery flow.

## Verification

- Pure state tests cover opening, FIFO queuing, duplicate coalescing, dismissal,
  and showing the next pending error.
- Input and hit-target tests prove that Enter, Esc, OK, and the close button
  dismiss; outside clicks and unrelated input do not.
- Renderer tests cover terminal-safe title, summary, and detail text at narrow
  and normal terminal sizes.
- Tests enumerate every error producer and prove it reaches the shared modal
  rather than a toast; toast tests prove only success and informational kinds
  remain.
- State tests prove that system prompts defer errors and that visible errors
  block other product modals.
- A deterministic frame-traced PTY regression submits a commit that Git rejects
  and proves the error modal appears, the commit editor does not reopen in any
  frame, and the submitted draft is preserved when reopened explicitly.
- Existing successful-commit tests continue to prove that success clears the
  draft.
- `make all` passes.
