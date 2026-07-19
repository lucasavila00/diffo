use crate::{
    ChangeRegion, DiffBlock, DiffDocument, ProjectionOptions, RenderLine, RowKind, SideBySideRow,
};

#[must_use]
pub fn inline_rows(document: &DiffDocument) -> Vec<RenderLine> {
    inline_rows_with_options(document, ProjectionOptions::default())
}

#[must_use]
pub fn inline_rows_with_options(
    document: &DiffDocument,
    options: ProjectionOptions,
) -> Vec<RenderLine> {
    let mut rows = Vec::new();
    for hunk in &document.hunks {
        rows.push(RenderLine {
            number: None,
            text: hunk.header.clone(),
            kind: RowKind::Header,
        });
        for block in &hunk.blocks {
            match block {
                DiffBlock::Context(lines) => rows.extend(lines.iter().map(|line| RenderLine {
                    number: line.new_number,
                    text: line.text.clone(),
                    kind: line_kind(&line.text, RowKind::Context, options),
                })),
                DiffBlock::Change { removed, added, .. } => {
                    rows.extend(removed.iter().map(|line| RenderLine {
                        number: line.old_number,
                        text: line.text.clone(),
                        kind: line_kind(&line.text, RowKind::Removed, options),
                    }));
                    rows.extend(added.iter().map(|line| RenderLine {
                        number: line.new_number,
                        text: line.text.clone(),
                        kind: line_kind(&line.text, RowKind::Added, options),
                    }));
                }
                DiffBlock::Meta(text) => rows.push(RenderLine {
                    number: None,
                    text: text.clone(),
                    kind: RowKind::Meta,
                }),
            }
        }
    }
    rows
}

#[must_use]
pub fn side_by_side_rows(document: &DiffDocument) -> Vec<SideBySideRow> {
    side_by_side_rows_with_options(document, ProjectionOptions::default())
}

#[must_use]
pub fn side_by_side_rows_with_options(
    document: &DiffDocument,
    options: ProjectionOptions,
) -> Vec<SideBySideRow> {
    let mut rows = Vec::new();
    for hunk in &document.hunks {
        let header = RenderLine {
            number: None,
            text: hunk.header.clone(),
            kind: RowKind::Header,
        };
        rows.push(SideBySideRow {
            old: Some(header.clone()),
            new: Some(header),
            kind: RowKind::Header,
        });
        for block in &hunk.blocks {
            match block {
                DiffBlock::Context(lines) => rows.extend(lines.iter().map(|line| SideBySideRow {
                    old: Some(RenderLine {
                        number: line.old_number,
                        text: line.text.clone(),
                        kind: line_kind(&line.text, RowKind::Context, options),
                    }),
                    new: Some(RenderLine {
                        number: line.new_number,
                        text: line.text.clone(),
                        kind: line_kind(&line.text, RowKind::Context, options),
                    }),
                    kind: RowKind::Context,
                })),
                DiffBlock::Change { alignment, .. } => {
                    rows.extend(alignment.iter().map(|pair| SideBySideRow {
                        old: pair.old.as_ref().map(|line| RenderLine {
                            number: line.old_number,
                            text: line.text.clone(),
                            kind: line_kind(&line.text, RowKind::Removed, options),
                        }),
                        new: pair.new.as_ref().map(|line| RenderLine {
                            number: line.new_number,
                            text: line.text.clone(),
                            kind: line_kind(&line.text, RowKind::Added, options),
                        }),
                        kind: RowKind::Changed,
                    }));
                }
                DiffBlock::Meta(text) => rows.push(SideBySideRow {
                    old: Some(RenderLine {
                        number: None,
                        text: text.clone(),
                        kind: RowKind::Meta,
                    }),
                    new: None,
                    kind: RowKind::Meta,
                }),
            }
        }
    }
    rows
}

#[must_use]
pub fn inline_change_regions(rows: &[RenderLine]) -> Vec<ChangeRegion> {
    change_regions(rows.iter().map(|row| row.kind))
}

#[must_use]
pub fn side_by_side_change_regions(rows: &[SideBySideRow]) -> Vec<ChangeRegion> {
    change_regions(rows.iter().map(|row| row.kind))
}

fn change_regions(kinds: impl Iterator<Item = RowKind>) -> Vec<ChangeRegion> {
    let mut current = None;
    let mut regions = Vec::new();
    for (index, kind) in kinds.enumerate() {
        let changed = matches!(
            kind,
            RowKind::Removed | RowKind::Added | RowKind::Changed | RowKind::Conflict
        );
        if changed {
            current
                .get_or_insert(ChangeRegion {
                    first: index,
                    last: index,
                })
                .last = index;
        } else if let Some(region) = current.take() {
            regions.push(region);
        }
    }
    if let Some(region) = current {
        regions.push(region);
    }
    regions
}

fn line_kind(text: &str, fallback: RowKind, options: ProjectionOptions) -> RowKind {
    if options.mark_conflicts
        && (text.starts_with("<<<<<<<")
            || text.starts_with("|||||||")
            || text.starts_with("=======")
            || text.starts_with(">>>>>>>"))
    {
        RowKind::Conflict
    } else {
        fallback
    }
}
