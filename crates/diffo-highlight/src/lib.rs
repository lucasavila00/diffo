mod engine;
mod types;

pub const MAX_HIGHLIGHT_FILE_LINES: usize = 10_000;
pub const HIGHLIGHT_LOOKBEHIND_LINES: usize = 256;
pub const MAX_HIGHLIGHT_BYTES_PER_SIDE: usize = 512 * 1024;

pub use engine::SyntaxHighlighter;
pub use types::{
    HighlightWindowRequest, HighlightedDiff, HighlightedLine, HighlightedWindow, LineRange, Rgb,
    StyledSpan,
};

#[cfg(test)]
mod tests;
