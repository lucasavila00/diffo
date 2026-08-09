use super::support::*;

#[derive(Deserialize)]
struct ExplorerFrame {
    requested_explorer_file: Option<String>,
    displayed_explorer_file: Option<String>,
    text_surface: Option<ExplorerSurface>,
}

#[derive(Deserialize)]
struct ExplorerSurface {
    surface: String,
    render_mode: String,
}

#[test]
fn command_palette_blocks_activity_switching_and_does_not_restore_hidden_state() -> Result<()> {
    let repository = changed_repository()?;
    let mut screen = repository.screen()?;

    screen
        .wait_for_text("Changes")?
        .press(Key::Char('1'))?
        .wait_for_text("Command Palette")?
        .type_text("sync")?
        .press(Key::Tab)?
        .wait_for_text("Command Palette")?
        .wait_for_text("Changes")?
        .press(Key::Escape)?
        .wait_for_text_gone("Command Palette")?
        .press(Key::Tab)?
        .wait_for_text("Explorer")?
        .wait_for_text_gone("Changes")?;

    screen
        .press(Key::Tab)?
        .wait_for_text("AI Review")?
        .press(Key::Tab)?
        .wait_for_text("Changes")?
        .wait_for_text_gone("Command Palette")?;
    Ok(())
}

#[cfg(feature = "codex-mock")]
#[test]
fn guided_review_stages_and_ai_commits_without_leaving_review() -> Result<()> {
    let repository = TestRepository::new()?;
    std::fs::write(repository.worktree.join("tracked.txt"), "reviewed change\n")?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Tab)?
        .press(Key::Tab)?
        .wait_for_text("AI Review")?
        .wait_for_text("Start review")?
        .wait_for_text("Stage / unstage")?
        .wait_for_text("the whole file")?
        .press(Key::Enter)?
        .wait_for_text("Review order")?
        .wait_for_text("Why this matters")?
        .wait_for_text("reviewed change")?
        .press(Key::Char(' '))?;

    wait_for("reviewed file to be staged", || {
        Ok(cached_paths(&repository.worktree)?.contains("tracked.txt"))
    })?;
    screen
        .wait_for_text("Review order")?
        .wait_for_text("reviewed change")?
        .press(Key::Char('i'))?;
    wait_for("reviewed change to be AI committed", || {
        Ok(
            git_output(&repository.worktree, &["log", "-1", "--format=%s"])?
                == "test: create commit with Codex",
        )
    })?;
    screen.wait_for_text("Committed")?;
    Ok(())
}

#[test]
fn activity_palettes_share_git_commands_and_keep_specific_catalogs_separate() -> Result<()> {
    let repository = changed_repository()?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Char('1'))?
        .wait_for_text("Git: Fetch")?
        .wait_for_text("Application: Update Diffo")?
        .wait_for_text_gone("Explorer: Collapse All Folders")?
        .press(Key::Escape)?
        .wait_for_text_gone("Command Palette")?
        .press(Key::Tab)?
        .press(Key::Char('1'))?
        .wait_for_text("Git: Fetch")?
        .wait_for_text("Application: Update Diffo")?
        .wait_for_text("Explorer: Collapse All Folders")?
        .press(Key::Escape)?
        .wait_for_text_gone("Command Palette")?;
    Ok(())
}

#[test]
fn activity_bar_clicks_select_tools_and_diff_returns_intact() -> Result<()> {
    let repository = changed_repository()?;
    let mut screen = repository.screen()?;

    screen
        .wait_for_text("Changes")?
        .click(&Selector::text(""))?
        .wait_for_text_gone("Changes")?
        .click(&Selector::text(""))?
        .wait_for_text("Changes")?;
    Ok(())
}

#[test]
fn empty_activity_keeps_quit_available() -> Result<()> {
    changed_repository()?
        .screen()?
        .press(Key::Tab)?
        .press(Key::Char('q'))?
        .wait_for_exit()?;
    Ok(())
}

#[test]
fn rapid_explorer_open_commits_only_the_latest_syntax_ready_file() -> Result<()> {
    let repository = TestRepository::new()?;
    std::fs::write(
        repository.worktree.join("a.rs"),
        "pub const EXPLORER_ALPHA: usize = 1;\n",
    )?;
    std::fs::write(
        repository.worktree.join("b.rs"),
        "pub const EXPLORER_BRAVO: usize = 2;\n",
    )?;
    git(&repository.worktree, &["add", "a.rs", "b.rs"])?;
    git(
        &repository.worktree,
        &["commit", "-m", "Add Explorer files"],
    )?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Tab)?
        .wait_for_text("a.rs")?
        .press(Key::Char('k'))?
        .wait_for_text("EXPLORER_BRAVO")?;

    assert!(!screen.contents().contains("EXPLORER_ALPHA"));
    assert!(
        screen
            .contents()
            .lines()
            .next()
            .unwrap_or_default()
            .contains("b.rs")
    );
    Ok(())
}

