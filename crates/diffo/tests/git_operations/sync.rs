use std::{fmt::Write as _, os::unix::fs::PermissionsExt as _};

use super::support::*;

#[test]
fn equal_tips_still_fetch_and_show_the_finish_plan() -> Result<()> {
    let repository = TestRepository::new()?;
    let mut gate = diffo_e2e::GitProxy::new("fetch", diffo_e2e::GitGatePhase::Before)?;
    let path = gate.path()?;
    let mut screen = DiffoScreen::launch_with_env(
        diffo_binary()?,
        &repository.worktree,
        &[("PATH", path.as_os_str())],
    )?;

    start_sync(&mut screen)?;
    gate.wait_until_blocked()?;
    screen.wait_for_text("Fetching")?;
    gate.release()?;
    screen
        .wait_for_text("origin/master has no upstream-only")?
        .wait_for_text("master has no local-only commits.")?
        .wait_for_text("Plan: finish after fetch; the branches")?
        .wait_for_text("have the same tip.")?
        .wait_for_text("Fetched; already up to date.")?;

    Ok(())
}

#[test]
fn command_palette_exposes_sync_without_pull_or_push_actions() -> Result<()> {
    let repository = TestRepository::new()?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Char('1'))?
        .type_text("pull")?
        .wait_for_text("No matching commands")?
        .press(Key::Escape)?
        .wait_for_text_gone("Command Palette")?
        .press(Key::Char('1'))?
        .type_text("push")?
        .wait_for_text("No matching commands")?
        .press(Key::Escape)?
        .wait_for_text_gone("Command Palette")?
        .press(Key::Char('1'))?
        .type_text("sync")?
        .wait_for_text("Git: Sync")?;
    Ok(())
}

#[test]
fn ahead_branch_shows_the_plan_before_normal_push() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("local.txt"), "local\n")?;
    git(&repository.worktree, &["add", "local.txt"])?;
    git(&repository.worktree, &["commit", "-m", "Local commit"])?;
    let remote_before = remote_head(&repository)?;
    let mut gate = diffo_e2e::GitProxy::new("push", diffo_e2e::GitGatePhase::Before)?;
    let path = gate.path()?;
    let mut screen = DiffoScreen::launch_with_env(
        diffo_binary()?,
        &repository.worktree,
        &[("PATH", path.as_os_str())],
    )?;

    start_sync(&mut screen)?;
    gate.wait_until_blocked()?;
    screen
        .wait_for_text("origin/master has no upstream-only")?
        .wait_for_text("master has 1 local-only commit.")?
        .wait_for_text("Plan: push master.")?
        .wait_for_text("Pushing")?;
    assert_eq!(remote_head(&repository)?, remote_before);

    gate.release()?;
    screen.wait_for_text("Pushed master.")?;
    Ok(())
}

#[test]
fn clean_divergence_in_separate_hunks_rebases_without_a_merge() -> Result<()> {
    let repository = TestRepository::new()?;
    let shared = numbered_text(20)?;
    repository.commit_remote("shared.txt", &shared, "Shared base")?;
    git(&repository.worktree, &["pull", "--ff-only"])?;

    fs::write(
        repository.worktree.join("shared.txt"),
        shared.replace("line 5\n", "local 5\n"),
    )?;
    git(&repository.worktree, &["commit", "-am", "Local hunk"])?;
    repository.commit_remote(
        "shared.txt",
        &shared.replace("line 15\n", "remote 15\n"),
        "Remote hunk",
    )?;
    let mut screen = repository.screen()?;

    start_sync(&mut screen)?;
    screen.wait_for_text("Rebased 1 commit and pushed master.")?;

    let combined = fs::read_to_string(repository.worktree.join("shared.txt"))?;
    assert!(combined.contains("local 5\n"));
    assert!(combined.contains("remote 15\n"));
    assert!(
        git_output(
            &repository.worktree,
            &["rev-list", "--min-parents=2", "HEAD"]
        )?
        .is_empty()
    );
    assert_eq!(remote_head(&repository)?, local_head(&repository)?);
    Ok(())
}

