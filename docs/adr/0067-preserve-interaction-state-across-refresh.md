# ADR 0067: Keep user state during refresh

Status: Accepted

Refines [ADR 0012](0012-live-repository-refresh.md) and
[ADR 0037](0037-git-checkout-to.md).

## Problem

In `Git: Checkout to...`, Down moves the selection. Then the screen flashes and the
first branch is selected again.

Cause: a repository refresh replaces the picker items. Replacement resets selection
and scroll. The key works. The refresh undoes it.

Tests missed this ordering. They test input and refresh separately.

## Decision

Background refresh must not reset user state.

- Data owns items, order, labels, enabled state, and payloads.
- User owns query, selection, and scroll while the control is open.
- Track selection by stable identity. For a branch: kind plus full ref.
- Do not track selection by row, label, or object ID.
- Initial load may reset state. Later refreshes must reconcile state.
- If the selected item still exists, is enabled, and matches the query, keep it.
- Use its newest payload after refresh.
- If it is gone or disabled, select the first enabled match.
- If there is no match, clear selection.
- Update items, selection, and scroll in one commit.

This rule applies to every control that receives background data while the user can
navigate, type, expand, or scroll.

## Tests

- Down, refresh, Enter must choose the branch selected by the user.
- Test reorder, new payload, removal, disabled selection, and empty results.
- Add a real-Git PTY test with keyboard input interleaved with a repository refresh.
- Every future live control needs one test that interleaves input and refresh.
