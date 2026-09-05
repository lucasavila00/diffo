# ADR 0076: Order checkout branches by recent commit

Defines the branch ordering used by [ADR 0037](0037-git-checkout-to.md).

## Context

The checkout picker orders local branches before remote branches and sorts each
group by name. That makes an active branch no easier to find than an old branch
with an alphabetically earlier name.

Git does not have one portable "last branch edit" timestamp. Reflogs are local,
optional, and unsuitable for remote-tracking refs. The committer timestamp of
the commit at a branch tip is available for both local and remote-tracking refs
in the existing `git for-each-ref` query. It represents the most useful common
approximation: when the latest change on that branch was committed. For
remote-tracking refs, it describes the last commit known to the local
repository, not activity that has not been fetched.

## Decision

Read each branch tip's committer timestamp in the existing machine-delimited
`git for-each-ref` invocation. Keep branch discovery to one Git process. Carry
the timestamp as branch metadata; it is informational and must never be used as
part of a checkout target.

Order the unfiltered checkout list by descending tip committer timestamp across
local and remote branches. Put branches without a usable timestamp after dated
branches. Break equal-timestamp ties by putting local branches before remote
branches, then by branch name. The current branch and its tracked remote remain
visible but disabled, and the first enabled row remains selected.

Fuzzy-match score remains the primary order while the user has entered search
text. Use the recency order to break equal search scores, so an empty query and
equally good matches both favor recent branches.

Show a muted, right-aligned relative age for the tip commit on each checkout
row, such as `12m ago`, `3h ago`, or `8d ago`. Describe commit time, not branch
activity or fetch time. Use the largest whole unit: `now` below one minute,
minutes below one hour, hours below one day, days below 30 days, months of 30
days below one year, and years of 365 days thereafter. Format ages from one
captured wall-clock value when the loaded branch set is installed. Do not run a
timer or redraw merely to advance the labels; reopening or reloading the picker
refreshes them. Treat future timestamps caused by clock skew as `now`, and show
no age when the timestamp is unavailable.

Keep the branch name as the row's primary content. Truncate the name before the
age when both fit, but omit the age when the available width cannot show useful
branch-name content beside it. Use shared search-picker and semantic chrome
styling; checkout rendering still performs no Git work.

## Consequences

Recently committed remote branches can appear before older local branches. Local
and remote names that point to the same commit remain predictably ordered with
the local entry first.

Commit timestamps can be rewritten and can reflect a misconfigured clock, so
this is a relevance hint rather than an audit record. Moving a branch to an
older commit can also make it appear older even if the ref move itself just
happened.

The shared search picker needs optional trailing row metadata without changing
other picker users. The branch model gains optional tip-time information, while
checkout identity continues to use only branch kind, full ref, and object ID.
