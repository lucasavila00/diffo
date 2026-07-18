#![doc = include_str!("../README.md")]

mod parser;
mod projection;
mod types;

pub use parser::parse_unified_patch;
pub use projection::{
    inline_change_starts, inline_rows, inline_rows_with_options, side_by_side_change_starts,
    side_by_side_rows, side_by_side_rows_with_options,
};
pub use types::{
    DiffBlock, DiffDocument, DiffLine, Hunk, LinePair, ProjectionOptions, RenderLine, RowKind,
    SideBySideRow,
};

#[cfg(test)]
mod tests;
