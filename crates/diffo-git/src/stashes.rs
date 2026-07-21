use anyhow::{Context, Result, bail};
use diffo_core::StashEntry;

use super::GitRepositorySource;

impl GitRepositorySource {
    pub(super) fn stash_entries(&self) -> Result<Vec<StashEntry>> {
        let output = self.git(&["stash", "list", "--format=%gd%x00%H%x00%gs%x00"])?;
        parse_stashes(&output)
    }

    pub(super) fn remote_names(&self) -> Result<Vec<String>> {
        let output = self.git(&["remote"])?;
        let mut remotes = String::from_utf8(output)
            .context("git returned non-UTF-8 remote names")?
            .lines()
            .map(str::to_owned)
            .filter(|name| !name.is_empty())
            .collect::<Vec<_>>();
        remotes.sort();
        remotes.dedup();
        Ok(remotes)
    }
}

fn parse_stashes(output: &[u8]) -> Result<Vec<StashEntry>> {
    let mut entries = Vec::new();
    for record in output
        .split(|byte| *byte == b'\n')
        .filter(|row| !row.is_empty())
    {
        let fields = record.split(|byte| *byte == 0).collect::<Vec<_>>();
        let [name, object_id, summary, empty] = fields.as_slice() else {
            bail!("invalid git stash record")
        };
        if !empty.is_empty() {
            bail!("invalid git stash record terminator")
        }
        let name = std::str::from_utf8(name).context("git stash name was not UTF-8")?;
        let object_id =
            std::str::from_utf8(object_id).context("git stash object ID was not UTF-8")?;
        let summary = std::str::from_utf8(summary).context("git stash summary was not UTF-8")?;
        if !name.starts_with("stash@{") || object_id.is_empty() {
            bail!("git returned an invalid stash identity")
        }
        entries.push(StashEntry {
            name: name.to_owned(),
            object_id: object_id.to_owned(),
            summary: summary.to_owned(),
        });
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_machine_delimited_stashes() {
        let entries = parse_stashes(
            b"stash@{0}\0aaaaaaaa\0On main: first\0\nstash@{1}\0bbbbbbbb\0WIP on topic\0\n",
        )
        .unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "stash@{0}");
        assert_eq!(entries[0].object_id, "aaaaaaaa");
        assert_eq!(entries[1].summary, "WIP on topic");
    }
}
