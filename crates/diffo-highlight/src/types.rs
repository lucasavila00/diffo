use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rgb {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StyledSpan {
    pub text: String,
    pub foreground: Rgb,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HighlightedLine {
    pub spans: Vec<StyledSpan>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HighlightedDiff {
    pub old: BTreeMap<u32, HighlightedLine>,
    pub new: BTreeMap<u32, HighlightedLine>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LineRange {
    pub start: u32,
    pub end: u32,
}

impl LineRange {
    #[must_use]
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    #[must_use]
    pub fn contains(self, line: u32) -> bool {
        line >= self.start && line <= self.end
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HighlightWindowRequest {
    pub old: Option<LineRange>,
    pub new: Option<LineRange>,
    pub lookbehind_lines: usize,
    pub maximum_bytes_per_side: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HighlightedWindow {
    pub styles: HighlightedDiff,
    pub old_coverage: Option<LineRange>,
    pub new_coverage: Option<LineRange>,
    pub old_lines_processed: usize,
    pub new_lines_processed: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HighlightedTextWindow {
    pub styles: BTreeMap<u32, HighlightedLine>,
    pub coverage: Option<LineRange>,
    pub lines_processed: usize,
}
