use std::{fs, path::Path};

use diffo_ui::{
    command_progress_style, design, disabled_control_style, enabled_control_style, theme,
};

#[test]
fn semantic_palette_is_fixed() {
    insta::assert_debug_snapshot!((
        [
            ("text", theme::TEXT),
            ("chrome", theme::CHROME),
            ("information", theme::INFORMATION),
            ("selection background", theme::SELECTION_BACKGROUND),
            ("success", theme::SUCCESS),
            ("warning", theme::WARNING),
            ("danger", theme::DANGER),
            ("conflict foreground", theme::CONFLICT_FOREGROUND),
            ("conflict background", theme::CONFLICT_BACKGROUND),
        ],
        [command_progress_style(0), command_progress_style(4)],
    ));
}

#[test]
fn enabled_controls_are_distinct_from_chrome() {
    insta::assert_debug_snapshot!((enabled_control_style(), disabled_control_style()));
}

#[test]
fn structural_geometry_is_fixed() {
    let geometry = [
        format!("border width: {}", design::BORDER_WIDTH),
        format!("panel inset: {:?}", design::PANEL_INSET),
        format!("dialog inset: {:?}", design::DIALOG_INSET),
        format!("activity rail width: {}", design::ACTIVITY_RAIL_WIDTH),
        format!(
            "command palette width at 100: {}",
            design::COMMAND_PALETTE_WIDTH.resolve(100)
        ),
        format!(
            "command palette width at 20: {}",
            design::COMMAND_PALETTE_WIDTH.resolve(20)
        ),
        format!("help width at 200: {}", design::HELP_WIDTH.resolve(200)),
        format!(
            "commit editor width at 100: {}",
            design::COMMIT_EDITOR_WIDTH.resolve(100)
        ),
    ];
    insta::assert_debug_snapshot!(geometry);
}

#[test]
fn chrome_renderers_use_design_system_tokens() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let crate_sources = [
        "crates/diffo-ui/src/command_palette.rs",
        "crates/diffo-app/src/explorer",
        "crates/diffo-ui/src/file_picker.rs",
        "crates/diffo-ui/src/file_picker",
        "crates/diffo-ui/src/text_view.rs",
        "crates/diffo-app/src/diff",
        "crates/diffo-app/src/workbench",
    ];
    let exceptions = [
        "crates/diffo-ui/src/file_picker/tests.rs",
        "crates/diffo-app/src/diff/rendering_tests.rs",
        "crates/diffo-app/src/diff/rendering_tests/chrome.rs",
        "crates/diffo-app/src/diff/rendering_tests/diff.rs",
        "crates/diffo-app/src/diff/rendering_tests/diff/transitions.rs",
        "crates/diffo-app/src/diff/view/style.rs",
        "crates/diffo-app/src/workbench/tests.rs",
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
    if directory.is_file() {
        sources.push(directory.to_owned());
        return;
    }
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
