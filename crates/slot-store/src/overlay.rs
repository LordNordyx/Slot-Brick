//! Overlays: pictures laid over a game, with a clear window the game shows through, kept by
//! the player in Overlays/<system> (GB, GBC, GBA, NES, SNES). Slot ships none of its own.
use std::path::{Path, PathBuf};

use crate::Platform;

pub const OVERLAYS_DIR: &str = "Overlays";

/// The longest overlay file name kept: anything longer is not one a player typed.
const NAME_MAX: usize = 128;

/// A name an overlay may be saved under: a plain PNG file name in its system's folder, never a
/// path that could reach anywhere else on the card.
pub fn overlay_name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= NAME_MAX
        && !name.starts_with('.')
        && !name.contains(['/', '\\', '\n', '\r', '\0'])
        && name.to_ascii_lowercase().ends_with(".png")
}

pub fn overlays_dir(root: &Path, platform: Platform) -> PathBuf {
    root.join(OVERLAYS_DIR).join(platform.dir_name())
}

/// The overlays there are for `platform`, in name order.
pub fn overlays(root: &Path, platform: Platform) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(overlays_dir(root, platform)) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| overlay_name_ok(n))
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names
}

/// Where the overlay called `name` is for `platform`, when it is a name one may be and is there.
pub fn overlay_path(root: &Path, platform: Platform, name: &str) -> Option<PathBuf> {
    let path = overlays_dir(root, platform).join(name);
    (overlay_name_ok(name) && path.is_file()).then_some(path)
}

/// The next choice from `now` among Off (None) and the overlays there are, either way round.
pub fn step_overlay(root: &Path, platform: Platform, now: Option<&str>, right: bool) -> Option<String> {
    let mut choices: Vec<Option<String>> = vec![None];
    choices.extend(overlays(root, platform).into_iter().map(Some));
    let at = choices.iter().position(|c| c.as_deref() == now).unwrap_or(0);
    let n = choices.len();
    choices.swap_remove(if right { (at + 1) % n } else { (at + n - 1) % n })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_plain_png_name_is_an_overlay_name() {
        for ok in ["dmg-1024x768.png", "GBA SP.PNG", "a.png"] {
            assert!(overlay_name_ok(ok), "{ok}");
        }
        for bad in ["", "../x.png", "a/b.png", "a\\b.png", ".hidden.png", "x.jpg", "x.png\n", &"x".repeat(200)] {
            assert!(!overlay_name_ok(bad), "{bad:?}");
        }
    }

    #[test]
    fn overlays_are_listed_by_name_and_stepped_through_from_off() {
        let d = tempfile::tempdir().unwrap();
        let dir = overlays_dir(d.path(), Platform::Gb);
        std::fs::create_dir_all(&dir).unwrap();
        for f in ["b.png", "A.png", "notes.txt", ".c.png"] {
            std::fs::write(dir.join(f), b"x").unwrap();
        }
        std::fs::create_dir(dir.join("d.png")).unwrap();
        assert_eq!(overlays(d.path(), Platform::Gb), ["A.png", "b.png"]);
        assert!(overlays(d.path(), Platform::Gba).is_empty());
        let step = |now: Option<&str>, right| step_overlay(d.path(), Platform::Gb, now, right);
        assert_eq!(step(None, true).as_deref(), Some("A.png"));
        assert_eq!(step(Some("A.png"), true).as_deref(), Some("b.png"));
        assert_eq!(step(Some("b.png"), true), None, "it comes round to Off");
        assert_eq!(step(None, false).as_deref(), Some("b.png"));
        assert_eq!(step(Some("gone.png"), true).as_deref(), Some("A.png"), "a missing one counts as Off");
        assert_eq!(overlay_path(d.path(), Platform::Gb, "A.png"), Some(dir.join("A.png")));
        assert_eq!(overlay_path(d.path(), Platform::Gb, "../GB/A.png"), None);
        assert_eq!(overlay_path(d.path(), Platform::Gb, "gone.png"), None);
    }
}