#[test]
fn conflicting_divergence_aborts_rebase_and_never_pushes() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "local\n")?;
    git(&repository.worktree, &["commit", "-am", "Local conflict"])?;
    let local_before = local_head(&repository)?;
    let remote = repository.commit_remote("tracked.txt", "remote\n", "Remote conflict")?;
    let mut screen = repository.screen()?;

    start_sync(&mut screen)?;
    screen
        .wait_for_text("Rebase conflicted in 1 file and was")?
        .wait_for_text("aborted. Nothing was pushed.")?;

    assert_eq!(local_head(&repository)?, local_before);
    assert_eq!(remote_head(&repository)?, remote);
    assert_eq!(
        fs::read_to_string(repository.worktree.join("tracked.txt"))?,
        "local\n"
    );
    assert!(!rebase_in_progress(&repository.worktree)?);
    Ok(())
}

#[test]
fn no_upstream_stops_before_fetch() -> Result<()> {
    let repository = TestRepository::new()?;
    let fetch_marker = install_fetch_marker(&repository.worktree, repository.root.path())?;
    git(&repository.worktree, &["branch", "--unset-upstream"])?;
    let local_before = local_head(&repository)?;
    let mut screen = repository.screen()?;

    start_sync(&mut screen)?;
    screen
        .wait_for_text("sync requires a configured")?
        .wait_for_text("upstream")?;

    assert!(!fetch_marker.exists());
    assert_eq!(local_head(&repository)?, local_before);
    Ok(())
}

#[test]
fn detached_head_stops_before_fetch() -> Result<()> {
    let repository = TestRepository::new()?;
    let fetch_marker = install_fetch_marker(&repository.worktree, repository.root.path())?;
    git(&repository.worktree, &["checkout", "--detach"])?;
    let local_before = local_head(&repository)?;
    let mut screen = repository.screen()?;

    start_sync(&mut screen)?;
    screen
        .wait_for_text("sync requires an existing")?
        .wait_for_text("local branch; HEAD is detached")?;

    assert!(!fetch_marker.exists());
    assert_eq!(local_head(&repository)?, local_before);
    Ok(())
}

#[test]
fn unborn_branch_stops_before_fetch() -> Result<()> {
    let root = tempfile::tempdir()?;
    git(root.path(), &["init", "--bare", "remote.git"])?;
    git(root.path(), &["init", "--initial-branch=master", "work"])?;
    let worktree = root.path().join("work");
    configure(&worktree)?;
    git(&worktree, &["remote", "add", "origin", "../remote.git"])?;
    git(&worktree, &["config", "branch.master.remote", "origin"])?;
    git(
        &worktree,
        &["config", "branch.master.merge", "refs/heads/master"],
    )?;
    let fetch_marker = install_fetch_marker(&worktree, root.path())?;
    let mut screen = DiffoScreen::launch(diffo_binary()?, &worktree)?;

    start_sync(&mut screen)?;
    screen
        .wait_for_text("sync requires an existing")?
        .wait_for_text("current branch is unborn")?;

    assert!(!fetch_marker.exists());
    Ok(())
}

#[test]
fn dirty_worktree_stops_before_fetch_without_changing_it() -> Result<()> {
    let repository = TestRepository::new()?;
    let upstream_before = git_output(&repository.worktree, &["rev-parse", "origin/master"])?;
    repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    let fetch_marker = install_fetch_marker(&repository.worktree, repository.root.path())?;
    fs::write(repository.worktree.join("tracked.txt"), "dirty\n")?;
    let mut screen = repository.screen()?;

    start_sync(&mut screen)?;
    screen
        .wait_for_text("sync currently requires a")?
        .wait_for_text("clean worktree and index")?;

    assert!(!fetch_marker.exists());
    assert_eq!(
        git_output(&repository.worktree, &["rev-parse", "origin/master"])?,
        upstream_before
    );
    assert_eq!(
        fs::read_to_string(repository.worktree.join("tracked.txt"))?,
        "dirty\n"
    );
    Ok(())
}

#[test]
fn merge_in_progress_stops_before_fetch() -> Result<()> {
    let repository = repository_with_conflicting_topic()?;
    git_must_fail(&repository.worktree, &["merge", "topic"])?;
    let merge_head = git_output(&repository.worktree, &["rev-parse", "MERGE_HEAD"])?;

    assert_operation_in_progress_stops(&repository)?;

    assert_eq!(
        git_output(&repository.worktree, &["rev-parse", "MERGE_HEAD"])?,
        merge_head
    );
    Ok(())
}

