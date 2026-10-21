//! What Slot keeps about each game besides its saves: how long it has been played, when it was
//! last played, and whether it is a favorite. Config/library.txt, a line a game:
//! `GBA<tab>Pixel Racer<tab>seconds played<tab>last played (unix)<tab>favorite (0/1)`.
use std::collections::HashMap;
use std::path::Path;

use crate::atomic::atomic_write;
use crate::platform::Platform;
use crate::scan::Cart;

pub const LIBRARY_FILE: &str = "Config/library.txt";

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct GameRecord {
    pub played_secs: u64,
    /// UTC seconds when it was last started; 0 for never.
    pub last_played: i64,
    pub favorite: bool,
}

/// How each shelf is ordered.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum LibrarySort {
    #[default]
    Az,
    /// The game played last first; games never played after, A–Z.
    Recent,
    /// The most played first; games never played after, A–Z.
    MostPlayed,
}

impl LibrarySort {
    pub const ALL: [LibrarySort; 3] = [LibrarySort::Az, LibrarySort::Recent, LibrarySort::MostPlayed];

    pub fn name(self) -> &'static str {
        match self {
            LibrarySort::Az => "az",
            LibrarySort::Recent => "recent",
            LibrarySort::MostPlayed => "most",
        }
    }

    pub fn parse(value: &str) -> Option<LibrarySort> {
        LibrarySort::ALL.into_iter().find(|s| s.name() == value)
    }
}

#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct Library {
    games: HashMap<(Platform, String), GameRecord>,
}

impl Library {
    pub fn read(root: &Path) -> Library {
        std::fs::read_to_string(root.join(LIBRARY_FILE))
            .map(|text| Library::parse(&text))
            .unwrap_or_default()
    }

    pub fn write(&self, root: &Path) -> std::io::Result<()> {
        atomic_write(&root.join(LIBRARY_FILE), self.text().as_bytes())
    }

    pub fn parse(text: &str) -> Library {
        let mut games = HashMap::new();
        for line in text.lines() {
            let fields: Vec<&str> = line.split('\t').collect();
            let [platform, stem, secs, last, favorite] = fields[..] else {
                continue;
            };
            let Some(platform) = Platform::ALL.into_iter().find(|p| p.dir_name() == platform) else {
                continue;
            };
            let (Ok(played_secs), Ok(last_played)) = (secs.parse(), last.parse()) else {
                continue;
            };
            let record = GameRecord { played_secs, last_played, favorite: favorite == "1" };
            games.insert((platform, stem.to_string()), record);
        }
        Library { games }
    }

    /// One line a game that has anything to keep, in a steady order.
    pub fn text(&self) -> String {
        let mut lines: Vec<String> = self
            .games
            .iter()
            .filter(|(_, r)| **r != GameRecord::default())
            .map(|((platform, stem), r)| {
                format!("{}\t{stem}\t{}\t{}\t{}", platform.dir_name(), r.played_secs, r.last_played, r.favorite as u8)
            })
            .collect();
        lines.sort();
        lines.into_iter().map(|l| l + "\n").collect()
    }

    pub fn get(&self, platform: Platform, stem: &str) -> GameRecord {
        self.games.get(&(platform, stem.to_string())).copied().unwrap_or_default()
    }

    pub fn get_mut(&mut self, platform: Platform, stem: &str) -> &mut GameRecord {
        self.games.entry((platform, stem.to_string())).or_default()
    }

    pub fn any_favorite(&self, carts: &[Cart]) -> bool {
        carts.iter().any(|c| self.get(c.platform, &c.stem).favorite)
    }
}

/// `carts`, in A–Z order (as `scan` gives them), put in `sort`'s order, and only the favorites
/// kept when `favorites_only`.
pub fn arrange(carts: &mut Vec<Cart>, sort: LibrarySort, favorites_only: bool, library: &Library) {
    if favorites_only {
        carts.retain(|c| library.get(c.platform, &c.stem).favorite);
    }
    // Stable: games alike in the order keep their A–Z order.
    match sort {
        LibrarySort::Az => {}
        LibrarySort::Recent => carts.sort_by_key(|c| std::cmp::Reverse(library.get(c.platform, &c.stem).last_played)),
        LibrarySort::MostPlayed => carts.sort_by_key(|c| std::cmp::Reverse(library.get(c.platform, &c.stem).played_secs)),
    }
}

