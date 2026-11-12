//! Fonts installed on the machine, plus the bundled fallback.
//!
//! The catalogue is built once from the platform font directories (or from
//! `BEAMBURIN_FONT_DIRS`) and is never written to. Text objects store the
//! chosen font name; the canvas registers only the fonts the document uses, so
//! opening a project does not load hundreds of files.
//!
//! If a name cannot be resolved (an old project, or a font removed from the
//! machine) the bundled `DejaVu Sans` is used, so text keeps rendering and the
//! project still opens.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use egui::{Context, FontData, FontDefinitions, FontFamily};

/// Name of the font embedded in the binary.
pub const BUNDLED_NAME: &str = "DejaVu Sans (bundled)";

/// Bundled font data, also used by text-to-outlines.
pub const BUNDLED_DATA: &[u8] = include_bytes!("../assets/fonts/DejaVuSans.ttf");
/// Bundled bold face.
pub const BUNDLED_BOLD_NAME: &str = "DejaVu Sans (bundled) Bold";
pub const BUNDLED_BOLD_DATA: &[u8] = include_bytes!("../assets/fonts/DejaVuSans-Bold.ttf");
/// Bundled oblique face.
pub const BUNDLED_OBLIQUE_NAME: &str = "DejaVu Sans (bundled) Oblique";
pub const BUNDLED_OBLIQUE_DATA: &[u8] = include_bytes!("../assets/fonts/DejaVuSans-Oblique.ttf");
/// Bundled bold oblique face.
pub const BUNDLED_BOLD_OBLIQUE_NAME: &str = "DejaVu Sans (bundled) Bold Oblique";
pub const BUNDLED_BOLD_OBLIQUE_DATA: &[u8] =
    include_bytes!("../assets/fonts/DejaVuSans-BoldOblique.ttf");

/// Prefix that keeps generated font families separate from egui's own.
const FAMILY_PREFIX: &str = "beamburin-font-";

/// A font file found on the machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontEntry {
    /// Display name shown in the interface, derived from the file name.
    pub name: String,
    pub path: PathBuf,
}

/// Fonts installed on the machine, sorted by name.
///
/// The bundled font is not included; use [`names`] for the full choice list.
pub fn installed() -> &'static [FontEntry] {
    static FONTS: OnceLock<Vec<FontEntry>> = OnceLock::new();
    FONTS.get_or_init(|| {
        let mut entries = collect_from(&font_dirs());
        entries.sort_by(|left, right| {
            left.name
                .to_lowercase()
                .cmp(&right.name.to_lowercase())
                .then_with(|| left.path.cmp(&right.path))
        });
        entries.dedup_by(|left, right| left.name == right.name);
        entries
    })
}

/// Every choice offered by the font pickers: the bundled font first.
pub fn names() -> Vec<String> {
    let mut names = Vec::with_capacity(installed().len() + 1);
    names.push(BUNDLED_NAME.to_string());
    names.extend(installed().iter().map(|entry| entry.name.clone()));
    names
}

/// Resolve a stored font name to a file on this machine.
///
/// `"sans"` and the empty name mean the bundled font, for compatibility with
/// projects created before fonts were selectable.
pub fn resolve(name: &str) -> Option<PathBuf> {
    if name.trim().is_empty() || name == "sans" || name == BUNDLED_NAME {
        return None;
    }
    installed()
        .iter()
        .find(|entry| entry.name == name)
        .map(|entry| entry.path.clone())
}

/// Font bytes for outline conversion, falling back to the bundled font.
pub fn read(name: &str) -> Vec<u8> {
    match name {
        BUNDLED_BOLD_NAME => BUNDLED_BOLD_DATA.to_vec(),
        BUNDLED_OBLIQUE_NAME => BUNDLED_OBLIQUE_DATA.to_vec(),
        BUNDLED_BOLD_OBLIQUE_NAME => BUNDLED_BOLD_OBLIQUE_DATA.to_vec(),
        _ => resolve(name)
            .and_then(|path| std::fs::read(path).ok())
            .unwrap_or_else(|| BUNDLED_DATA.to_vec()),
    }
}

/// Name of the bold and/or italic face of a font, when one is available.
///
/// The bundled `DejaVu` family carries all four faces. Installed families are
/// matched by name (Bold/Italic/Oblique); when no face matches, the base font is
/// returned so the text still renders.
pub fn variant(name: &str, bold: bool, italic: bool) -> String {
    let base = if name.trim().is_empty() || name == "sans" || name == BUNDLED_NAME {
        BUNDLED_NAME
    } else {
        name
    };
    if base == BUNDLED_NAME {
        return match (bold, italic) {
            (true, true) => BUNDLED_BOLD_OBLIQUE_NAME,
            (true, false) => BUNDLED_BOLD_NAME,
            (false, true) => BUNDLED_OBLIQUE_NAME,
            (false, false) => BUNDLED_NAME,
        }
        .to_string();
    }
    let normalized = normalize_name(base);
    let mut best: Option<&FontEntry> = None;
    for entry in installed() {
        let candidate = normalize_name(&entry.name);
        if !candidate.starts_with(&normalized) {
            continue;
        }
        let has_bold = candidate.contains("bold");
        let has_italic = candidate.contains("italic") || candidate.contains("oblique");
        if has_bold != bold || has_italic != italic {
            continue;
        }
        if best.is_none_or(|current| entry.name.len() < current.name.len()) {
            best = Some(entry);
        }
    }
    best.map_or_else(|| base.to_string(), |entry| entry.name.clone())
}

