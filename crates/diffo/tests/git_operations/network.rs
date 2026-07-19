use super::ssh::{Authentication, LocalSshServer};
use super::support::*;

#[test]
fn real_remote_error_shows_the_executed_action() -> Result<()> {
    let repository = TestRepository::new()?;
    let missing_remote = repository.root.path().join("missing.git");
    git(
        &repository.worktree,
        &[
            "remote",
            "set-url",
            "origin",
            missing_remote
                .to_str()
                .context("remote path is not UTF-8")?,
        ],
    )?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Char('1'))?
        .type_text("fetch")?
        .press(Key::Enter)?
        .wait_for_text("Fetch failed:")?
        .wait_for_text("Fetch")?;
    Ok(())
}

#[test]
fn palette_search_runs_fetch() -> Result<()> {
    let repository = TestRepository::new()?;
    let remote_commit = repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Char('1'))?
        .wait_for_text("Command Palette")?
        .type_text("fetch")?
        .press(Key::Enter)?;

    wait_for("origin tracking branch to be fetched", || {
        Ok(git_output(&repository.worktree, &["rev-parse", "origin/HEAD"])? == remote_commit)
    })?;
    screen.wait_for_text("Fetched 1 ref")?;
    screen.wait_for_text_gone("Fetching")?;
    assert!(!repository.worktree.join("remote.txt").exists());
    Ok(())
}

#[test]
fn explorer_palette_runs_the_shared_fetch_command() -> Result<()> {
    shared_palette_fetch_from_activity(1)
}

#[test]
fn search_palette_runs_the_shared_fetch_command() -> Result<()> {
    shared_palette_fetch_from_activity(2)
}

fn shared_palette_fetch_from_activity(activity_tabs: usize) -> Result<()> {
    let repository = TestRepository::new()?;
    let remote_commit = repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    let mut screen = repository.screen()?;

    screen
        .press_many(Key::Tab, activity_tabs)?
        .press(Key::Char('1'))?
        .wait_for_text("Command Palette")?
        .type_text("fetch")?
        .press(Key::Enter)?
        .wait_for_text_gone("Command Palette")?;
    wait_for("shared fetch command to update origin", || {
        Ok(git_output(&repository.worktree, &["rev-parse", "origin/HEAD"])? == remote_commit)
    })?;
    screen.wait_for_text("Fetched 1 ref")?;
    Ok(())
}

#[test]
fn palette_search_runs_pull() -> Result<()> {
    let repository = TestRepository::new()?;
    repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Char('1'))?
        .wait_for_text("Command Palette")?
        .type_text("pull")?
        .press(Key::Enter)?;

    wait_for("remote file to be pulled", || {
        Ok(repository.worktree.join("remote.txt").exists())
    })?;
    screen.wait_for_text("Pulled 1 commit")?;
    screen.wait_for_text_gone("Pulling")?;
    Ok(())
}

#[test]
fn cancelling_a_delayed_command_releases_the_next_queued_command() -> Result<()> {
    let repository = TestRepository::new()?;
    repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    let mut screen = repository.screen_with_operation_delay()?;

    screen
        .press(Key::Char('1'))?
        .type_text("fetch")?
        .press(Key::Enter)?
        .wait_for_text("Fetching")?
        .press(Key::Char('1'))?
        .type_text("pull")?
        .press(Key::Enter)?
        .wait_for_text_gone("Command Palette")?;
    assert!(screen.contents().contains("Fetching"));
    assert!(!screen.contents().contains("Pulling"));

    screen
        .click(&Selector::text("×"))?
        .wait_for_text("Pulling")?;
    wait_for("queued pull to update the worktree", || {
        Ok(repository.worktree.join("remote.txt").exists())
    })?;
    screen.wait_for_text("Pulled 1 commit")?;
    assert!(!screen.contents().contains("Fetch complete"));
    Ok(())
}

#[test]
fn primary_pull_button_shows_loading_and_pulls() -> Result<()> {
    let repository = TestRepository::new()?;
    repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    git(&repository.worktree, &["fetch", "origin"])?;
    let mut screen = repository.screen_with_operation_delay()?;

    screen
        .wait_for_text("[ Pull ]")?
        .click(&Selector::text("[ Pull ]"))?
        .wait_for_text("Pulling")?;
    wait_for("primary pull to update the worktree", || {
        Ok(repository.worktree.join("remote.txt").exists())
    })?;
    screen.wait_for_text_gone("Pulling")?;
    Ok(())
}

