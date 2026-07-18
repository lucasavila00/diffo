mod engine;
mod types;

pub use engine::SyntaxHighlighter;
pub use types::{
    HighlightWindowRequest, HighlightedDiff, HighlightedLine, HighlightedWindow, LineRange, Rgb,
    StyledSpan,
};

#[cfg(test)]
mod tests;
