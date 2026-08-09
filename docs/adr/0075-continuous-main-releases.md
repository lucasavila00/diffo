# ADR 0075: Publish every main commit from a tagless release branch

Supersedes the GitHub Releases authority, tag trigger, and tag-derived version
decisions in [ADR 0070](0070-publish-and-self-update-diffo.md).

## Context

Diffo releases currently require a maintainer to create and push an immutable
stable SemVer tag. That manual gate means commits on `main` are not immediately
available through the update channel.

GitHub Releases cannot provide a genuinely tagless channel because every GitHub
Release has a backing tag. GitHub Actions artifacts are also unsuitable for the
public updater because they are temporary and do not provide the existing fixed
raw-asset interface.

The updater still needs an ordered stable SemVer value: schema 1 accepts only
stable SemVer and installs only a strictly newer version. The publication
transport does not need to use that version as a Git ref.

## Decision

Run the release workflow on every push to `main`. Do not create, consume, or
require release tags.

For each checked-out main commit, derive the release version as
`<workspace-major>.<workspace-minor>.<mainline-commit-count>`. Count
first-parent commits from the complete repository history. Embed that version in
the executable and write the same version to schema-1 update metadata. Also
embed the exact checked-out source commit SHA and publish it as additive
schema-1 metadata. The version is only an update-ordering key; interfaces
identify builds by source commit SHA.

Publish exactly the GNU/Linux executable, schema-1 metadata, and `SHA256SUMS` as
the root tree of a new parentless commit. Force-update the dedicated `release`
branch to that commit. This atomically replaces the published artifact set
without retaining source files or release history on the branch.

Fetch updates from the fixed raw-content URL for the `release` branch. This
replaces the GitHub latest-release URL. Existing binaries using the old URL will
not discover new releases; this one-time update-channel break is accepted.

Keep the installer on `main` pointed at the same raw release-branch artifacts.
Running the installer when `/usr/local/bin/diffo` already exists replaces it
with the latest verified binary. This is the manual migration path for clients
built with the old update endpoint.

Retain the target, protocol, verification, replacement behavior, and
release-build scope from ADR 0070. CI continues to own repository validation;
the release workflow builds publication artifacts without repeating `make all`.

## Consequences

Every push to `main` publishes an installable version without creating any tag.
Release versions remain stable SemVer values that the updater can order. The
release-branch commit message records both the version and exact source commit.

Old clients do not migrate automatically, but rerunning the public installation
command upgrades them directly to the tagless channel without requiring an
uninstall.

The workspace major or minor version must never move backwards. Main history
must not be rewritten to a lower first-parent commit count within the same
release line. A workspace major or minor bump starts a new, higher release line
while preserving the automatically derived patch version.

The `release` branch is generated output. Its history is intentionally replaced,
so it must not be used for development or protected against the workflow's force
update.

## Verification

- Verify the workflow triggers for pushes to `main` and has no tag trigger or
  release creation command.
- Verify a full-history checkout produces a stable SemVer version from the
  workspace release line and first-parent commit count.
- Verify the same version is embedded in the executable and schema-1 metadata.
- Verify the exact checked-out source commit SHA is embedded in the executable
  and schema-1 metadata.
- Verify the generated parentless commit contains only the three publication
  files and is force-pushed to `refs/heads/release`.
- Verify the updater's fixed endpoint reads from the raw `release` branch.
- Verify the installer downloads from that branch and replaces an existing
  destination.
- Complete repository validation with `make all`.
