# ADR 0033: File-list scrollbars

## Decision

Add independent vertical scrollbars to the Staged and Changes boxes.

- Show only on overflow.
- Keep one offset per box in the app model.
- Clamp offsets after refresh, resize, stage, and unstage.
- Click, drag, and wheel scroll only the pointed box. Scrolling does not select
  a file.
- Reserve one column inside the right border. Do not overlap file actions.
- Use the visible offset for rendering and hit-testing.
- No horizontal file-list scrolling.
