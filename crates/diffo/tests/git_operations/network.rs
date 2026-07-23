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
        .wait_for_text("Fetch failed")?
        .wait_for_text("no remote configured")?;
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
fn palette_search_runs_sync() -> Result<()> {
    let repository = TestRepository::new()?;
    repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Char('1'))?
        .wait_for_text("Command Palette")?
        .type_text("sync")?
        .press(Key::Enter)?;

    wait_for("remote file to be synced", || {
        Ok(repository.worktree.join("remote.txt").exists())
    })?;
    screen.wait_for_text("Fast-forwarded master by 1 commit.")?;
    screen.wait_for_text_gone("Fast-forwarding master")?;
    Ok(())
}

#[test]
fn cancelling_a_blocked_git_client_enables_global_sync() -> Result<()> {
    let repository = TestRepository::new()?;
    repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    let gate = diffo_e2e::GitProxy::new("fetch", diffo_e2e::GitGatePhase::Before)?;
    let path = gate.path()?;
    let mut screen = DiffoScreen::launch_with_env(
        diffo_binary()?,
        &repository.worktree,
        &[("PATH", path.as_os_str())],
    )?;

    screen
        .press(Key::Char('1'))?
        .type_text("fetch")?
        .press(Key::Enter)?
        .wait_for_text("Fetching")?;
    gate.wait_until_blocked()?;
    assert!(screen.contents().contains("Fetching"));
    assert!(!screen.contents().contains("Fast-forwarding"));

    screen.click(&Selector::text(""))?;
    wait_for("global sync to update the worktree", || {
        screen.press(Key::Char('9'))?;
        Ok(repository.worktree.join("remote.txt").exists())
    })?;
    screen.wait_for_text("Fast-forwarded master by 1 commit.")?;
    assert!(!screen.contents().contains("Fetch complete"));
    Ok(())
}

#[test]
fn global_sync_button_shows_fast_forward_progress() -> Result<()> {
    let repository = TestRepository::new()?;
    repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    git(&repository.worktree, &["fetch", "origin"])?;
    let mut gate = diffo_e2e::GitProxy::new("merge", diffo_e2e::GitGatePhase::Before)?;
    let path = gate.path()?;
    let mut screen = DiffoScreen::launch_with_env(
        diffo_binary()?,
        &repository.worktree,
        &[("PATH", path.as_os_str())],
    )?;

    screen
        .wait_for_text("[ Sync (9 / F9) ]")?
        .click(&Selector::text("[ Sync (9 / F9) ]"))?;
    gate.wait_until_blocked()?;
    screen
        .wait_for_text("origin/master has 1 upstream-only")?
        .wait_for_text("master has no local-only commits.")?
        .wait_for_text("Plan:")?
        .wait_for_text("fast-forward master to origin/master.")?
        .wait_for_text("Fast-forwarding master")?;
    gate.release()?;
    wait_for("global sync to update the worktree", || {
        Ok(repository.worktree.join("remote.txt").exists())
    })?;
    screen.wait_for_text_gone("Fast-forwarding master")?;
    Ok(())
}

#[test]
fn rejected_push_shows_an_acknowledgement_modal() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("local.txt"), "local\n")?;
    git(&repository.worktree, &["add", "local.txt"])?;
    git(&repository.worktree, &["commit", "-m", "Local commit"])?;
    let local_before = git_output(&repository.worktree, &["rev-parse", "HEAD"])?;
    let mut gate = diffo_e2e::GitProxy::new("push", diffo_e2e::GitGatePhase::Before)?;
    let path = gate.path()?;
    let mut screen = DiffoScreen::launch_with_env(
        diffo_binary()?,
        &repository.worktree,
        &[("PATH", path.as_os_str())],
    )?;
    screen
        .wait_for_text("[ Sync (9 / F9) ]")?
        .click(&Selector::text("[ Sync (9 / F9) ]"))?;
    confirm_protected_push(&mut screen, 1, "origin/master")?;
    gate.wait_until_blocked()?;

    let remote = repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    gate.release()?;
    screen
        .wait_for_text("Push rejected")?
        .wait_for_text("remote changed; nothing was pushed")?;
    thread::sleep(Duration::from_millis(300));
    assert!(screen.contents().contains("Push rejected"));
    assert_eq!(
        git_output(&repository.worktree, &["rev-parse", "HEAD"])?,
        local_before
    );
    assert_eq!(
        git_output(
            &repository.root.path().join("remote.git"),
            &["rev-parse", "HEAD"]
        )?,
        remote
    );
    Ok(())
}

#[test]
fn success_toast_is_automatically_dismissed() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "changed\n")?;
    git(&repository.worktree, &["add", "tracked.txt"])?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Enter)?
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
    fs::copy(diffo_binary()?, &launched_binary)?;
    fs::set_permissions(&launched_binary, fs::Permissions::from_mode(0o700))?;
    let mut screen = ssh_screen_with_binary(&launched_binary, &repository, &ssh, &[])?;

    fs::remove_file(&launched_binary)?;
    fs::write(&launched_binary, "replacement must not run\n")?;
    fs::set_permissions(&launched_binary, fs::Permissions::from_mode(0o600))?;

    screen
        .wait_for_text("[ Sync (9 / F9) ]")?
        .click(&Selector::text("[ Sync (9 / F9) ]"))?
        .wait_for_text("Trust diffo-e2e?")?;
    screen.press(Key::Right)?.press(Key::Enter)?;
    confirm_protected_push(&mut screen, 1, "origin/master")?;
    screen.wait_for_text("Pushed master.")?;

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
        .wait_for_text("Fetch failed")?
        .wait_for_text("authentication required")?
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
        &diffo_binary()?,
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
    ssh_screen_with_binary(&diffo_binary()?, repository, ssh, &[])
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