#[test]
fn rejected_push_shows_a_persistent_failure_toast() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("local.txt"), "local\n")?;
    git(&repository.worktree, &["add", "local.txt"])?;
    git(&repository.worktree, &["commit", "-m", "Local commit"])?;
    let mut screen = repository.screen()?;
    screen.wait_for_text("[ Push ]")?;

    repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    screen
        .click(&Selector::text("[ Push ]"))?
        .wait_for_text("Push rejected: remote changed")?;
    thread::sleep(Duration::from_millis(300));
    assert!(screen.contents().contains("Push rejected"));
    Ok(())
}

#[test]
fn success_toast_is_automatically_dismissed() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "changed\n")?;
    git(&repository.worktree, &["add", "tracked.txt"])?;
    let mut screen = repository.screen()?;

    screen
        .click(&Selector::text("[ Commit ]"))?
        .wait_for_text("Committed ")?
        .wait_for_text_gone("Committed ")?;
    Ok(())
}

#[test]
fn local_ssh_host_approval_completes_fetch() -> Result<()> {
    let repository = TestRepository::new()?;
    let remote_commit = repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    let ssh = LocalSshServer::start(&repository, Authentication::PublicKey { passphrase: "" })?;
    let mut screen = ssh_screen(&repository, &ssh)?;

    screen
        .press(Key::Char('1'))?
        .type_text("fetch")?
        .press(Key::Enter)?
        .wait_for_text("Trust diffo-e2e?")?
        .wait_for_text("SHA256:")?;
    assert!(fs::read_to_string(ssh.known_hosts())?.is_empty());
    screen.press(Key::Right)?.press(Key::Enter)?;

    wait_for("approved SSH fetch to update origin", || {
        Ok(git_output(&repository.worktree, &["rev-parse", "origin/HEAD"])? == remote_commit)
    })?;
    screen.wait_for_text("Fetched 1 ref")?;
    assert!(fs::read_to_string(ssh.known_hosts())?.contains("diffo-e2e ssh-ed25519"));
    Ok(())
}

#[test]
fn ssh_push_uses_running_image_after_launched_binary_is_replaced() -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;

    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("local.txt"), "local\n")?;
    git(&repository.worktree, &["add", "local.txt"])?;
    git(&repository.worktree, &["commit", "-m", "Local commit"])?;
    let local_head = git_output(&repository.worktree, &["rev-parse", "HEAD"])?;
    let ssh = LocalSshServer::start(&repository, Authentication::PublicKey { passphrase: "" })?;
    let launched_binary = repository.root.path().join("diffo-under-test");
    fs::copy(env!("CARGO_BIN_EXE_diffo"), &launched_binary)?;
    fs::set_permissions(&launched_binary, fs::Permissions::from_mode(0o700))?;
    let mut screen = ssh_screen_with_binary(&launched_binary, &repository, &ssh, &[])?;

    fs::remove_file(&launched_binary)?;
    fs::write(&launched_binary, "replacement must not run\n")?;
    fs::set_permissions(&launched_binary, fs::Permissions::from_mode(0o600))?;

    screen
        .wait_for_text("[ Push ]")?
        .click(&Selector::text("[ Push ]"))?
        .wait_for_text("Trust diffo-e2e?")?;
    screen
        .press(Key::Right)?
        .press(Key::Enter)?
        .wait_for_text("Pushed ")?;

    assert_eq!(
        git_output(
            &repository.root.path().join("remote.git"),
            &["rev-parse", "HEAD"]
        )?,
        local_head
    );
    assert!(fs::read_to_string(ssh.known_hosts())?.contains("diffo-e2e ssh-ed25519"));
    Ok(())
}

#[test]
fn cancelling_local_ssh_host_approval_preserves_refs_and_known_hosts() -> Result<()> {
    let repository = TestRepository::new()?;
    let before = git_output(&repository.worktree, &["rev-parse", "origin/HEAD"])?;
    repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    let ssh = LocalSshServer::start(&repository, Authentication::PublicKey { passphrase: "" })?;
    let mut screen = ssh_screen(&repository, &ssh)?;

    screen
        .press(Key::Char('1'))?
        .type_text("fetch")?
        .press(Key::Enter)?
        .wait_for_text("Trust diffo-e2e?")?
        .press(Key::Enter)?
        .wait_for_text_gone("Trust diffo-e2e?")?
        .wait_for_text_gone("Fetching")?;

    assert_eq!(
        git_output(&repository.worktree, &["rev-parse", "origin/HEAD"])?,
        before
    );
    assert!(fs::read_to_string(ssh.known_hosts())?.is_empty());
    assert!(!repository.worktree.join("remote.txt").exists());
    Ok(())
}

