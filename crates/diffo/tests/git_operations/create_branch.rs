use super::support::*;

#[derive(Deserialize)]
struct BranchFrame {
    refresh_generation: u64,
    head: String,
    repository_files: Vec<String>,
}

#[test]
fn create_branch_from_commits_the_new_head_and_snapshot_in_one_traced_frame() -> Result<()> {
    let repository = TestRepository::new()?;
    let original = git_output(&repository.worktree, &["branch", "--show-current"])?;
    git(&repository.worktree, &["branch", "base"])?;
    let base_commit = git_output(&repository.worktree, &["rev-parse", "refs/heads/base"])?;
    fs::write(
        repository.worktree.join("tracked.txt"),
        "changed on topic\n",
    )?;
    let trace_path = repository.root.path().join("create-branch-frames.ronl");
    let mut gate = diffo_e2e::GitProxy::new("checkout", diffo_e2e::GitGatePhase::After)?;
    let path = gate.path()?;
    let mut screen = DiffoScreen::launch_with_env(
        diffo_binary()?,
        &repository.worktree,
        &[
            ("PATH", path.as_os_str()),
            ("DIFFO_TRACE_FRAMES", trace_path.as_os_str()),
        ],
    )?;

    screen
        .wait_for_text("changed on topic")?
        .press(Key::Char('1'))?
        .type_text("create branch from")?
        .press(Key::Enter)?
        .wait_for_text("Create branch from")?
        .wait_for_text_gone("Loading branches...")?
        .type_text("base")?
        .press(Key::Enter)?
        .wait_for_text_gone("Create branch from")?
        .wait_for_text("Create branch")?
        .type_text("topic name")?
        .wait_for_text("The new branch will be topic-name")?
        .press(Key::Enter)?
        .wait_for_text("Creating branch topic-name")?;
    gate.wait_until_blocked()?;

    assert_eq!(
        git_output(&repository.worktree, &["branch", "--show-current"])?,
        "topic-name"
    );
    assert_eq!(
        git_output(
            &repository.worktree,
            &["rev-parse", "refs/heads/topic-name"]
        )?,
        base_commit
    );
    assert!(!screen.contents().contains(" topic-name ·"));

    gate.release()?;
    screen
        .wait_for_text(" topic-name ·")?
        .wait_for_text("Created and checked out topic-name")?
        .press(Key::Char('q'))?
        .wait_for_exit()?;

    let trace = fs::read_to_string(&trace_path).context("read create branch frame trace")?;
    let frames = trace
        .lines()
        .map(ron::from_str::<BranchFrame>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let first_created = frames
        .iter()
        .find(|frame| frame.head.starts_with("named:topic-name:"))
        .with_context(|| format!("trace has no created branch frame:\n{trace}"))?;
    assert!(first_created.refresh_generation > 0);
    assert!(
        first_created
            .repository_files
            .iter()
            .any(|file| { file == "tracked.txt:staged=false:unstaged=true" })
    );
    assert!(frames.iter().all(|frame| {
        frame.head.starts_with(&format!("named:{original}:"))
            || frame.head.starts_with("named:topic-name:")
    }));
    let upstream = Command::new("git")
        .args(["config", "--get", "branch.topic-name.remote"])
        .current_dir(&repository.worktree)
        .output()?;
    assert!(!upstream.status.success());
    assert!(upstream.stdout.is_empty());
    Ok(())
}

#[test]
fn cancelling_create_branch_before_mutation_preserves_head_and_ref() -> Result<()> {
    let repository = TestRepository::new()?;
    let original = git_output(&repository.worktree, &["branch", "--show-current"])?;
    let gate = diffo_e2e::GitProxy::new("checkout", diffo_e2e::GitGatePhase::Before)?;
    let path = gate.path()?;
    let mut screen = DiffoScreen::launch_with_env(
        diffo_binary()?,
        &repository.worktree,
        &[("PATH", path.as_os_str())],
    )?;

    screen
        .press(Key::Char('1'))?
        .type_text("create branch")?
        .press(Key::Enter)?
        .wait_for_text("Create branch")?
        .wait_for_text_gone("Loading branches...")?
        .type_text("cancelled-topic")?
        .press(Key::Enter)?
        .wait_for_text("Creating branch cancelled-topic")?;
    gate.wait_until_blocked()?;
    screen
        .click(&Selector::toast_action(
            "Creating branch cancelled-topic",
            "",
        ))?
        .wait_for_text_gone("Creating branch cancelled-topic")?;

    assert_eq!(
        git_output(&repository.worktree, &["branch", "--show-current"])?,
        original
    );
    git_must_fail(
        &repository.worktree,
        &["show-ref", "--verify", "refs/heads/cancelled-topic"],
    )?;
    assert!(
        !screen
            .contents()
            .contains("Created and checked out cancelled-topic")
    );
    Ok(())
}

#[test]
fn create_branch_from_remote_uses_the_selected_commit_without_upstream() -> Result<()> {
    let repository = TestRepository::new()?;
    let original = git_output(&repository.worktree, &["branch", "--show-current"])?;
    git(&repository.worktree, &["checkout", "-b", "remote-base"])?;
    fs::write(repository.worktree.join("remote.txt"), "remote base\n")?;
    git(&repository.worktree, &["add", "remote.txt"])?;
    git(&repository.worktree, &["commit", "-m", "Remote base"])?;
    let base_commit = git_output(&repository.worktree, &["rev-parse", "HEAD"])?;
    git(&repository.worktree, &["push", "origin", "remote-base"])?;
    git(&repository.worktree, &["checkout", &original])?;
    git(&repository.worktree, &["branch", "-D", "remote-base"])?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Char('1'))?
        .type_text("create branch from")?
        .press(Key::Enter)?
        .wait_for_text("Create branch from")?
        .wait_for_text_gone("Loading branches...")?
        .type_text("origin/remote-base")?
        .press(Key::Enter)?
        .wait_for_text_gone("Create branch from")?
        .type_text("from-remote")?
        .press(Key::Enter)?
        .wait_for_text("Created and checked out from-remote")?
        .press(Key::Char('q'))?
        .wait_for_exit()?;

    assert_eq!(
        git_output(
            &repository.worktree,
            &["rev-parse", "refs/heads/from-remote"]
        )?,
        base_commit
    );
    git_must_fail(
        &repository.worktree,
        &["config", "--get", "branch.from-remote.remote"],
    )?;
    Ok(())
}