fn normalize_name(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect()
}

/// egui family for a stored font name.
pub fn family(name: &str) -> FontFamily {
    FontFamily::Name(family_key(name).into())
}

/// Stable key used to register the family in [`FontDefinitions`].
pub fn family_key(name: &str) -> String {
    let name = if name.trim().is_empty() || name == "sans" {
        BUNDLED_NAME
    } else {
        name
    };
    format!("{FAMILY_PREFIX}{name}")
}

/// Register the fonts used by the document with egui.
///
/// Does nothing while the set is unchanged. Fonts that cannot be read fall back
/// to the bundled font, so a missing file is never a rendering failure.
pub fn ensure_loaded(ctx: &Context, requested: &[String]) {
    let mut needed: Vec<String> = requested
        .iter()
        .map(|name| {
            if name.trim().is_empty() || name == "sans" {
                BUNDLED_NAME.to_string()
            } else {
                name.clone()
            }
        })
        .collect();
    needed.sort();
    needed.dedup();

    let id = egui::Id::new("beamburin_document_fonts");
    if ctx.data(|data| data.get_temp::<Vec<String>>(id)).as_deref() == Some(needed.as_slice()) {
        return;
    }

    let mut definitions = FontDefinitions::default();
    for (name, data) in [
        (BUNDLED_NAME, BUNDLED_DATA),
        (BUNDLED_BOLD_NAME, BUNDLED_BOLD_DATA),
        (BUNDLED_OBLIQUE_NAME, BUNDLED_OBLIQUE_DATA),
        (BUNDLED_BOLD_OBLIQUE_NAME, BUNDLED_BOLD_OBLIQUE_DATA),
    ] {
        let key = family_key(name);
        definitions
            .font_data
            .insert(key.clone(), Arc::new(FontData::from_static(data)));
        definitions
            .families
            .insert(FontFamily::Name(key.as_str().into()), vec![key]);
    }

    for name in &needed {
        let key = family_key(name);
        definitions
            .families
            .entry(FontFamily::Name(key.as_str().into()))
            .or_insert_with(|| vec!["beamburin-bundled".to_string()]);
        if matches!(
            name.as_str(),
            BUNDLED_NAME | BUNDLED_BOLD_NAME | BUNDLED_OBLIQUE_NAME | BUNDLED_BOLD_OBLIQUE_NAME
        ) {
            continue;
        }
        let Some(path) = resolve(name) else {
            continue;
        };
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        if bytes.len() > 32 * 1024 * 1024 {
            continue;
        }
        definitions
            .font_data
            .insert(key.clone(), Arc::new(FontData::from_owned(bytes)));
        definitions.families.insert(
            FontFamily::Name(key.as_str().into()),
            vec![key, "beamburin-bundled".to_string()],
        );
    }

    // The interface font is egui's default, which has no arrows: hints such as
    // "Preferences → Shortcuts" rendered a box. The bundled face is already in
    // the binary, so use it as the last fallback for the interface families;
    // egui only reaches for it on glyphs the default font does not carry.
    let bundled_key = family_key(BUNDLED_NAME);
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        let chain = definitions.families.entry(family).or_default();
        if !chain.contains(&bundled_key) {
            chain.push(bundled_key.clone());
        }
    }

    ctx.set_fonts(definitions);
    ctx.data_mut(|data| data.insert_temp(id, needed));
    ctx.request_repaint();
}

/// Font directories for this platform, in preference order.
fn font_dirs() -> Vec<PathBuf> {
    if let Some(value) = std::env::var_os("BEAMBURIN_FONT_DIRS") {
        return std::env::split_paths(&value).collect();
    }
    let mut dirs = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        dirs.push(home.join(".fonts"));
        dirs.push(home.join(".local/share/fonts"));
    }
    if let Some(data) = std::env::var_os("XDG_DATA_HOME") {
        dirs.push(PathBuf::from(data).join("fonts"));
    }
    if cfg!(windows) {
        let windir =
            std::env::var_os("WINDIR").map_or_else(|| PathBuf::from("C:\\Windows"), PathBuf::from);
        dirs.push(windir.join("Fonts"));
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(local).join("Microsoft/Windows/Fonts"));
        }
    } else {
        dirs.push(PathBuf::from("/usr/local/share/fonts"));
        dirs.push(PathBuf::from("/usr/share/fonts"));
    }
    dirs
}

