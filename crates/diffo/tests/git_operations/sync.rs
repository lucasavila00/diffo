use std::{fmt::Write as _, os::unix::fs::PermissionsExt as _};

use super::support::*;

#[derive(Deserialize)]
struct ProtectedPushFrame {
    input_events: Vec<String>,
    protected_push_prompt: bool,
    head: String,
}

#[derive(Deserialize)]
struct DirtySyncFrame {
    input_events: Vec<String>,
    head: String,
    repository_files: Vec<String>,
}

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
    let trace_path = repository.root.path().join("protected-push-frames.ronl");
    let mut gate = diffo_e2e::GitProxy::new("push", diffo_e2e::GitGatePhase::Before)?;
    let path = gate.path()?;
    let mut screen = DiffoScreen::launch_with_env(
        diffo_binary()?,
        &repository.worktree,
        &[
            ("PATH", path.as_os_str()),
            ("DIFFO_TRACE_FRAMES", trace_path.as_os_str()),
        ],
    )?;

    start_sync(&mut screen)?;
    confirm_protected_push(&mut screen, 1, "origin/master")?;
    gate.wait_until_blocked()?;
    screen
        .wait_for_text("origin/master has no upstream-only")?
        .wait_for_text("master has 1 local-only commit.")?
        .wait_for_text("Plan: push master.")?
        .wait_for_text("Pushing")?;
    assert_eq!(remote_head(&repository)?, remote_before);

    gate.release()?;
    screen
        .wait_for_text("Pushed master.")?
        .press(Key::Char('q'))?
        .wait_for_exit()?;
    let trace = fs::read_to_string(&trace_path).context("read protected push frame trace")?;
    let frames = trace
        .lines()
        .map(ron::from_str::<ProtectedPushFrame>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let modal = frames
        .iter()
        .position(|frame| frame.protected_push_prompt)
        .with_context(|| format!("trace has no protected-push modal frame:\n{trace}"))?;
    assert!(frames[modal..].iter().any(|frame| {
        frame
            .input_events
            .iter()
            .any(|event| event.contains("Right"))
    }));
    Ok(())
}