/// Play time as the shelf prints it: in minutes ("12M", "200M"), or with `hours`, in hours and
/// minutes ("12M", "3H20M", "3H05M"). Nothing under a minute.
pub fn played_text(secs: u64, hours: bool) -> Option<String> {
    let mins = secs / 60;
    match (mins, hours) {
        (0, _) => None,
        (m, false) | (m @ 0..=59, true) => Some(format!("{m}M")),
        (m, true) => Some(format!("{}H{:02}M", m / 60, m % 60)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cart(platform: Platform, stem: &str) -> Cart {
        Cart {
            platform,
            stem: stem.into(),
            rom: format!("{stem}.gba").into(),
            label: None,
            famicom_label: None,
            title: String::new(),
            code: String::new(),
            shell: None,
        }
    }

    fn stems(carts: &[Cart]) -> Vec<&str> {
        carts.iter().map(|c| c.stem.as_str()).collect()
    }

    #[test]
    fn records_round_trip_and_bad_lines_are_skipped() {
        let mut lib = Library::default();
        *lib.get_mut(Platform::Gba, "Pixel Racer") = GameRecord { played_secs: 3700, last_played: 1_800_000_000, favorite: true };
        lib.get_mut(Platform::Gb, "Untouched");
        let text = lib.text();
        assert_eq!(text, "GBA\tPixel Racer\t3700\t1800000000\t1\n");
        let again = Library::parse(&format!("{text}nonsense\nXYZ\ta\t1\t2\t1\nGB\tx\tlots\t2\t0\n"));
        assert_eq!(again.get(Platform::Gba, "Pixel Racer"), lib.get(Platform::Gba, "Pixel Racer"));
        assert_eq!(again.games.len(), 1);
    }

    #[test]
    fn shelves_sort_by_last_played_or_most_played_with_the_rest_a_to_z() {
        let all = [cart(Platform::Gba, "Alpha"), cart(Platform::Gba, "Beta"), cart(Platform::Gba, "Gamma"), cart(Platform::Gba, "Delta")];
        let mut lib = Library::default();
        lib.get_mut(Platform::Gba, "Gamma").last_played = 20;
        lib.get_mut(Platform::Gba, "Gamma").played_secs = 10;
        lib.get_mut(Platform::Gba, "Beta").last_played = 10;
        lib.get_mut(Platform::Gba, "Beta").played_secs = 500;
        lib.get_mut(Platform::Gba, "Delta").favorite = true;
        let order = |sort, favorites| {
            let mut carts = all.to_vec();
            arrange(&mut carts, sort, favorites, &lib);
            stems(&carts).join(" ")
        };
        assert_eq!(order(LibrarySort::Az, false), "Alpha Beta Gamma Delta");
        assert_eq!(order(LibrarySort::Recent, false), "Gamma Beta Alpha Delta");
        assert_eq!(order(LibrarySort::MostPlayed, false), "Beta Gamma Alpha Delta");
        assert_eq!(order(LibrarySort::Recent, true), "Delta");
        assert!(lib.any_favorite(&all) && !Library::default().any_favorite(&all));
    }

    #[test]
    fn play_time_reads_in_minutes_or_hours_and_minutes() {
        for hours in [false, true] {
            assert_eq!(played_text(59, hours), None);
            assert_eq!(played_text(12 * 60 + 5, hours).as_deref(), Some("12M"));
        }
        assert_eq!(played_text(3 * 3600 + 20 * 60, false).as_deref(), Some("200M"));
        assert_eq!(played_text(3 * 3600 + 20 * 60, true).as_deref(), Some("3H20M"));
        assert_eq!(played_text(3 * 3600 + 5 * 60, true).as_deref(), Some("3H05M"));
        assert_eq!(played_text(3 * 3600, true).as_deref(), Some("3H00M"));
    }

    #[test]
    fn sort_names_round_trip() {
        for sort in LibrarySort::ALL {
            assert_eq!(LibrarySort::parse(sort.name()), Some(sort));
        }
    }
}
