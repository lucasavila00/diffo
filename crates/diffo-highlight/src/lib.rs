mod engine;
mod types;

pub use engine::SyntaxHighlighter;
pub use types::{HighlightedDiff, HighlightedLine, Rgb, StyledSpan};

#[cfg(test)]
mod tests;
