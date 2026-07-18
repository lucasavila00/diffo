use super::support::*;

#[test]
fn mock_remote_error_shows_the_executed_action() -> Result<()> {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../diffo-core/fixtures/repository-state.ron");
    let mut screen = DiffoScreen::launch_with_env(
        env!("CARGO_BIN_EXE_diffo"),
        Path::new(env!("CARGO_MANIFEST_DIR")),
        &[("DIFFO_MOCK_FILE", fixture.as_os_str())],
    )?;

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
    let mut screen = repository.screen_with_network_delay()?;

    screen
        .press(Key::Char('1'))?
        .wait_for_text("Command Palette")?
        .type_text("fetch")?
        .press(Key::Enter)?
        .wait_for_text("Fetching")?;

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
    let mut screen = repository.screen_with_network_delay()?;

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
    let mut screen = repository.screen_with_network_delay()?;

    screen
        .press(Key::Char('1'))?
        .wait_for_text("Command Palette")?
        .type_text("pull")?
        .press(Key::Enter)?
        .wait_for_text("Pulling")?;

    wait_for("remote file to be pulled", || {
        Ok(repository.worktree.join("remote.txt").exists())
    })?;
    screen.wait_for_text("Pulled 1 commit")?;
    screen.wait_for_text_gone("Pulling")?;
    Ok(())
}

#[test]
fn primary_pull_button_shows_loading_and_pulls() -> Result<()> {
    let repository = TestRepository::new()?;
    repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    git(&repository.worktree, &["fetch", "origin"])?;
    let mut screen = repository.screen_with_network_delay()?;

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
    let mut screen = repository.screen_with_network_delay()?;
    screen.wait_for_text("[ Push ]")?;

    repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    screen
        .click(&Selector::text("[ Push ]"))?
        .wait_for_text("Pushing")?
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
    let known_hosts = repository.root.path().join("known_hosts");
    let ssh = local_ssh_transport(&repository, SshPrompt::ConfirmHost)?;
    let mut screen = ssh_screen(&repository, &ssh, &known_hosts, None)?;

    screen
        .press(Key::Char('1'))?
        .type_text("fetch")?
        .press(Key::Enter)?
        .wait_for_text("Trust fakehost?")?
        .wait_for_text("SHA256:abcdefghijklmnopqrstuvwxyz0123456789+/=")?;
    assert!(!known_hosts.exists());
    screen.press(Key::Right)?.press(Key::Enter)?;

    wait_for("approved SSH fetch to update origin", || {
        Ok(git_output(&repository.worktree, &["rev-parse", "origin/HEAD"])? == remote_commit)
    })?;
    screen.wait_for_text("Fetched 1 ref")?;
    assert_eq!(fs::read_to_string(known_hosts)?, "fakehost\n");
    Ok(())
}

#[test]
fn cancelling_local_ssh_host_approval_preserves_refs_and_known_hosts() -> Result<()> {
    let repository = TestRepository::new()?;
    let before = git_output(&repository.worktree, &["rev-parse", "origin/HEAD"])?;
    repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    let known_hosts = repository.root.path().join("known_hosts");
    let ssh = local_ssh_transport(&repository, SshPrompt::ConfirmHost)?;
    let mut screen = ssh_screen(&repository, &ssh, &known_hosts, None)?;

    screen
        .press(Key::Char('1'))?
        .type_text("fetch")?
        .press(Key::Enter)?
        .wait_for_text("Trust fakehost?")?
        .press(Key::Enter)?
        .wait_for_text("Operation cancelled")?;

    assert_eq!(
        git_output(&repository.worktree, &["rev-parse", "origin/HEAD"])?,
        before
    );
    assert!(!known_hosts.exists());
    assert!(!repository.worktree.join("remote.txt").exists());
    Ok(())
}

#[test]
fn local_helpers_complete_sequential_username_and_secret_prompts() -> Result<()> {
    let repository = TestRepository::new()?;
    let remote_commit = repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    let known_hosts = repository.root.path().join("known_hosts");
    let ssh = local_ssh_transport(&repository, SshPrompt::Credentials)?;
    let secret = "sentinel-secret";
    let mut screen = ssh_screen(&repository, &ssh, &known_hosts, Some(secret))?;

    screen
        .press(Key::Char('1'))?
        .type_text("fetch")?
        .press(Key::Enter)?
        .wait_for_text("Username for example.com")?
        .type_text("alice")?
        .press(Key::Enter)?
        .wait_for_text("Secret for example.com")?;
    for character in secret.chars() {
        screen.press(Key::Char(character))?;
    }
    assert!(!screen.contents().contains(secret));
    screen.press(Key::Enter)?;

    wait_for("credentialed SSH helper fetch to update origin", || {
        Ok(git_output(&repository.worktree, &["rev-parse", "origin/HEAD"])? == remote_commit)
    })?;
    screen.wait_for_text("Fetched 1 ref")?;
    Ok(())
}

