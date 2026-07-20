use anyhow::{Context, Result, bail};
use diffo_core::{BranchKind, BranchRef};

use super::GitRepositorySource;

const LOCAL_PREFIX: &str = "refs/heads/";
const REMOTE_PREFIX: &str = "refs/remotes/";

impl GitRepositorySource {
    pub(super) fn branch_refs(&self) -> Result<Vec<BranchRef>> {
        let output = self.git(&[
            "for-each-ref",
            "--format=%(refname)%00%(objectname)%00%(symref)%00%(committerdate:unix)%00",
            "refs/heads",
            "refs/remotes",
        ])?;
        parse_branch_refs(&output)
    }
}

fn parse_branch_refs(output: &[u8]) -> Result<Vec<BranchRef>> {
    let mut branches = Vec::new();
    for record in output
        .split(|byte| *byte == b'\n')
        .filter(|row| !row.is_empty())
    {
        let fields = record.split(|byte| *byte == 0).collect::<Vec<_>>();
        let [full_ref, object_id, symref, tip_commit_unix_seconds, empty] = fields.as_slice()
        else {
            bail!("invalid git branch record")
        };
        if !empty.is_empty() {
            bail!("invalid git branch record terminator")
        }
        let full_ref = std::str::from_utf8(full_ref).context("git branch ref was not UTF-8")?;
        let object_id =
            std::str::from_utf8(object_id).context("git branch object ID was not UTF-8")?;
        let symref = std::str::from_utf8(symref).context("git branch symref was not UTF-8")?;
        let tip_commit_unix_seconds = std::str::from_utf8(tip_commit_unix_seconds)
            .context("git branch commit time was not UTF-8")?;
        let (kind, name) = if let Some(name) = full_ref.strip_prefix(LOCAL_PREFIX) {
            (BranchKind::Local, name)
        } else if let Some(name) = full_ref.strip_prefix(REMOTE_PREFIX) {
            if !symref.is_empty() {
                continue;
            }
            (BranchKind::Remote, name)
        } else {
            bail!("git returned an unexpected branch ref: {full_ref}")
        };
        if name.is_empty() || object_id.is_empty() {
            bail!("git returned an incomplete branch ref")
        }
        branches.push(BranchRef {
            kind,
            name: name.to_owned(),
            full_ref: full_ref.to_owned(),
            object_id: object_id.to_owned(),
            tip_commit_unix_seconds: if tip_commit_unix_seconds.is_empty() {
                None
            } else {
                Some(
                    tip_commit_unix_seconds
                        .parse()
                        .context("git branch commit time was invalid")?,
                )
            },
        });
    }
    branches.sort_by(|left, right| {
        right
            .tip_commit_unix_seconds
            .cmp(&left.tip_commit_unix_seconds)
            .then_with(|| branch_rank(left.kind).cmp(&branch_rank(right.kind)))
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(branches)
}

const fn branch_rank(kind: BranchKind) -> u8 {
    match kind {
        BranchKind::Local => 0,
        BranchKind::Remote => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sorts_by_recency_and_hides_symbolic_remote_heads() {
        let output = b"refs/remotes/origin/recent\0ccc\0\x00400\0\nrefs/remotes/origin/topic\0bbb\0\x00300\0\nrefs/heads/topic\0aaa\0\x00300\0\nrefs/remotes/origin/HEAD\0aaa\0refs/remotes/origin/main\x00300\0\nrefs/heads/main\0aaa\0\x00300\0\nrefs/heads/undated\0ddd\0\0\0\n";

        let branches = parse_branch_refs(output).unwrap();

        assert_eq!(
            branches
                .iter()
                .map(|branch| (
                    branch.kind,
                    branch.name.as_str(),
                    branch.tip_commit_unix_seconds
                ))
                .collect::<Vec<_>>(),
            [
                (BranchKind::Remote, "origin/recent", Some(400)),
                (BranchKind::Local, "main", Some(300)),
                (BranchKind::Local, "topic", Some(300)),
                (BranchKind::Remote, "origin/topic", Some(300)),
                (BranchKind::Local, "undated", None),
            ]
        );
    }

    #[test]
    fn rejects_invalid_commit_times() {
        let output = b"refs/heads/main\0aaa\0\0not-a-time\0\n";

        let error = parse_branch_refs(output).unwrap_err();

        assert!(error.to_string().contains("commit time was invalid"));
    }
}
