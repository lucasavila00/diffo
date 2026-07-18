# ADR 0051: Workbench-owned operation toasts

Status: Accepted

Refines [ADR 0018](0018-network-operation-feedback.md),
[ADR 0020](0020-operation-toasts.md), and
[ADR 0039](0039-independent-app-modes.md).

## Context

Operation toasts predate the Explorer, Search, and Diff activity split. Their queue,
expiry deadlines, rendering, and mouse handling remained attached to the Diff
activity. Repository commands are now available from every activity, so completing
an operation while Explorer or Search is active updates the Diff-owned queue but
does not render its toast.

An in-flight network activity and a toast also represent different state. The
network activity says that Fetch, Pull, or Push is still running. A toast reports a
completed or failed action. Sharing an owner or lifetime between them can make a
result disappear when the activity indicator stops.

## Decision

Make toasts a workbench overlay. The workbench owns one toast queue, its identifiers,
and its expiry deadlines. No activity model owns, copies, or synchronizes toast
state.

Repository result handling has independent effects:

- update repository and pending-operation state;
- distribute the new repository snapshot to activities that consume it; and
- enqueue the operation result or failure in the workbench toast queue.

Watcher-only repository refreshes never enqueue a toast. Stage and Unstage retain
their existing no-toast behavior. Toast text, severity, duplicate replacement,
three-item limit, three-second non-error expiry, and persistent errors remain as
specified by ADR 0020.

The workbench renders the queue over the active activity's content after that
activity renders. It performs toast hit testing before routing ordinary input to
the active activity, while modal input keeps priority. Clicking a toast dismisses
it from Explorer, Search, and Diff in the same way. Switching activities preserves
the queue and its deadlines.

Keep pending network feedback separate. Its spinner, operation label, border
animation, and action disabling describe in-flight work only. Starting or stopping
that feedback cannot clear, suppress, render, or extend a toast. Toast expiry is
driven by the workbench tick and does not depend on which activity is active.

This replaces the toast-state and toast-rendering ownership in ADR 0020. The toast
data and operation-to-message projection may remain in a shared application crate,
but the live queue belongs to the workbench rather than an activity.

## Alternatives

- Render the Diff activity's toast queue from every activity. Rejected because Diff
  would remain the hidden owner of product-wide UI state.
- Give every activity its own toast queue. Rejected because results would need to be
  copied or could change when the user switches activities.
- Treat the completed toast as the final network-activity frame. Rejected because
  pending feedback and result feedback have different lifetimes and error rules.

## Verification

- Complete and fail the same repository operation while Explorer and Diff are
  active; both render the same toast text and style.
- Switch activities while a toast is visible; the same toast remains visible and
  keeps its original expiry deadline.
- Click-dismiss and automatic expiry work from every activity; error toasts remain
  until dismissed or replaced.
- Starting and stopping network activity does not remove an existing toast, and a
  watcher-only snapshot does not create one.
- Existing queue ordering, duplicate replacement, maximum-size, rendering, and Git
  operation tests continue to pass.