/// Scan directories recursively, without following links, for `.ttf`/`.otf`.
fn collect_from(dirs: &[PathBuf]) -> Vec<FontEntry> {
    let mut entries = Vec::new();
    for dir in dirs {
        walk(dir, 0, &mut entries);
    }
    entries
}

fn walk(dir: &Path, depth: usize, entries: &mut Vec<FontEntry>) {
    if depth > 4 {
        return;
    }
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for item in read.flatten() {
        let Ok(file_type) = item.file_type() else {
            continue;
        };
        let path = item.path();
        if file_type.is_dir() {
            walk(&path, depth + 1, entries);
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if !extension.eq_ignore_ascii_case("ttf") && !extension.eq_ignore_ascii_case("otf") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        let name = stem.replace(['_'], " ").trim().to_string();
        if name.is_empty() {
            continue;
        }
        entries.push(FontEntry { name, path });
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BUNDLED_BOLD_DATA, BUNDLED_BOLD_NAME, BUNDLED_BOLD_OBLIQUE_DATA, BUNDLED_BOLD_OBLIQUE_NAME,
        BUNDLED_DATA, BUNDLED_NAME, BUNDLED_OBLIQUE_NAME, collect_from, ensure_loaded, family,
        family_key, names, read, resolve, variant,
    };
    use egui::Context;

    #[test]
    fn the_bundled_font_is_always_the_first_choice() {
        let all = names();
        assert_eq!(all.first().map(String::as_str), Some(BUNDLED_NAME));
        assert_eq!(read(BUNDLED_NAME), BUNDLED_DATA);
        assert!(resolve(BUNDLED_NAME).is_none());
    }

    #[test]
    fn legacy_font_names_fall_back_to_the_bundled_font() {
        assert_eq!(family_key("sans"), family_key(BUNDLED_NAME));
        assert_eq!(family_key(""), family_key(BUNDLED_NAME));
        assert_eq!(family("sans"), family(BUNDLED_NAME));
        assert_eq!(read("sans"), BUNDLED_DATA);
        assert_eq!(read("a font that does not exist"), BUNDLED_DATA);
    }

    #[test]
    fn the_interface_falls_back_to_the_bundled_font_for_arrows() {
        // Hints say "Preferences → Shortcuts"; the default interface font has
        // no arrow, so the bundled face has to be in the fallback chain.
        let ctx = Context::default();
        ensure_loaded(&ctx, &[]);
        for character in ['→', '←'] {
            let mut has_glyph = false;
            ctx.fonts(|fonts| {
                has_glyph = fonts.has_glyph(&egui::FontId::proportional(13.0), character);
            });
            assert!(
                has_glyph,
                "the interface font has no glyph for {character:?}"
            );
        }
    }

    #[test]
    fn family_keys_are_distinct_per_font() {
        assert_ne!(family_key("DejaVu Sans"), family_key("Some Other Font"));
        assert!(family_key("DejaVu Sans").starts_with("beamburin-font-"));
    }

    #[test]
    fn scanning_finds_font_files_and_ignores_other_files() {
        let dir = std::env::temp_dir().join("bw_font_scan");
        let nested = dir.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(dir.join("Alpha_Title.ttf"), BUNDLED_DATA).unwrap();
        std::fs::write(nested.join("Beta.otf"), BUNDLED_DATA).unwrap();
        std::fs::write(dir.join("notes.txt"), b"not a font").unwrap();
        let entries = collect_from(std::slice::from_ref(&dir));
        let mut found: Vec<String> = entries.into_iter().map(|entry| entry.name).collect();
        found.sort();
        assert_eq!(found, vec!["Alpha Title", "Beta"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_catalogue_is_sorted_and_unique() {
        let entries = super::installed();
        let mut names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count);
        for pair in entries.windows(2) {
            assert!(
                pair[0].name.to_lowercase() <= pair[1].name.to_lowercase(),
                "{} before {}",
                pair[0].name,
                pair[1].name
            );
        }
    }

    #[test]
    fn bundled_variants_are_named_and_resolved() {
        assert_eq!(variant("sans", false, false), BUNDLED_NAME);
        assert_eq!(variant("", true, false), BUNDLED_BOLD_NAME);
        assert_eq!(variant(BUNDLED_NAME, false, true), BUNDLED_OBLIQUE_NAME);
        assert_eq!(variant(BUNDLED_NAME, true, true), BUNDLED_BOLD_OBLIQUE_NAME);
        assert_eq!(read(BUNDLED_BOLD_NAME).len(), BUNDLED_BOLD_DATA.len());
        assert_eq!(
            read(BUNDLED_BOLD_OBLIQUE_NAME).len(),
            BUNDLED_BOLD_OBLIQUE_DATA.len()
        );
    }
}
