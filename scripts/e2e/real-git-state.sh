#!/usr/bin/env bash
set -euo pipefail

workspace_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
test_root=$(mktemp -d)
trap 'rm -rf "$test_root"' EXIT

git init --bare --quiet "$test_root/origin.git"
git clone --quiet "$test_root/origin.git" "$test_root/seed"
git -C "$test_root/seed" config user.name "Diffo Test"
git -C "$test_root/seed" config user.email "diffo@example.invalid"
printf 'base\n' > "$test_root/seed/tracked.txt"
git -C "$test_root/seed" add tracked.txt
git -C "$test_root/seed" commit --quiet -m "Base commit"
git -C "$test_root/seed" push --quiet -u origin HEAD

git clone --quiet "$test_root/origin.git" "$test_root/work"
git -C "$test_root/work" config user.name "Diffo Test"
git -C "$test_root/work" config user.email "diffo@example.invalid"
printf 'committed\n' > "$test_root/work/committed.txt"
git -C "$test_root/work" add committed.txt
git -C "$test_root/work" commit --quiet -m "Unpushed commit"
printf 'staged\n' >> "$test_root/work/tracked.txt"
git -C "$test_root/work" add tracked.txt
printf 'unstaged\n' >> "$test_root/work/tracked.txt"
printf 'untracked\n' > "$test_root/work/untracked.txt"

dump="$test_root/snapshot.ron"
DIFFO_REPOSITORY="$test_root/work" cargo run \
    --quiet \
    --manifest-path "$workspace_root/Cargo.toml" \
    --package git-diff-tui \
    --example dump_state > "$dump"

grep -q 'name: Some("master")' "$dump"
grep -q 'path: "tracked.txt"' "$dump"
grep -q 'staged: Some' "$dump"
grep -q 'unstaged: Some' "$dump"
grep -q 'path: "untracked.txt"' "$dump"
grep -q 'kind: Untracked' "$dump"
grep -q 'summary: "Unpushed commit"' "$dump"
grep -q 'ahead: 1' "$dump"

echo "real Git state E2E passed"