#[test]
fn cancelling_protected_push_installs_the_fetched_snapshot_without_moving_branches() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("local.txt"), "local\n")?;
    git(&repository.worktree, &["add", "local.txt"])?;
    git(&repository.worktree, &["commit", "-m", "Local commit"])?;
    let local_before = local_head(&repository)?;
    let remote = repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    let trace_path = repository.root.path().join("cancelled-push-frames.ronl");
    let mut screen = DiffoScreen::launch_with_env(
        diffo_binary()?,
        &repository.worktree,
        &[("DIFFO_TRACE_FRAMES", trace_path.as_os_str())],
    )?;

    start_sync(&mut screen)?;
    screen
        .wait_for_text("Confirm push")?
        .wait_for_text("Push 1 commit directly to origin/master?")?
        .press(Key::Enter)?
        .wait_for_text_gone("Confirm push")?
        .wait_for_text("1 1")?;

    assert_eq!(local_head(&repository)?, local_before);
    assert_eq!(remote_head(&repository)?, remote);
    assert_eq!(
        git_output(&repository.worktree, &["rev-parse", "origin/master"])?,
        remote
    );
    screen.press(Key::Char('q'))?.wait_for_exit()?;
    let trace = fs::read_to_string(&trace_path).context("read cancelled push frame trace")?;
    let frames = trace
        .lines()
        .map(ron::from_str::<ProtectedPushFrame>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let modal = frames
        .iter()
        .position(|frame| frame.protected_push_prompt)
        .with_context(|| format!("trace has no protected-push modal frame:\n{trace}"))?;
    assert!(frames[modal..].iter().any(|frame| {
        !frame.protected_push_prompt
            && frame.head.ends_with(&local_before)
            && frame
                .input_events
                .iter()
                .any(|event| event.contains("Enter"))
    }));
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
    confirm_protected_push(&mut screen, 1, "origin/master")?;
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
    confirm_protected_push(&mut screen, 1, "origin/master")?;
    screen
        .wait_for_text("Rebase conflicted in 1 file and was")?
        .wait_for_text("aborted. Nothing was")?
        .wait_for_text("pushed.")?
        .wait_for_text("Git exit status: 1")?
        .wait_for_text("stderr:")?;

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
fn missing_upstream_is_repaired_by_one_frame_traced_sync() -> Result<()> {
    let repository = TestRepository::new()?;
    let fetch_marker = install_fetch_marker(&repository.worktree, repository.root.path())?;
    git(&repository.worktree, &["branch", "--unset-upstream"])?;
    let local_before = local_head(&repository)?;
    let trace_path = repository.root.path().join("missing-upstream-frames.ronl");
    let mut screen = DiffoScreen::launch_with_env(
        diffo_binary()?,
        &repository.worktree,
        &[("DIFFO_TRACE_FRAMES", trace_path.as_os_str())],
    )?;

    start_sync(&mut screen)?;
    screen
        .wait_for_text("Fetched; already up to date.")?
        .press(Key::Char('q'))?
        .wait_for_exit()?;

    assert!(fetch_marker.exists());
    assert_eq!(local_head(&repository)?, local_before);
    assert_eq!(
        git_output(
            &repository.worktree,
            &["rev-parse", "--abbrev-ref", "@{upstream}"]
        )?,
        "origin/master"
    );
    let trace = fs::read_to_string(&trace_path).context("read missing-upstream frame trace")?;
    let frames = trace
        .lines()
        .map(ron::from_str::<DirtySyncFrame>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    assert!(frames.iter().any(|frame| {
        frame
            .input_events
            .iter()
            .any(|event| event.contains("Char('1')"))
    }));
    assert!(
        frames
            .iter()
            .all(|frame| frame.head.ends_with(&local_before))
    );
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
        .wait_for_text("current branch is")?
        .wait_for_text("unborn")?;

    assert!(!fetch_marker.exists());
    Ok(())
}

#[test]
fn dirty_worktree_fast_forwards_unrelated_files_atomically() -> Result<()> {
    let repository = TestRepository::new()?;
    let remote = repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    let fetch_marker = install_fetch_marker(&repository.worktree, repository.root.path())?;
    fs::write(repository.worktree.join("tracked.txt"), "dirty\n")?;
    let trace_path = repository
        .root
        .path()
        .join("dirty-fast-forward-frames.ronl");
    let mut screen = DiffoScreen::launch_with_env(
        diffo_binary()?,
        &repository.worktree,
        &[("DIFFO_TRACE_FRAMES", trace_path.as_os_str())],
    )?;

    start_sync(&mut screen)?;
    screen
        .wait_for_text("Fast-forwarded master by 1 commit.")?
        .press(Key::Char('q'))?
        .wait_for_exit()?;

    assert!(fetch_marker.exists());
    assert_eq!(
        git_output(&repository.worktree, &["rev-parse", "origin/master"])?,
        remote
    );
    assert_eq!(local_head(&repository)?, remote);
    assert_eq!(
        fs::read_to_string(repository.worktree.join("tracked.txt"))?,
        "dirty\n"
    );
    assert_eq!(
        fs::read_to_string(repository.worktree.join("remote.txt"))?,
        "remote\n"
    );
    let trace = fs::read_to_string(&trace_path).context("read dirty sync frame trace")?;
    let frames = trace
        .lines()
        .map(ron::from_str::<DirtySyncFrame>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let committed = frames
        .iter()
        .position(|frame| frame.head.ends_with(&remote))
        .with_context(|| format!("trace has no committed fast-forward frame:\n{trace}"))?;
    assert!(frames[committed..].iter().all(|frame| {
        frame
            .repository_files
            .iter()
            .any(|file| file == "tracked.txt:staged=false:unstaged=true")
    }));
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
    confirm_protected_push(&mut screen, 1, "origin/master")?;
    screen
        .wait_for_text("Push rejected")?
        .wait_for_text("rejected by remote hook")?;
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
