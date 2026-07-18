use super::support::*;

#[test]
fn tab_cycles_empty_activities_and_restores_diff_overlay_state() -> Result<()> {
    let repository = changed_repository()?;
    let mut screen = repository.screen()?;

    screen
        .wait_for_text("Changes")?
        .press(Key::Char('1'))?
        .wait_for_text("Command Palette")?
        .type_text("pull")?
        .press(Key::Tab)?
        .wait_for_text_gone("Command Palette")?;
    assert!(!screen.contents().contains("Changes"));

    screen
        .press(Key::Tab)?
        .press(Key::Tab)?
        .wait_for_text("Command Palette")?
        .wait_for_text("pull")?;
    Ok(())
}

#[test]
fn activity_bar_clicks_select_tools_and_diff_returns_intact() -> Result<()> {
    let repository = changed_repository()?;
    let mut screen = repository.screen()?;

    screen
        .wait_for_text("Changes")?
        .click(&Selector::text("⌕"))?
        .wait_for_text_gone("Changes")?
        .click(&Selector::text("≠"))?
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
