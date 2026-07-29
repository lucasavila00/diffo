# ADR 0099: Refresh without optional Git locks

Status: Accepted

Refines [ADR 0012](0012-live-repository-refresh.md).

## Problem

Every repository refresh runs `git status`. By default, Git may update cached file
metadata in the index while answering that read. The update is optional, but it
still creates `.git/index.lock`.

That is a bad fit for Diffo. Refreshes happen in the background, and Diffo watches
Git metadata for changes. An optional lock can interfere with a real Git command
and can even cause another refresh.

## Decision

Run snapshot status as `git --no-optional-locks status`. Git still computes and
returns the current status; it simply does not write its cached observations back
to the index.

Keep normal Git locking for commands that actually change the repository. Do not
retry lock failures or remove lock files: a lock may belong to another live Git
process, and guessing otherwise risks corruption.

## Result

Passive refreshes no longer write the index or create an optional index lock.
Staging, commits, checkouts, and other mutations keep Git's required locking.

A real-Git test changes only a tracked file's timestamp and proves that collecting
a snapshot leaves the index unchanged.