#[test]
fn cherry_pick_in_progress_stops_before_fetch() -> Result<()> {
    let repository = repository_with_conflicting_topic()?;
    git_must_fail(&repository.worktree, &["cherry-pick", "topic"])?;
    let cherry_pick_head = git_output(&repository.worktree, &["rev-parse", "CHERRY_PICK_HEAD"])?;

    assert_operation_in_progress_stops(&repository)?;

    assert_eq!(
        git_output(&repository.worktree, &["rev-parse", "CHERRY_PICK_HEAD"])?,
        cherry_pick_head
    );
    Ok(())
}

#[test]
fn rebase_in_progress_stops_before_fetch() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "local\n")?;
    git(&repository.worktree, &["commit", "-am", "Local conflict"])?;
    repository.commit_remote("tracked.txt", "remote\n", "Remote conflict")?;
    git(&repository.worktree, &["fetch", "origin"])?;
    git_must_fail(&repository.worktree, &["rebase", "origin/master"])?;
    assert!(rebase_in_progress(&repository.worktree)?);

    assert_operation_in_progress_stops(&repository)?;

    assert!(rebase_in_progress(&repository.worktree)?);
    Ok(())
}

#[test]
fn local_merge_history_stops_after_fetch_before_rebase_or_push() -> Result<()> {
    let repository = TestRepository::new()?;
    git(&repository.worktree, &["switch", "-c", "topic"])?;
    fs::write(repository.worktree.join("topic.txt"), "topic\n")?;
    git(&repository.worktree, &["add", "topic.txt"])?;
    git(&repository.worktree, &["commit", "-m", "Topic commit"])?;
    git(&repository.worktree, &["switch", "master"])?;
    fs::write(repository.worktree.join("main.txt"), "main\n")?;
    git(&repository.worktree, &["add", "main.txt"])?;
    git(&repository.worktree, &["commit", "-m", "Main commit"])?;
    git(
        &repository.worktree,
        &["merge", "--no-ff", "topic", "-m", "Local merge"],
    )?;
    let local_before = local_head(&repository)?;
    let remote = repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    let fetch_marker = install_fetch_marker(&repository.worktree, repository.root.path())?;
    let mut screen = repository.screen()?;

    start_sync(&mut screen)?;
    screen
        .wait_for_text("sync cannot rebase local-only")?
        .wait_for_text("history containing merge commits")?;

    assert!(fetch_marker.exists());
    assert_eq!(local_head(&repository)?, local_before);
    assert_eq!(remote_head(&repository)?, remote);
    assert!(!rebase_in_progress(&repository.worktree)?);
    Ok(())
}

#[test]
fn rejected_push_after_rebase_keeps_rebased_commits_and_does_not_retry() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("local.txt"), "local\n")?;
    git(&repository.worktree, &["add", "local.txt"])?;
    git(&repository.worktree, &["commit", "-m", "Local commit"])?;
    let local_before = local_head(&repository)?;
    let remote = repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    let attempts = repository.root.path().join("push-attempts");
    let hook = repository.root.path().join("remote.git/hooks/pre-receive");
    fs::write(
        &hook,
        format!(
            "#!/bin/sh\nprintf attempt >> '{}'\nexit 1\n",
            attempts.display()
        ),
    )?;
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755))?;
    let mut screen = repository.screen()?;

    start_sync(&mut screen)?;
    screen.wait_for_text("Push rejected: rejected by remote hook")?;
    thread::sleep(Duration::from_millis(300));

    let local_after = local_head(&repository)?;
    assert_ne!(local_after, local_before);
    git(
        &repository.worktree,
        &["merge-base", "--is-ancestor", &remote, &local_after],
    )?;
    assert_eq!(remote_head(&repository)?, remote);
    assert_eq!(fs::read_to_string(attempts)?, "attempt");
    assert!(repository.worktree.join("local.txt").exists());
    assert!(repository.worktree.join("remote.txt").exists());
    assert!(!rebase_in_progress(&repository.worktree)?);
    Ok(())
}

