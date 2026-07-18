use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use diffo_core::{ChangeKind, FileState, HeadState, UpstreamState};

use super::NO_CHANGE;

pub(super) struct ParsedStatus {
    pub(super) head: HeadState,
    pub(super) files: Vec<ParsedFile>,
    pub(super) upstream: Option<UpstreamState>,
}

pub(super) struct ParsedFile {
    pub(super) state: FileState,
    pub(super) index_status: char,
    pub(super) worktree_status: char,
}

pub(super) fn parse_status(output: &[u8]) -> Result<ParsedStatus> {
    let records = output.split(|byte| *byte == 0).collect::<Vec<_>>();
    let mut branch_name = None;
    let mut branch_oid = None;
    let mut upstream_name = None;
    let mut ahead = 0;
    let mut behind = 0;
    let mut files = Vec::new();
    let mut index = 0;

    while index < records.len() {
        let record = std::str::from_utf8(records[index]).context("git status was not UTF-8")?;
        index += 1;
        if record.is_empty() {
            continue;
        }

        if let Some(oid) = record.strip_prefix("# branch.oid ") {
            branch_oid = Some(oid.to_owned());
        } else if let Some(name) = record.strip_prefix("# branch.head ") {
            branch_name = Some(name.to_owned());
        } else if let Some(name) = record.strip_prefix("# branch.upstream ") {
            upstream_name = Some(name.to_owned());
        } else if let Some(counts) = record.strip_prefix("# branch.ab ") {
            let mut parts = counts.split_whitespace();
            ahead = parse_count(parts.next(), '+')?;
            behind = parse_count(parts.next(), '-')?;
        } else if record.starts_with("1 ") {
            let fields = record.splitn(9, ' ').collect::<Vec<_>>();
            let [_, xy, _, _, _, _, _, _, path] = fields.as_slice() else {
                bail!("invalid ordinary git status record: {record}");
            };
            files.push(parsed_file(path, None, xy)?);
        } else if record.starts_with("2 ") {
            let fields = record.splitn(10, ' ').collect::<Vec<_>>();
            let [_, xy, _, _, _, _, _, _, _, path] = fields.as_slice() else {
                bail!("invalid renamed git status record: {record}");
            };
            let old_path = records
                .get(index)
                .context("rename record has no old path")?;
            index += 1;
            let old_path = std::str::from_utf8(old_path).context("old path was not UTF-8")?;
            files.push(parsed_file(path, Some(old_path), xy)?);
        } else if record.starts_with("u ") {
            let fields = record.splitn(11, ' ').collect::<Vec<_>>();
            let path = fields.last().context("conflict record has no path")?;
            files.push(ParsedFile {
                state: empty_file(path, None, ChangeKind::Conflicted),
                index_status: 'U',
                worktree_status: 'U',
            });
        } else if let Some(path) = record.strip_prefix("? ") {
            files.push(ParsedFile {
                state: empty_file(path, None, ChangeKind::Untracked),
                index_status: NO_CHANGE,
                worktree_status: NO_CHANGE,
            });
        }
    }

    let head = parse_head(branch_name.as_deref(), branch_oid.as_deref())?;
    let upstream = upstream_name.map(|name| UpstreamState {
        name,
        ahead,
        behind,
    });
    Ok(ParsedStatus {
        head,
        files,
        upstream,
    })
}

fn parse_head(name: Option<&str>, oid: Option<&str>) -> Result<HeadState> {
    let name = name.context("Git status is missing branch.head")?;
    let oid = oid.context("Git status is missing branch.oid")?;
    if name == "(detached)" {
        if oid == "(initial)" {
            bail!("detached Git status has no commit");
        }
        Ok(HeadState::Detached {
            commit: oid.to_owned(),
        })
    } else if oid == "(initial)" {
        Ok(HeadState::Unborn {
            name: name.to_owned(),
        })
    } else {
        Ok(HeadState::Named {
            name: name.to_owned(),
            commit: oid.to_owned(),
        })
    }
}

fn parse_count(value: Option<&str>, prefix: char) -> Result<usize> {
    let value = value.context("branch count is missing")?;
    value
        .strip_prefix(prefix)
        .context("branch count has an invalid prefix")?
        .parse()
        .context("branch count is not a number")
}

fn parsed_file(path: &str, old_path: Option<&str>, xy: &str) -> Result<ParsedFile> {
    let mut statuses = xy.chars();
    let index_status = statuses.next().context("index status is missing")?;
    let worktree_status = statuses.next().context("worktree status is missing")?;
    let status = if index_status == NO_CHANGE {
        worktree_status
    } else {
        index_status
    };
    Ok(ParsedFile {
        state: empty_file(path, old_path, change_kind(status)?),
        index_status,
        worktree_status,
    })
}

fn empty_file(path: &str, old_path: Option<&str>, kind: ChangeKind) -> FileState {
    FileState {
        path: PathBuf::from(path),
        old_path: old_path.map(PathBuf::from),
        kind,
        staged: None,
        unstaged: None,
    }
}

fn change_kind(status: char) -> Result<ChangeKind> {
    match status {
        'A' => Ok(ChangeKind::Added),
        'M' | 'T' => Ok(ChangeKind::Modified),
        'D' => Ok(ChangeKind::Deleted),
        'R' => Ok(ChangeKind::Renamed),
        'C' => Ok(ChangeKind::Copied),
        'U' => Ok(ChangeKind::Conflicted),
        _ => bail!("unknown git file status: {status}"),
    }
}
