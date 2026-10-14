//! Matching ROM file names against No-Intro names, as libretro writes them (the
//! characters `&*/:`<>?\|"` replaced by `_`), for Cart Art's label lookup. ROMs on a card
//! are often named differently, so this matches on a normalised title and then picks the
//! best region and release.

use unicode_normalization::UnicodeNormalization;

/// A name as libretro stores it on disk.
pub fn libretro_name(name: &str) -> String {
    name.chars()
        .map(|c| if "&*/:`<>?\\|\"".contains(c) { '_' } else { c })
        .collect()
}

/// The title with tags, punctuation, accents, case and "the" removed:
/// "Legend of Zelda, The - Link's Awakening DX (USA, Europe) (SGB Enhanced)"
/// and "the_legend_of_zelda_links_awakening_dx" both give "legendofzeldalinksawakeningdx".
pub fn title_key(name: &str) -> String {
    let mut plain = String::with_capacity(name.len());
    let mut depth = 0u32;
    for c in name.nfd() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            '&' if depth == 0 => plain.push_str(" and "),
            '\'' => {}
            c if depth == 0 => plain.push(c),
            _ => {}
        }
    }
    plain
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty() && !w.eq_ignore_ascii_case("the"))
        .map(str::to_ascii_lowercase)
        .collect()
}

/// Every comma-separated tag inside ( ) and [ ].
fn tags(name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur: Option<String> = None;
    for c in name.chars() {
        match (c, &mut cur) {
            ('(' | '[', _) => cur = Some(String::new()),
            (')' | ']', Some(t)) => {
                out.extend(t.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()));
                cur = None;
            }
            (c, Some(t)) => t.push(c),
            _ => {}
        }
    }
    out
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Region {
    Usa,
    World,
    Europe,
    Japan,
    Germany,
    France,
    Spain,
    Italy,
}

fn regions(name: &str) -> Vec<Region> {
    let mut out = Vec::new();
    for tag in tags(name) {
        match tag.as_str() {
            "USA" => out.push(Region::Usa),
            "World" => out.push(Region::World),
            "Europe" => out.push(Region::Europe),
            "Japan" => out.push(Region::Japan),
            "Germany" => out.push(Region::Germany),
            "France" => out.push(Region::France),
            "Spain" => out.push(Region::Spain),
            "Italy" => out.push(Region::Italy),
            // GoodTools: (U), (E), (J), (G), (UE), (JU) ...
            t if !t.is_empty() && t.len() <= 3 && t.chars().all(|c| "UEJWGFSI".contains(c)) => {
                for c in t.chars() {
                    out.push(match c {
                        'U' => Region::Usa,
                        'E' => Region::Europe,
                        'J' => Region::Japan,
                        'G' => Region::Germany,
                        'F' => Region::France,
                        'S' => Region::Spain,
                        'I' => Region::Italy,
                        _ => Region::World,
                    });
                }
            }
            _ => {}
        }
    }
    out
}

const UNWANTED: [&str; 9] = [
    "Beta", "Proto", "Demo", "Sample", "Kiosk", "Pirate", "Unl", "Hack", "Aftermarket",
];
/// Re-releases: fine, but the original cartridge is preferred.
const REISSUE: [&str; 4] = ["Virtual Console", "Switch Online", "Alternate", "Promo"];

/// `asked`: the ROM's own tags. A ROM tagged (Hack) or (Beta) wants one of those.
fn score(candidate: &str, wanted: &[Region], asked: &[String]) -> i32 {
    let tags = tags(candidate);
    let mut score = 0;
    let unwanted = |t: &String| {
        UNWANTED
            .iter()
            .any(|u| t.starts_with(u) && !asked.iter().any(|a| a.starts_with(u)))
    };
    if tags.iter().any(unwanted) {
        score -= 100;
    }
    if tags.iter().any(|t| REISSUE.contains(&t.as_str())) {
        score -= 5;
    }
    let have = regions(candidate);
    if !wanted.is_empty() && have.iter().any(|r| wanted.contains(r)) {
        score += 50;
    }
    score += have
        .iter()
        .map(|r| match r {
            Region::Usa => 30,
            Region::World => 25,
            Region::Europe => 20,
            Region::Japan => 10,
            Region::Germany | Region::France | Region::Spain | Region::Italy => 15,
        })
        .max()
        .unwrap_or(0);
    score - tags.len() as i32
}

/// Below this many characters a title prefix is too vague to trust ("zelda").
const PREFIX_MIN: usize = 8;

/// The libretro name for a ROM named `stem`, if any of `names` is a good match.
pub fn best_match<'a>(stem: &str, names: &'a [String]) -> Option<&'a str> {
    matching(stem, names, true)
}

/// As best_match, but only a name with the very same title: never a longer one that begins
/// with it. For lists full of hacks and homebrew ("Super Metroid Legacy"), where a game
/// missing from the list must not take a look-alike's place.
pub fn same_title_match<'a>(stem: &str, names: &'a [String]) -> Option<&'a str> {
    matching(stem, names, false)
}

fn matching<'a>(stem: &str, names: &'a [String], longer: bool) -> Option<&'a str> {
    let exact = libretro_name(stem);
    if let Some(n) = names.iter().find(|n| n.eq_ignore_ascii_case(&exact)) {
        return Some(n);
    }
    let key = title_key(stem);
    if key.is_empty() {
        return None;
    }
    let wanted = regions(stem);
    let asked = tags(stem);
    let pick = |pool: Vec<&'a String>| {
        pool.into_iter()
            .max_by(|a, b| {
                score(a, &wanted, &asked)
                    .cmp(&score(b, &wanted, &asked))
                    .then(b.len().cmp(&a.len()))
                    .then(b.cmp(a))
            })
            .map(String::as_str)
    };
    // A ROM that says it is a hack only gets art made for a hack, never the original's.
    let is_hack = |tags: &[String]| tags.iter().any(|t| t.starts_with("Hack"));
    let hack = is_hack(&asked);
    let keyed: Vec<(&String, String)> = names
        .iter()
        .filter(|n| !hack || is_hack(&tags(n)))
        .map(|n| (n, title_key(n)))
        .collect();
    let same: Vec<&String> = keyed.iter().filter(|(_, k)| *k == key).map(|(n, _)| *n).collect();
    if !same.is_empty() {
        return pick(same);
    }
    if !longer || key.len() < PREFIX_MIN {
        return None;
    }
    // "pokemon_emerald" -> "Pokemon - Emerald Version": the shortest longer title wins.
    // Never the other way round, or "golden_sun_2" would get "Golden Sun".
    let near: Vec<(&String, usize)> = keyed
        .iter()
        .filter(|(_, k)| k.starts_with(&key))
        .map(|(n, k)| (*n, k.len() - key.len()))
        .collect();
    let closest = near.iter().map(|(_, d)| *d).min()?;
    pick(near.into_iter().filter(|(_, d)| *d == closest).map(|(n, _)| n).collect())
}
