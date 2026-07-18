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
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
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