#[test]
fn create_branch_validation_keeps_an_existing_name_in_the_modal() -> Result<()> {
    let repository = TestRepository::new()?;
    let original = git_output(&repository.worktree, &["branch", "--show-current"])?;
    git(&repository.worktree, &["branch", "existing"])?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Char('1'))?
        .type_text("create branch")?
        .press(Key::Enter)?
        .wait_for_text("Create branch")?
        .wait_for_text_gone("Loading branches...")?
        .type_text("existing")?
        .wait_for_text("Branch existing already exists")?
        .press(Key::Enter)?
        .wait_for_text("Branch existing already exists")?;
    assert_eq!(
        git_output(&repository.worktree, &["branch", "--show-current"])?,
        original
    );
    screen
        .press(Key::Escape)?
        .wait_for_text_gone("Branch existing already exists")?
        .press(Key::Char('q'))?
        .wait_for_exit()?;
    Ok(())
}

#[test]
fn create_branch_from_can_cancel_the_picker_and_name_steps() -> Result<()> {
    let repository = TestRepository::new()?;
    let branches_before = git_output(&repository.worktree, &["branch", "--format=%(refname)"])?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Char('1'))?
        .type_text("create branch from")?
        .press(Key::Enter)?
        .wait_for_text("Create branch from")?
        .wait_for_text_gone("Loading branches...")?
        .wait_for_text("origin/master")?
        .press(Key::Escape)?
        .wait_for_text_gone("Create branch from")?
        .press(Key::Char('1'))?
        .type_text("create branch from")?
        .press(Key::Enter)?
        .wait_for_text_gone("Loading branches...")?
        .wait_for_text("origin/master")?
        .press(Key::Enter)?
        .wait_for_text_gone("Create branch from")?
        .wait_for_text("Create branch")?
        .press(Key::Escape)?
        .wait_for_text_gone("Create branch")?
        .press(Key::Char('q'))?
        .wait_for_exit()?;

    assert_eq!(
        git_output(&repository.worktree, &["branch", "--format=%(refname)"])?,
        branches_before
    );
    Ok(())
}

#[test]
fn create_branch_from_rejects_a_base_that_moves_before_mutation() -> Result<()> {
    let repository = TestRepository::new()?;
    let original = git_output(&repository.worktree, &["branch", "--show-current"])?;
    git(&repository.worktree, &["branch", "base"])?;
    fs::write(repository.worktree.join("later.txt"), "later\n")?;
    git(&repository.worktree, &["add", "later.txt"])?;
    git(
        &repository.worktree,
        &["commit", "-m", "Advance current branch"],
    )?;
    let mut gate = diffo_e2e::GitProxy::new("show-ref", diffo_e2e::GitGatePhase::Before)?;
    let path = gate.path()?;
    let mut screen = DiffoScreen::launch_with_env(
        diffo_binary()?,
        &repository.worktree,
        &[("PATH", path.as_os_str())],
    )?;

    screen
        .press(Key::Char('1'))?
        .type_text("create branch from")?
        .press(Key::Enter)?
        .wait_for_text_gone("Loading branches...")?
        .wait_for_text("origin/master")?
        .type_text("base")?
        .press(Key::Enter)?
        .wait_for_text_gone("Create branch from")?
        .type_text("stale-base-topic")?
        .press(Key::Enter)?
        .wait_for_text("Creating branch stale-base-topic")?;
    gate.wait_until_blocked()?;
    git(&repository.worktree, &["branch", "-f", "base", &original])?;
    gate.release()?;
    screen
        .wait_for_text("Create branch failed")?
        .wait_for_text("selected branch")?
        .wait_for_text("changed; reopen the branch picker")?
        .press(Key::Escape)?
        .wait_for_text_gone("Create branch failed")?
        .press(Key::Char('q'))?
        .wait_for_exit()?;

    assert_eq!(
        git_output(&repository.worktree, &["branch", "--show-current"])?,
        original
    );
    git_must_fail(
        &repository.worktree,
        &["show-ref", "--verify", "refs/heads/stale-base-topic"],
    )?;
    Ok(())
}
