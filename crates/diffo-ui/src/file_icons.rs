//! Fixed one-cell file and folder icons.

use std::path::Path;

pub const GENERIC_FILE: &str = "";
pub const FOLDER: &str = "";

const FILE_NAMES: &[(&str, &str)] = &[
    (".gitignore", ""),
    ("cargo.lock", ""),
    ("cargo.toml", ""),
    ("dockerfile", ""),
    ("license", ""),
    ("makefile", ""),
    ("package-lock.json", ""),
    ("package.json", ""),
    ("readme", ""),
    ("readme.md", ""),
];

const COMPOUND_EXTENSIONS: &[(&str, &str)] = &[
    ("blade.php", ""),
    ("d.ts", ""),
    ("tar.bz2", ""),
    ("tar.gz", ""),
    ("tar.xz", ""),
];

const EXTENSIONS: &[(&str, &str)] = &[
    ("bash", ""),
    ("c", ""),
    ("cpp", ""),
    ("css", ""),
    ("go", ""),
    ("gz", ""),
    ("h", ""),
    ("hpp", ""),
    ("html", ""),
    ("js", ""),
    ("json", ""),
    ("jsx", ""),
    ("lock", ""),
    ("md", ""),
    ("php", ""),
    ("py", ""),
    ("ron", ""),
    ("rs", ""),
    ("scss", ""),
    ("sh", ""),
    ("toml", ""),
    ("ts", ""),
    ("tsx", ""),
    ("yaml", ""),
    ("yml", ""),
    ("zip", ""),
];

/// Returns the fixed icon for a file path.
#[must_use]
pub fn file_icon(path: &Path) -> &'static str {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return GENERIC_FILE;
    };
    let name = name.to_ascii_lowercase();

    if let Some((_, icon)) = FILE_NAMES.iter().find(|(candidate, _)| *candidate == name) {
        return icon;
    }

    if let Some((_, icon)) = COMPOUND_EXTENSIONS
        .iter()
        .filter(|(extension, _)| has_extension(&name, extension))
        .max_by_key(|(extension, _)| extension.len())
    {
        return icon;
    }

    path.extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .and_then(|extension| {
            EXTENSIONS
                .iter()
                .find(|(candidate, _)| *candidate == extension)
                .map(|(_, icon)| *icon)
        })
        .unwrap_or(GENERIC_FILE)
}

fn has_extension(name: &str, extension: &str) -> bool {
    name.strip_suffix(extension)
        .is_some_and(|prefix| prefix.ends_with('.'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::text::Line;

    #[test]
    fn matches_file_name_before_extension() {
        assert_eq!(file_icon(Path::new("package.json")), "");
        assert_eq!(file_icon(Path::new("other.json")), "");
    }

    #[test]
    fn matches_compound_then_final_extension() {
        assert_eq!(file_icon(Path::new("view.blade.php")), "");
        assert_eq!(file_icon(Path::new("view.php")), "");
        assert_eq!(file_icon(Path::new("types.d.ts")), "");
        assert_eq!(file_icon(Path::new("main.rs")), "");
    }

    #[test]
    fn unknown_files_use_the_generic_icon() {
        assert_eq!(file_icon(Path::new("unknown.custom")), GENERIC_FILE);
        assert_eq!(file_icon(Path::new("no-extension")), GENERIC_FILE);
    }

    #[test]
    fn every_icon_has_terminal_width_one() {
        let icons = FILE_NAMES
            .iter()
            .chain(COMPOUND_EXTENSIONS)
            .chain(EXTENSIONS)
            .map(|(_, icon)| *icon)
            .chain([GENERIC_FILE, FOLDER]);

        for icon in icons {
            assert_eq!(Line::raw(icon).width(), 1, "icon {icon:?}");
        }
    }
}