#[test]
fn unsupported_real_ssh_password_prompt_fails_closed() -> Result<()> {
    let repository = TestRepository::new()?;
    let before = git_output(&repository.worktree, &["rev-parse", "origin/HEAD"])?;
    repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    let ssh = LocalSshServer::start(&repository, Authentication::Password)?;
    ssh.trust_host()?;
    let mut screen = ssh_screen(&repository, &ssh)?;

    screen
        .press(Key::Char('1'))?
        .type_text("fetch")?
        .press(Key::Enter)?
        .wait_for_text("Fetch failed:")?
        .wait_for_text_gone("Fetching")?;

    assert_eq!(
        git_output(&repository.worktree, &["rev-parse", "origin/HEAD"])?,
        before
    );
    assert!(!repository.worktree.join("remote.txt").exists());
    assert!(!String::from_utf8_lossy(screen.raw_output()).contains("password:"));
    Ok(())
}

#[test]
fn real_ssh_key_passphrase_completes_fetch_without_leaking_secret() -> Result<()> {
    let repository = TestRepository::new()?;
    let remote_commit = repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    let secret = "passphrase-sentinel";
    let ssh = LocalSshServer::start(
        &repository,
        Authentication::PublicKey { passphrase: secret },
    )?;
    ssh.trust_host()?;
    let trace_path = repository.root.path().join("passphrase-frames.ronl");
    let mut screen = ssh_screen_with_binary(
        Path::new(env!("CARGO_BIN_EXE_diffo")),
        &repository,
        &ssh,
        &[("DIFFO_TRACE_FRAMES", trace_path.as_os_str())],
    )?;

    screen
        .press(Key::Char('1'))?
        .type_text("fetch")?
        .press(Key::Enter)?
        .wait_for_text("Passphrase for")?;
    for character in secret.chars() {
        screen.press(Key::Char(character))?;
    }
    assert!(!screen.contents().contains(secret));
    screen.press(Key::Enter)?;

    wait_for("passphrase SSH helper fetch to update origin", || {
        Ok(git_output(&repository.worktree, &["rev-parse", "origin/HEAD"])? == remote_commit)
    })?;
    screen.wait_for_text("Fetched 1 ref")?;
    assert!(!String::from_utf8_lossy(screen.raw_output()).contains(secret));
    screen.press(Key::Char('q'))?.wait_for_exit()?;
    drop(screen);
    assert!(!fs::read_to_string(trace_path)?.contains(secret));
    Ok(())
}

#[test]
fn cancelling_real_ssh_passphrase_preserves_repository_state() -> Result<()> {
    let repository = TestRepository::new()?;
    let before = git_output(&repository.worktree, &["rev-parse", "origin/HEAD"])?;
    repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    let ssh = LocalSshServer::start(
        &repository,
        Authentication::PublicKey {
            passphrase: "cancel-sentinel",
        },
    )?;
    ssh.trust_host()?;
    let mut screen = ssh_screen(&repository, &ssh)?;

    screen
        .press(Key::Char('1'))?
        .type_text("fetch")?
        .press(Key::Enter)?
        .wait_for_text("Passphrase for")?
        .press(Key::Escape)?
        .wait_for_text_gone("Passphrase for")?
        .wait_for_text_gone("Fetching")?;

    assert_eq!(
        git_output(&repository.worktree, &["rev-parse", "origin/HEAD"])?,
        before
    );
    assert!(!repository.worktree.join("remote.txt").exists());
    Ok(())
}

fn ssh_screen(repository: &TestRepository, ssh: &LocalSshServer) -> Result<DiffoScreen> {
    ssh_screen_with_binary(Path::new(env!("CARGO_BIN_EXE_diffo")), repository, ssh, &[])
}

fn ssh_screen_with_binary(
    binary: &Path,
    repository: &TestRepository,
    ssh: &LocalSshServer,
    extra_environment: &[(&str, &OsStr)],
) -> Result<DiffoScreen> {
    let command = ssh.command();
    let mut environment = vec![("GIT_SSH_COMMAND", OsStr::new(&command))];
    environment.extend(extra_environment.iter().copied());
    DiffoScreen::launch_with_env(binary, &repository.worktree, &environment)
}
