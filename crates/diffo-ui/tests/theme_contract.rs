use std::{fs, path::Path};

use diffo_ui::{design, disabled_control_style, enabled_control_style, interaction, theme};
use ratatui::layout::Margin;
use ratatui::style::{Color, Modifier};

#[test]
fn semantic_palette_is_fixed() {
    assert_eq!(theme::TEXT, Color::White);
    assert_eq!(theme::CHROME, Color::DarkGray);
    assert_eq!(theme::INFORMATION, Color::LightCyan);
    assert_eq!(theme::SELECTION_BACKGROUND, theme::CHROME);
    assert_eq!(theme::SUCCESS, Color::LightGreen);
    assert_eq!(theme::WARNING, Color::Yellow);
    assert_eq!(theme::DANGER, Color::LightRed);
    assert_eq!(theme::CONFLICT_FOREGROUND, Color::LightYellow);
    assert_eq!(theme::CONFLICT_BACKGROUND, Color::Indexed(58));
}

#[test]
fn enabled_controls_are_distinct_from_chrome() {
    let style = enabled_control_style();
    assert_eq!(style.fg, Some(theme::TEXT));
    assert_ne!(style.fg, Some(theme::CHROME));
    assert!(style.add_modifier.contains(Modifier::BOLD));
    assert_eq!(disabled_control_style().fg, Some(theme::CHROME));
    assert_eq!(interaction::FLAT_ROW, "· ");
    assert_eq!(interaction::EDIT, "✎");
    assert_eq!(interaction::DISMISS, "×");
    assert_eq!(interaction::PANE_DRAG, "↔");
}

#[test]
fn structural_geometry_is_fixed() {
    assert_eq!(design::BORDER_WIDTH, 1);
    assert_eq!(
        design::PANEL_INSET,
        Margin {
            horizontal: 1,
            vertical: 1,
        }
    );
    assert_eq!(
        design::DIALOG_INSET,
        Margin {
            horizontal: 2,
            vertical: 1,
        }
    );
    assert_eq!(design::ACTIVITY_RAIL_WIDTH, 5);
    assert_eq!(design::COMMAND_PALETTE_WIDTH.resolve(100), 70);
    assert_eq!(design::COMMAND_PALETTE_WIDTH.resolve(20), 20);
    assert_eq!(design::HELP_WIDTH.resolve(200), 90);
    assert_eq!(design::COMMIT_EDITOR_WIDTH.resolve(100), 70);
}

#[test]
fn chrome_renderers_use_design_system_tokens() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let crate_sources = [
        "crates/diffo-command/src",
        "crates/diffo-explorer/src",
        "crates/diffo-file-picker/src",
        "crates/diffo-text-view/src",
        "crates/diffo-tui/src",
        "crates/diffo-workbench/src",
    ];
    let exceptions = [
        "crates/diffo-tui/src/rendering_tests.rs",
        "crates/diffo-tui/src/style.rs",
    ];

    let mut sources = Vec::new();
    for relative in crate_sources {
        rust_sources(&workspace.join(relative), &mut sources);
    }
    for path in sources {
        let relative = path
            .strip_prefix(&workspace)
            .expect("chrome source should be inside the workspace");
        if exceptions
            .iter()
            .any(|exception| relative == Path::new(exception))
        {
            continue;
        }
        let source = fs::read_to_string(&path).expect("chrome source should be readable");
        let production = source
            .rfind("#[cfg(test)]\nmod tests {")
            .map_or(source.as_str(), |tests| &source[..tests]);
        assert!(
            !production.contains("Color::"),
            "{} chooses a raw color; use a diffo_ui::theme role",
            relative.display()
        );
        assert!(
            !production.contains("Margin {"),
            "{} defines local padding; use a diffo_ui::design inset",
            relative.display()
        );
        for line in production.lines() {
            let declaration = line.trim_start();
            let local_dimension = (declaration.starts_with("const ")
                || declaration.starts_with("pub const ")
                || declaration.starts_with("pub(crate) const "))
                && ["_WIDTH", "_HEIGHT", "_MARGIN", "_PADDING", "_INSET"]
                    .iter()
                    .any(|suffix| declaration.contains(suffix));
            assert!(
                !local_dimension || declaration.contains("VIEWER_GUTTER_WIDTH"),
                "{} defines local chrome geometry; use a diffo_ui::design token",
                relative.display()
            );
            assert!(
                !numeric_argument(declaration, "Constraint::Length(")
                    && !numeric_argument(declaration, "Constraint::Percentage("),
                "{} uses a raw layout constraint; use a diffo_ui::design token",
                relative.display()
            );
        }
    }
}

fn numeric_argument(line: &str, call: &str) -> bool {
    line.find(call)
        .and_then(|start| line[start + call.len()..].chars().next())
        .is_some_and(|character| character.is_ascii_digit())
}

fn rust_sources(directory: &Path, sources: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(directory).expect("chrome source directory should be readable") {
        let path = entry
            .expect("chrome source entry should be readable")
            .path();
        if path.is_dir() {
            rust_sources(&path, sources);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            sources.push(path);
        }
    }
}