#[test]
fn local_ssh_helper_completes_a_masked_key_passphrase_prompt() -> Result<()> {
    let repository = TestRepository::new()?;
    let remote_commit = repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    let known_hosts = repository.root.path().join("known_hosts");
    let ssh = local_ssh_transport(&repository, SshPrompt::Passphrase)?;
    let secret = "passphrase-sentinel";
    let mut screen = ssh_screen(&repository, &ssh, &known_hosts, Some(secret))?;

    screen
        .press(Key::Char('1'))?
        .type_text("fetch")?
        .press(Key::Enter)?
        .wait_for_text("Passphrase for /keys/id_ed25519")?;
    for character in secret.chars() {
        screen.press(Key::Char(character))?;
    }
    assert!(!screen.contents().contains(secret));
    screen.press(Key::Enter)?;

    wait_for("passphrase SSH helper fetch to update origin", || {
        Ok(git_output(&repository.worktree, &["rev-parse", "origin/HEAD"])? == remote_commit)
    })?;
    screen.wait_for_text("Fetched 1 ref")?;
    Ok(())
}

#[derive(Clone, Copy)]
enum SshPrompt {
    ConfirmHost,
    Credentials,
    Passphrase,
}

fn local_ssh_transport(repository: &TestRepository, prompt: SshPrompt) -> Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt as _;

    let remote = repository.root.path().join("remote.git");
    let remote_url = format!("fakehost:{}", remote.display());
    git(
        &repository.worktree,
        &["remote", "set-url", "origin", &remote_url],
    )?;
    let dialogue = match prompt {
        SshPrompt::ConfirmHost => concat!(
            "prompt=\"The authenticity of host 'fakehost (127.0.0.1)' can't be established.\n",
            "ED25519 key fingerprint is SHA256:abcdefghijklmnopqrstuvwxyz0123456789+/=.\n",
            "This key is not known by any other names.\n",
            "Are you sure you want to continue connecting (yes/no/[fingerprint])? \"\n",
            "answer=$(SSH_ASKPASS_PROMPT=confirm \"$SSH_ASKPASS\" \"$prompt\") || exit 1\n",
            "test \"$answer\" = yes || exit 1\n",
            "printf 'fakehost\\n' > \"$DIFFO_TEST_KNOWN_HOSTS\"\n",
        ),
        SshPrompt::Credentials => concat!(
            "username=$(\"$GIT_ASKPASS\" \"Username for 'https://person@example.com': \" ) || exit 1\n",
            "test \"$username\" = alice || exit 1\n",
            "secret=$(\"$GIT_ASKPASS\" \"Password for 'https://person:credential@example.com/repo': \" ) || exit 1\n",
            "test \"$secret\" = \"$DIFFO_TEST_SECRET\" || exit 1\n",
        ),
        SshPrompt::Passphrase => concat!(
            "secret=$(SSH_ASKPASS_PROMPT=none \"$SSH_ASKPASS\" \"Enter passphrase for key '/keys/id_ed25519': \" ) || exit 1\n",
            "test \"$secret\" = \"$DIFFO_TEST_SECRET\" || exit 1\n",
        ),
    };
    let script = format!(
        "#!/bin/sh\nset -eu\n{dialogue}command=\nfor argument in \"$@\"; do command=$argument; done\nexec sh -c \"$command\"\n"
    );
    let path = repository.root.path().join("ssh-transport");
    fs::write(&path, script)?;
    let mut permissions = fs::metadata(&path)?.permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&path, permissions)?;
    Ok(path)
}

fn ssh_screen(
    repository: &TestRepository,
    ssh: &Path,
    known_hosts: &Path,
    secret: Option<&str>,
) -> Result<DiffoScreen> {
    let mut environment = vec![
        ("GIT_SSH", ssh.as_os_str()),
        ("GIT_SSH_VARIANT", OsStr::new("ssh")),
        ("DIFFO_TEST_KNOWN_HOSTS", known_hosts.as_os_str()),
    ];
    if let Some(secret) = secret {
        environment.push(("DIFFO_TEST_SECRET", OsStr::new(secret)));
    }
    DiffoScreen::launch_with_env(
        env!("CARGO_BIN_EXE_diffo"),
        &repository.worktree,
        &environment,
    )
}