#[test]
fn explorer_cold_scrolls_keep_full_text_in_both_directions() -> Result<()> {
    let repository = TestRepository::new()?;
    let mut contents = String::new();
    for line in 1..=700 {
        writeln!(contents, "EXPLORER_LINE_{line:04} value {line}")
            .context("build Explorer scrolling fixture")?;
    }
    fs::write(repository.worktree.join("a-large.txt"), contents)?;
    git(&repository.worktree, &["add", "a-large.txt"])?;
    git(
        &repository.worktree,
        &["commit", "-m", "Add Explorer scrolling fixture"],
    )?;
    let trace_path = repository.root.path().join("explorer-scroll-frames.ronl");
    let mut screen = DiffoScreen::launch_with_env(
        diffo_binary()?,
        &repository.worktree,
        &[("DIFFO_TRACE_FRAMES", trace_path.as_os_str())],
    )?;
    screen
        .press(Key::Tab)?
        .wait_for_text("EXPLORER_LINE_0001")?
        .drag_vertical_scrollbar(0, 100)?
        .wait_for_text("EXPLORER_LINE_0674")?
        .drag_vertical_scrollbar(100, 40)?
        .wait_for_text("EXPLORER_LINE_0259")?
        .press(Key::Char('q'))?
        .wait_for_exit()?;
    drop(screen);

    let trace = fs::read_to_string(&trace_path).context("read Explorer frame trace")?;
    let frames = trace
        .lines()
        .map(ron::from_str::<TextSurfaceFrame>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let inputs = frames
        .iter()
        .enumerate()
        .filter(|(_, frame)| {
            frame
                .input_events
                .iter()
                .any(|event| event.contains("Drag(Left)"))
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    assert_eq!(
        inputs.len(),
        2,
        "trace has unexpected drag frames:\n{trace}"
    );
    for (input, direction) in inputs.into_iter().zip(["down", "up"]) {
        let surface = frames[input]
            .text_surface
            .as_ref()
            .with_context(|| format!("{direction} input has no text surface:\n{trace}"))?;
        assert_eq!(surface.surface, "Explorer");
        assert_eq!(
            surface.render_mode, "Full",
            "{direction} exposed an Explorer syntax skeleton:\n{trace}"
        );
        let committed = frames[input + 1..]
            .iter()
            .filter_map(|frame| frame.text_surface.as_ref())
            .find(|later| later.surface == "Explorer" && later.viewport.0 != surface.viewport.0)
            .with_context(|| format!("{direction} target never committed:\n{trace}"))?;
        if direction == "down" {
            assert!(committed.viewport.0 > surface.viewport.0, "{trace}");
        } else {
            assert!(committed.viewport.0 < surface.viewport.0, "{trace}");
        }
        assert!(
            committed.viewport.0.abs_diff(surface.viewport.0) > 100,
            "{direction} did not cross cold syntax coverage:\n{trace}"
        );
        assert_eq!(
            committed.render_mode, "Full",
            "{direction} target committed without syntax:\n{trace}"
        );
    }
    Ok(())
}

#[test]
fn quick_open_from_diff_commits_explorer_selection_and_content_atomically() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("a.txt"), "ALPHA_CONTENT\n")?;
    fs::create_dir(repository.worktree.join("nested"))?;
    fs::write(
        repository.worktree.join("nested/bravo.txt"),
        "BRAVO_CONTENT\n",
    )?;
    git(&repository.worktree, &["add", "."])?;
    git(&repository.worktree, &["commit", "-m", "Add files"])?;
    fs::write(repository.worktree.join("a.txt"), "ALPHA_CHANGED\n")?;
    let trace_path = repository.root.path().join("quick-open-frames.ronl");
    let mut screen = DiffoScreen::launch_with_env(
        diffo_binary()?,
        &repository.worktree,
        &[("DIFFO_TRACE_FRAMES", trace_path.as_os_str())],
    )?;

    screen
        .wait_for_text("ALPHA_CHANGED")?
        .press(Key::Char('o'))?
        .wait_for_text("Quick Open")?
        .type_text("bravo")?
        .press(Key::Enter)?
        .wait_for_text("BRAVO_CONTENT")?
        .press(Key::Char('q'))?
        .wait_for_exit()?;
    drop(screen);

    let trace = fs::read_to_string(&trace_path).context("read Quick Open frame trace")?;
    let frames = trace
        .lines()
        .map(ron::from_str::<ExplorerFrame>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    assert!(frames.iter().all(|frame| {
        frame.requested_explorer_file == frame.displayed_explorer_file
            || frame
                .text_surface
                .as_ref()
                .is_none_or(|surface| surface.render_mode != "Full")
    }));
    assert!(frames.iter().any(|frame| {
        frame.requested_explorer_file.as_deref() == Some("nested/bravo.txt")
            && frame.displayed_explorer_file.as_deref() == Some("nested/bravo.txt")
            && frame.text_surface.as_ref().is_some_and(|surface| {
                surface.surface == "Explorer" && surface.render_mode == "Full"
            })
    }));
    Ok(())
}

#[test]
fn explorer_removes_a_deleted_file_without_showing_head_content() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("keep.txt"), "KEEP_CONTENT\n")?;
    fs::write(repository.worktree.join("removed.txt"), "REMOVED_CONTENT\n")?;
    git(&repository.worktree, &["add", "keep.txt", "removed.txt"])?;
    git(
        &repository.worktree,
        &["commit", "-m", "Add Explorer files"],
    )?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Tab)?
        .wait_for_text("Explorer")?
        .wait_for_text("keep.txt")?
        .click(&Selector::text("keep.txt"))?
        .wait_for_text("KEEP_CONTENT")?
        .click(&Selector::text("removed.txt"))?
        .wait_for_text("REMOVED_CONTENT")?;

    fs::remove_file(repository.worktree.join("removed.txt"))?;

    screen
        .wait_for_text_gone("removed.txt")?
        .wait_for_text("base")?;

    assert!(!screen.contents().contains("REMOVED_CONTENT"));
    Ok(())
}

