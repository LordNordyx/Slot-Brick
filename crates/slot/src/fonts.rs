//! HOME → System → Font: the font all of Slot's text is drawn in. Slot's own (Open Sans Bold)
//! is built in; the fonts that come with Slot are in System/Fonts, and the player's own (any
//! .ttf or .otf) in Fonts. The choice is kept in Config/font.txt, by name (the file's name
//! without its extension), and takes effect when Slot starts.
use std::fs;
use std::path::{Path, PathBuf};

/// The built-in font's name.
pub const BUILT_IN: &str = "Slot";
const CHOICE: &str = "Config/font.txt";
/// Where fonts are found: those that come with Slot, then the player's.
const DIRS: [&str; 2] = ["System/Fonts", "Fonts"];

/// Every font there is, by name, after the built-in one; a name found twice is the first.
pub fn available(root: &Path) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    for dir in DIRS {
        let Ok(entries) = fs::read_dir(root.join(dir)) else {
            continue;
        };
        let mut found: Vec<(String, PathBuf)> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("ttf") || e.eq_ignore_ascii_case("otf"))
            })
            .filter_map(|p| Some((p.file_stem()?.to_str()?.to_string(), p)))
            .filter(|(name, _)| !name.starts_with('.') && name != BUILT_IN)
            .collect();
        found.sort_by_key(|(name, _)| name.to_lowercase());
        for (name, path) in found {
            if !out.iter().any(|(n, _)| *n == name) {
                out.push((name, path));
            }
        }
    }
    out
}

/// The chosen font's name: the built-in one when none was chosen.
pub fn chosen(root: &Path) -> String {
    fs::read_to_string(root.join(CHOICE))
        .map(|s| s.trim().to_string())
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| BUILT_IN.to_string())
}

pub fn save(root: &Path, name: &str) -> std::io::Result<()> {
    fs::write(root.join(CHOICE), format!("{name}\n"))
}

/// The font after (or before) `current`, from the built-in one through those there are.
pub fn step(root: &Path, current: &str, right: bool) -> String {
    let mut names = vec![BUILT_IN.to_string()];
    names.extend(available(root).into_iter().map(|(name, _)| name));
    let at = names.iter().position(|n| n == current).unwrap_or(0);
    let to = if right {
        (at + 1) % names.len()
    } else {
        (at + names.len() - 1) % names.len()
    };
    names.swap_remove(to)
}

/// The font this Slot draws in, as load_chosen found it (the built-in one before that).
pub fn running() -> String {
    RUNNING
        .get()
        .cloned()
        .unwrap_or_else(|| BUILT_IN.to_string())
}
static RUNNING: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// At startup, before anything is drawn: the chosen font, if it is not the built-in one. A
/// font gone missing, or one that is not a font, leaves the built-in one, and says so.
pub fn load_chosen(root: &Path) -> String {
    let name = load(root);
    let _ = RUNNING.set(name.clone());
    name
}

fn load(root: &Path) -> String {
    let name = chosen(root);
    if name == BUILT_IN {
        return name;
    }
    let Some((_, path)) = available(root).into_iter().find(|(n, _)| *n == name) else {
        eprintln!("slot: font: {name} is not in System/Fonts or Fonts; using Slot's own");
        return BUILT_IN.to_string();
    };
    match fs::read(&path) {
        Ok(bytes) if slot_ui::text::set_label_font(&bytes) => name,
        Ok(_) => {
            eprintln!(
                "slot: font: {} is not a font Slot can draw; using Slot's own",
                path.display()
            );
            BUILT_IN.to_string()
        }
        Err(e) => {
            eprintln!("slot: font: {}: {e}; using Slot's own", path.display());
            BUILT_IN.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fonts_come_from_slot_then_the_player_and_step_round_from_the_built_in_one() {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir_all(d.path().join("System/Fonts")).unwrap();
        fs::create_dir_all(d.path().join("Fonts")).unwrap();
        fs::create_dir_all(d.path().join("Config")).unwrap();
        fs::write(d.path().join("System/Fonts/GohuFont.ttf"), b"").unwrap();
        fs::write(d.path().join("Fonts/Mine.OTF"), b"").unwrap();
        fs::write(d.path().join("Fonts/GohuFont.ttf"), b"").unwrap();
        fs::write(d.path().join("Fonts/notes.txt"), b"").unwrap();
        let names: Vec<String> = available(d.path()).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["GohuFont", "Mine"]);
        assert_eq!(chosen(d.path()), BUILT_IN);
        assert_eq!(step(d.path(), BUILT_IN, true), "GohuFont");
        assert_eq!(step(d.path(), "Mine", true), BUILT_IN);
        assert_eq!(step(d.path(), BUILT_IN, false), "Mine");
        save(d.path(), "Mine").unwrap();
        assert_eq!(chosen(d.path()), "Mine");
    }

    #[test]
    fn a_font_that_is_not_one_leaves_the_built_in_font() {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir_all(d.path().join("Fonts")).unwrap();
        fs::create_dir_all(d.path().join("Config")).unwrap();
        fs::write(d.path().join("Fonts/Broken.ttf"), b"not a font").unwrap();
        save(d.path(), "Broken").unwrap();
        assert_eq!(load(d.path()), BUILT_IN);
        save(d.path(), "Gone").unwrap();
        assert_eq!(load(d.path()), BUILT_IN);
    }
}
