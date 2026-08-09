use super::support::*;

#[derive(Deserialize)]
struct MergeFrame {
    refresh_generation: u64,
    head: String,
    repository_operation: diffo_core::RepositoryOperationState,
    repository_files: Vec<String>,
}

#[test]
fn clean_merge_installs_the_new_head_in_one_traced_frame() -> Result<()> {
    let repository = TestRepository::new()?;
    let destination = git_output(&repository.worktree, &["branch", "--show-current"])?;
    let old_head = git_output(&repository.worktree, &["rev-parse", "HEAD"])?;
    git(&repository.worktree, &["switch", "-c", "topic"])?;
    fs::write(repository.worktree.join("topic.txt"), "topic\n")?;
    git(&repository.worktree, &["add", "topic.txt"])?;
    git(&repository.worktree, &["commit", "-m", "Topic"])?;
    let new_head = git_output(&repository.worktree, &["rev-parse", "HEAD"])?;
    git(&repository.worktree, &["switch", &destination])?;

    let trace_path = repository.root.path().join("merge-frames.ronl");
    let mut gate = diffo_e2e::GitProxy::new("merge", diffo_e2e::GitGatePhase::After)?;
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
        .press(Key::Char('1'))?
        .type_text("git: merge")?
        .press(Key::Enter)?
        .wait_for_text("Select a branch or tag to merge from")?
        .wait_for_text_gone("Loading refs...")?
        .type_text("topic")?
        .wait_for(&Selector::selected_row("topic"))?
        .press(Key::Enter)?
        .wait_for_text("Merging topic")?;
    gate.wait_until_blocked()?;

    assert_eq!(
        git_output(&repository.worktree, &["rev-parse", "HEAD"])?,
        new_head
    );
    assert!(!screen.contents().contains("Merged topic"));

    gate.release()?;
    screen
        .wait_for_text("Merged topic")?
        .press(Key::Char('q'))?
        .wait_for_exit()?;

    let trace = fs::read_to_string(&trace_path).context("read merge frame trace")?;
    let frames = trace
        .lines()
        .map(ron::from_str::<MergeFrame>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let old = format!("named:{destination}:{old_head}");
    let new = format!("named:{destination}:{new_head}");
    let first_new = frames
        .iter()
        .find(|frame| frame.head == new)
        .with_context(|| format!("trace has no merged frame:\n{trace}"))?;
    assert!(first_new.refresh_generation > 0);
    assert!(
        frames
            .iter()
            .all(|frame| frame.head == old || frame.head == new)
    );
    Ok(())
}

#[test]
fn late_cancellation_installs_the_head_git_already_changed() -> Result<()> {
    let repository = TestRepository::new()?;
    let destination = git_output(&repository.worktree, &["branch", "--show-current"])?;
    git(&repository.worktree, &["switch", "-c", "topic"])?;
    fs::write(repository.worktree.join("topic.txt"), "topic\n")?;
    git(&repository.worktree, &["add", "topic.txt"])?;
    git(&repository.worktree, &["commit", "-m", "Topic"])?;
    let new_head = git_output(&repository.worktree, &["rev-parse", "HEAD"])?;
    git(&repository.worktree, &["switch", &destination])?;

    let trace_path = repository.root.path().join("cancelled-merge-frames.ronl");
    let gate = diffo_e2e::GitProxy::new("merge", diffo_e2e::GitGatePhase::After)?;
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
        .press(Key::Char('1'))?
        .type_text("git: merge")?
        .press(Key::Enter)?
        .wait_for_text_gone("Loading refs...")?
        .type_text("topic")?
        .wait_for(&Selector::selected_row("topic"))?
        .press(Key::Enter)?
        .wait_for_text("Merging topic")?;
    gate.wait_until_blocked()?;
    assert_eq!(
        git_output(&repository.worktree, &["rev-parse", "HEAD"])?,
        new_head
    );

    screen
        .click(&Selector::toast_action("Merging topic", ""))?
        .wait_for_text_gone("Merging topic")?
        .wait_for_text(&new_head[..7])?
        .press(Key::Char('q'))?
        .wait_for_exit()?;

    assert!(!screen.contents().contains("Merged topic"));
    let trace = fs::read_to_string(&trace_path).context("read cancelled merge frame trace")?;
    let new = format!("named:{destination}:{new_head}");
    assert!(
        trace
            .lines()
            .map(ron::from_str::<MergeFrame>)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .iter()
            .any(|frame| frame.head == new)
    );
    Ok(())
}

#[test]
fn conflicted_merge_can_be_aborted_from_the_command_palette() -> Result<()> {
    let repository = TestRepository::new()?;
    let destination = git_output(&repository.worktree, &["branch", "--show-current"])?;
    git(&repository.worktree, &["switch", "-c", "topic"])?;
    fs::write(repository.worktree.join("tracked.txt"), "topic\n")?;
    git(&repository.worktree, &["commit", "-am", "Topic"])?;
    git(&repository.worktree, &["switch", &destination])?;
    fs::write(repository.worktree.join("tracked.txt"), "destination\n")?;
    git(&repository.worktree, &["commit", "-am", "Destination"])?;
    let destination_head = git_output(&repository.worktree, &["rev-parse", "HEAD"])?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Char('1'))?
        .type_text("git: merge")?
        .press(Key::Enter)?
        .wait_for_text_gone("Loading refs...")?
        .type_text("topic")?
        .wait_for(&Selector::selected_row("topic"))?
        .press(Key::Enter)?
        .wait_for_text("Merge stopped with conflicts in 1 file")?
        .wait_for_text("conflicts")?
        .press(Key::Char('1'))?
        .type_text("abort merge")?
        .wait_for_text("Git: Abort Merge")?
        .press(Key::Enter)?
        .wait_for_text("Merge aborted")?
        .press(Key::Char('q'))?
        .wait_for_exit()?;

    assert_eq!(
        git_output(&repository.worktree, &["rev-parse", "HEAD"])?,
        destination_head
    );
    assert_eq!(
        fs::read_to_string(repository.worktree.join("tracked.txt"))?,
        "destination\n"
    );
    git_must_fail(
        &repository.worktree,
        &["rev-parse", "--verify", "MERGE_HEAD"],
    )?;
    Ok(())
}