#[test]
fn ignored_file_rename_commits_explorer_path_and_content_atomically() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join(".gitignore"), "*.ignored\n")?;
    git(&repository.worktree, &["add", ".gitignore"])?;
    git(
        &repository.worktree,
        &["commit", "-m", "Ignore Explorer fixture"],
    )?;
    fs::write(
        repository.worktree.join("old.ignored"),
        "IGNORED_CONTENT_BEFORE\n",
    )?;
    let trace_path = repository.root.path().join("ignored-explorer-frames.ronl");
    let mut screen = DiffoScreen::launch_with_env(
        diffo_binary()?,
        &repository.worktree,
        &[("DIFFO_TRACE_FRAMES", trace_path.as_os_str())],
    )?;

    screen
        .press(Key::Tab)?
        .wait_for_text("old.ignored")?
        .click(&Selector::text("old.ignored"))?
        .wait_for_text("IGNORED_CONTENT_BEFORE")?;

    fs::rename(
        repository.worktree.join("old.ignored"),
        repository.worktree.join("new.ignored"),
    )?;
    fs::write(
        repository.worktree.join("new.ignored"),
        "IGNORED_CONTENT_AFTER\n",
    )?;

    screen
        .wait_for_text("new.ignored")?
        .wait_for_text("IGNORED_CONTENT_AFTER")?
        .wait_for_text_gone("IGNORED_CONTENT_BEFORE")?
        .press(Key::Char('q'))?
        .wait_for_exit()?;

    let trace = fs::read_to_string(&trace_path).context("read ignored Explorer frame trace")?;
    let frames = trace
        .lines()
        .map(ron::from_str::<ExplorerFrame>)
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("parse ignored Explorer frame trace")?;
    assert!(
        frames.iter().any(|frame| {
            frame.requested_explorer_file.as_deref() == Some("new.ignored")
                && frame.displayed_explorer_file.as_deref() == Some("old.ignored")
                && frame.text_surface.as_ref().is_some_and(|surface| {
                    surface.surface == "Explorer" && surface.render_mode == "TextSkeleton"
                })
        }),
        "trace has no atomic ignored-file transition:\n{trace}"
    );
    assert!(
        frames.iter().all(|frame| {
            frame.requested_explorer_file == frame.displayed_explorer_file
                || frame
                    .text_surface
                    .as_ref()
                    .is_none_or(|surface| surface.render_mode != "Full")
        }),
        "trace rendered mismatched Explorer identities:\n{trace}"
    );
    assert!(
        frames.iter().any(|frame| {
            frame.requested_explorer_file.as_deref() == Some("new.ignored")
                && frame.displayed_explorer_file.as_deref() == Some("new.ignored")
                && frame
                    .text_surface
                    .as_ref()
                    .is_some_and(|surface| surface.render_mode == "Full")
        }),
        "trace has no committed ignored file:\n{trace}"
    );
    Ok(())
}

#[test]
fn explorer_horizontal_pan_is_bounded_and_terminal_safe() -> Result<()> {
    let repository = TestRepository::new()?;
    let line = format!("START_{}\x1b[2JCONTROL_RIGHT_EDGE\n", "x".repeat(100));
    fs::write(repository.worktree.join("tracked.txt"), line)?;
    git(&repository.worktree, &["add", "tracked.txt"])?;
    git(
        &repository.worktree,
        &["commit", "-m", "Add wide control fixture"],
    )?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Tab)?
        .wait_for_text("START_")?
        .press_many(Key::Right, 20)?
        .wait_for_text("␛[2JCONTROL_RIGHT_EDGE")?;
    let panned = screen.contents();
    assert!(panned.contains("Explorer"), "{panned}");
    assert!(panned.contains("[ Commands (1 / F1) ]"), "{panned}");

    screen.press_many(Key::Left, 20)?.wait_for_text("START_")?;
    Ok(())
}
