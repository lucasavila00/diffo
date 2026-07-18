use super::support::*;

#[test]
fn mock_renamed_file_renders_unchanged_content() -> Result<()> {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../diffo-core/fixtures/repository-state.ron");
    let mut screen = DiffoScreen::launch_with_env(
        env!("CARGO_BIN_EXE_diffo"),
        Path::new(env!("CARGO_MANIFEST_DIR")),
        &[("DIFFO_MOCK_FILE", fixture.as_os_str())],
    )?;

    screen
        .wait_for_text("src/empty-...")?
        .click(&Selector::text("src/conten..."))?
        .wait_for_text("pub struct RenamedFile")?
        .wait_for_text("Content is unchanged by the rename")?;
    Ok(())
}

#[test]
fn real_merge_conflict_renders_as_a_highlighted_worktree_file() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(
        repository.worktree.join("tracked.txt"),
        "fn value() -> i32 { 1 }\n",
    )?;
    git(&repository.worktree, &["add", "tracked.txt"])?;
    git(&repository.worktree, &["commit", "-m", "Local edit"])?;
    repository.commit_remote("tracked.txt", "fn value() -> i32 { 2 }\n", "Remote edit")?;
    git(&repository.worktree, &["fetch", "origin"])?;
    git_must_fail(&repository.worktree, &["merge", "origin/master"])?;

    let mut screen = repository.screen()?;
    screen
        .wait_for_text("U  tracked.txt")?
        .wait_for_text("<<<<<<< HEAD")?
        .wait_for_text("=======")?
        .wait_for_text(">>>>>>> origin/master")?;
    Ok(())
}

#[test]
fn unstaged_source_containing_git_metadata_renders_as_text() -> Result<()> {
    let repository = TestRepository::new()?;
    let path = repository.worktree.join("crates/diffo-diff/src/lib.rs");
    fs::create_dir_all(path.parent().context("source parent")?)?;
    fs::write(&path, METADATA_LOOKING_SOURCE_BEFORE)?;
    git(&repository.worktree, &["add", "."])?;
    git(&repository.worktree, &["commit", "-m", "Add diff parser"])?;
    fs::write(&path, METADATA_LOOKING_SOURCE_AFTER)?;

    let mut screen = repository.screen()?;
    assert_metadata_looking_source_is_text(&mut screen)?;
    Ok(())
}

#[test]
fn staged_source_containing_git_metadata_renders_as_text() -> Result<()> {
    let repository = TestRepository::new()?;
    let path = repository.worktree.join("metadata.txt");
    fs::write(&path, METADATA_LOOKING_SOURCE_BEFORE)?;
    git(&repository.worktree, &["add", "metadata.txt"])?;
    git(&repository.worktree, &["commit", "-m", "Add metadata text"])?;
    fs::write(&path, METADATA_LOOKING_SOURCE_AFTER)?;
    git(&repository.worktree, &["add", "metadata.txt"])?;

    let mut screen = repository.screen()?;
    assert_metadata_looking_source_is_text(&mut screen)?;
    Ok(())
}

#[test]
fn untracked_source_containing_git_metadata_renders_as_text() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(
        repository.worktree.join("metadata.txt"),
        METADATA_LOOKING_SOURCE_AFTER,
    )?;

    let mut screen = repository.screen()?;
    assert_metadata_looking_source_is_text(&mut screen)?;
    Ok(())
}

fn assert_metadata_looking_source_is_text(screen: &mut DiffoScreen) -> Result<()> {
    screen
        .wait_for_text("GIT binary patch")?
        .wait_for_text("Binary files a/x and b/x differ")?
        .wait_for_text("diff --cc file.rs")?
        .wait_for_text("@@@ -1 -1 +1 @@@")?
        .wait_for_text("<<<<<<< HEAD")?;
    assert!(!screen.contents().contains("Binary file changed."));
    Ok(())
}
