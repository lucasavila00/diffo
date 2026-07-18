# ADR 0028: Split `diffo-app/src/model.rs`

Status: Proposed

## Decision

Keep `model.rs` for state types, `Model`, construction, and module exports.

- `model/navigation.rs`: file selection, context menu, scrolling, pane size.
- `model/commit.rs`: draft editing and primary action.
- `model/palette.rs`: command palette state.
- `model/staging.rs`: stage and unstage action creation.
- `model/repository.rs`: snapshot install and operation completion.
- `model/toast.rs`: toast queue and failure text.

Use separate `impl Model` blocks. Move tests beside their owner.
Do not change fields, messages, effects, or public exports.
