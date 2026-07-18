#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DiffDocument {
    pub hunks: Vec<Hunk>,
    pub binary: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Hunk {
    pub header: String,
    pub old_start: u32,
    pub new_start: u32,
    pub blocks: Vec<DiffBlock>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiffBlock {
    Context(Vec<DiffLine>),
    Change {
        removed: Vec<DiffLine>,
        added: Vec<DiffLine>,
        alignment: Vec<LinePair>,
    },
    Meta(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffLine {
    pub old_number: Option<u32>,
    pub new_number: Option<u32>,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinePair {
    pub old: Option<DiffLine>,
    pub new: Option<DiffLine>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RowKind {
    Header,
    Context,
    Removed,
    Added,
    Changed,
    Conflict,
    Meta,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderLine {
    pub number: Option<u32>,
    pub text: String,
    pub kind: RowKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SideBySideRow {
    pub old: Option<RenderLine>,
    pub new: Option<RenderLine>,
    pub kind: RowKind,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProjectionOptions {
    pub mark_conflicts: bool,
}
