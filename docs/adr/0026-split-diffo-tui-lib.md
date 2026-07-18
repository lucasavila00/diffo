# ADR 0026: Split `diffo-tui/src/lib.rs`

Status: Proposed

## Decision

Keep `lib.rs` for `Renderer`, its public types, and top-level frame orchestration.

- `diff.rs`: buffer key, cache, worker, preparation, anchors.
- `diff_view.rs`: diff rows, syntax spans, hunk buttons, scrollbars.
- `files.rs`: file lists, commit composer, status bar.
- `overlays.rs`: palette, help, toasts, commit editor, context menu.
- `geometry.rs`: layouts, hit targets, scrollbar math.
- `style.rs`: colors, contrast, row styles.

Move tests beside their owner. Keep atomic buffer commit in `Renderer::prepare_frame`.
No behavior or public API change.