#[test]
fn resolved_conflict_stays_visible_until_the_merge_commit_is_installed() -> Result<()> {
    let repository = TestRepository::new()?;
    let destination = git_output(&repository.worktree, &["branch", "--show-current"])?;
    git(&repository.worktree, &["switch", "-c", "topic"])?;
    fs::write(repository.worktree.join("tracked.txt"), "topic\n")?;
    git(&repository.worktree, &["commit", "-am", "Topic"])?;
    git(&repository.worktree, &["switch", &destination])?;
    fs::write(repository.worktree.join("tracked.txt"), "destination\n")?;
    git(&repository.worktree, &["commit", "-am", "Destination"])?;
    let old_head = git_output(&repository.worktree, &["rev-parse", "HEAD"])?;
    git_must_fail(&repository.worktree, &["merge", "--no-edit", "topic"])?;
    let trace_path = repository.root.path().join("resolved-merge-frames.ronl");
    let mut screen = DiffoScreen::launch_with_env(
        diffo_binary()?,
        &repository.worktree,
        &[("DIFFO_TRACE_FRAMES", trace_path.as_os_str())],
    )?;

    screen
        .wait_for_text("merge conflicts")?
        .wait_for_text("Resolve and stage 1")?
        .wait_for_text("[ Complete merge ]")?;

    fs::write(repository.worktree.join("tracked.txt"), "resolved\n")?;
    git(&repository.worktree, &["add", "tracked.txt"])?;
    screen
        .wait_for_text("merge ready")?
        .wait_for_text("All conflicts resolve")?
        .click(&Selector::text("[ Complete merge ]"))?
        .wait_for_text("Committed ")?
        .wait_for_text("Unpushed")?
        .wait_for_text_gone("merge ready")?
        .press(Key::Char('q'))?
        .wait_for_exit()?;

    let new_head = git_output(&repository.worktree, &["rev-parse", "HEAD"])?;
    assert_ne!(new_head, old_head);
    assert_eq!(
        git_output(&repository.worktree, &["show", "-s", "--format=%s", "HEAD"])?,
        "Complete merge"
    );
    assert_eq!(
        git_output(&repository.worktree, &["show", "-s", "--format=%P", "HEAD"])?
            .split_whitespace()
            .count(),
        2
    );
    git_must_fail(
        &repository.worktree,
        &["rev-parse", "--verify", "MERGE_HEAD"],
    )?;

    let trace = fs::read_to_string(&trace_path).context("read resolved merge frame trace")?;
    let frames = trace
        .lines()
        .map(ron::from_str::<MergeFrame>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let old = format!("named:{destination}:{old_head}");
    let new = format!("named:{destination}:{new_head}");
    let ready = frames
        .iter()
        .find(|frame| {
            frame.head == old
                && frame.repository_operation == diffo_core::RepositoryOperationState::Merge
                && frame
                    .repository_files
                    .iter()
                    .any(|file| file == "tracked.txt:staged=true:unstaged=false")
        })
        .with_context(|| format!("trace has no merge-ready frame:\n{trace}"))?;
    assert!(ready.refresh_generation > 0);
    assert!(frames.iter().any(|frame| {
        frame.head == new
            && frame.repository_operation == diffo_core::RepositoryOperationState::None
            && frame.repository_files.is_empty()
    }));
    assert!(frames.iter().all(|frame| {
        frame.head != new
            || (frame.repository_operation == diffo_core::RepositoryOperationState::None
                && frame.repository_files.is_empty())
    }));
    Ok(())
}

#[test]
fn cancelling_before_merge_mutation_preserves_head() -> Result<()> {
    let repository = TestRepository::new()?;
    let destination = git_output(&repository.worktree, &["branch", "--show-current"])?;
    let old_head = git_output(&repository.worktree, &["rev-parse", "HEAD"])?;
    git(&repository.worktree, &["switch", "-c", "topic"])?;
    fs::write(repository.worktree.join("topic.txt"), "topic\n")?;
    git(&repository.worktree, &["add", "topic.txt"])?;
    git(&repository.worktree, &["commit", "-m", "Topic"])?;
    git(&repository.worktree, &["switch", &destination])?;
    let gate = diffo_e2e::GitProxy::new("merge", diffo_e2e::GitGatePhase::Before)?;
    let path = gate.path()?;
    let mut screen = DiffoScreen::launch_with_env(
        diffo_binary()?,
        &repository.worktree,
        &[("PATH", path.as_os_str())],
    )?;

    screen
        .press(Key::Char('1'))?
        .type_text("git: merge")?
        .press(Key::Enter)?
        .wait_for_text_gone("Loading refs...")?
        .type_text("topic")?
        .wait_for(&Selector::selected_row("topic"))?
        .press(Key::Enter)?
        .wait_for_text("Merging topic")?;
    gate.wait_until_blocked()?;
    screen
        .click(&Selector::toast_action("Merging topic", ""))?
        .wait_for_text_gone("Merging topic")?;

    assert_eq!(
        git_output(&repository.worktree, &["rev-parse", "HEAD"])?,
        old_head
    );
    assert!(!screen.contents().contains("Merged topic"));
    Ok(())
}