#[test]
fn sync_fetches_the_remote_that_owns_the_configured_upstream() -> Result<()> {
    let repository = TestRepository::new()?;
    let upstream = repository.root.path().join("upstream.git");
    git(repository.root.path(), &["init", "--bare", "upstream.git"])?;
    git(
        &repository.worktree,
        &[
            "remote",
            "add",
            "upstream",
            upstream.to_str().context("upstream path")?,
        ],
    )?;
    git(&repository.worktree, &["push", "-u", "upstream", "master"])?;
    git(
        repository.root.path(),
        &["clone", "upstream.git", "upstream-seed"],
    )?;
    let seed = repository.root.path().join("upstream-seed");
    configure(&seed)?;
    fs::write(seed.join("upstream.txt"), "upstream\n")?;
    git(&seed, &["add", "upstream.txt"])?;
    git(&seed, &["commit", "-m", "Upstream commit"])?;
    git(&seed, &["push", "origin", "HEAD"])?;
    git(
        &repository.worktree,
        &["remote", "set-url", "origin", "../missing-origin.git"],
    )?;
    let mut screen = repository.screen()?;

    start_sync(&mut screen)?;
    screen.wait_for_text("Fast-forwarded master by 1 commit.")?;

    assert!(repository.worktree.join("upstream.txt").exists());
    assert_eq!(
        git_output(&repository.worktree, &["rev-parse", "HEAD"])?,
        git_output(&repository.worktree, &["rev-parse", "upstream/master"])?
    );
    Ok(())
}

fn start_sync(screen: &mut DiffoScreen) -> Result<()> {
    screen
        .press(Key::Char('1'))?
        .wait_for_text("Command Palette")?
        .type_text("sync")?
        .press(Key::Enter)?
        .wait_for_text_gone("Command Palette")?;
    Ok(())
}

fn install_fetch_marker(worktree: &Path, root: &Path) -> Result<PathBuf> {
    let marker = root.join(format!(
        "fetch-ran-{}",
        worktree.file_name().unwrap_or_default().to_string_lossy()
    ));
    let upload_pack = root.join(format!(
        "upload-pack-{}",
        worktree.file_name().unwrap_or_default().to_string_lossy()
    ));
    fs::write(
        &upload_pack,
        format!(
            "#!/bin/sh\ntouch '{}'\nexec git-upload-pack \"$@\"\n",
            marker.display()
        ),
    )?;
    fs::set_permissions(&upload_pack, fs::Permissions::from_mode(0o755))?;
    git(
        worktree,
        &[
            "config",
            "remote.origin.uploadpack",
            upload_pack.to_str().context("upload-pack path")?,
        ],
    )?;
    Ok(marker)
}

fn numbered_text(count: usize) -> Result<String> {
    let mut text = String::new();
    for line in 1..=count {
        writeln!(text, "line {line}").context("build numbered text")?;
    }
    Ok(text)
}

fn local_head(repository: &TestRepository) -> Result<String> {
    git_output(&repository.worktree, &["rev-parse", "HEAD"])
}

fn remote_head(repository: &TestRepository) -> Result<String> {
    git_output(
        &repository.root.path().join("remote.git"),
        &["rev-parse", "HEAD"],
    )
}

fn rebase_in_progress(worktree: &Path) -> Result<bool> {
    for name in ["rebase-merge", "rebase-apply"] {
        let path = git_output(worktree, &["rev-parse", "--git-path", name])?;
        if worktree.join(path).exists() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn repository_with_conflicting_topic() -> Result<TestRepository> {
    let repository = TestRepository::new()?;
    git(&repository.worktree, &["switch", "-c", "topic"])?;
    fs::write(repository.worktree.join("tracked.txt"), "topic\n")?;
    git(&repository.worktree, &["commit", "-am", "Topic conflict"])?;
    git(&repository.worktree, &["switch", "master"])?;
    fs::write(repository.worktree.join("tracked.txt"), "master\n")?;
    git(&repository.worktree, &["commit", "-am", "Master conflict"])?;
    Ok(repository)
}

fn assert_operation_in_progress_stops(repository: &TestRepository) -> Result<()> {
    let fetch_marker = install_fetch_marker(&repository.worktree, repository.root.path())?;
    let mut screen = repository.screen()?;

    start_sync(&mut screen)?;
    screen
        .wait_for_text("finish or abort the merge,")?
        .wait_for_text("rebase, or cherry-pick before syncing")?;

    assert!(!fetch_marker.exists());
    Ok(())
}
