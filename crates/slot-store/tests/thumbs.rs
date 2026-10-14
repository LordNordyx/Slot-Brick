use slot_store::{best_match, libretro_name, same_title_match, title_key};

/// Real No-Intro names, as libretro writes them.
const GBA: &[&str] = &[
    "Golden Sun (France)",
    "Golden Sun (Germany)",
    "Golden Sun (USA, Europe)",
    "Golden Sun - The Lost Age (USA, Europe)",
    "Legend of Zelda, The - The Minish Cap (Europe) (En,Fr,De,Es,It)",
    "Legend of Zelda, The - The Minish Cap (USA) (Demo) (Kiosk)",
    "Legend of Zelda, The - The Minish Cap (USA)",
    "Metroid - Zero Mission (Europe) (En,Fr,De,Es,It) (Virtual Console)",
    "Metroid - Zero Mission (Europe) (En,Fr,De,Es,It)",
    "Metroid - Zero Mission (Japan)",
    "Metroid - Zero Mission (USA) (Virtual Console)",
    "Metroid - Zero Mission (USA)",
    "Pokemon - Emerald Version (USA, Europe) (Alternate)",
    "Pokemon - Emerald Version (USA, Europe)",
    "Tetris Worlds (Europe) (En,Es,It)",
    "Tetris Worlds (Japan)",
    "Tetris Worlds (USA)",
];

const GBC: &[&str] = &[
    "Legend of Zelda, The - Link's Awakening DX (France) (SGB Enhanced) (GB Compatible)",
    "Legend of Zelda, The - Link's Awakening DX (USA, Europe) (Rev 1) (SGB Enhanced) (GB Compatible)",
    "Legend of Zelda, The - Link's Awakening DX (USA, Europe) (SGB Enhanced) (GB Compatible)",
    "Legend of Zelda, The - Link's Awakening DX - Redux (USA)",
    "Pokemon - Crystal Version (Australia)",
    "Pokemon - Crystal Version (USA)",
    "Pokemon - Gold Version (USA, Europe) (SGB Enhanced) (GB Compatible)",
    "Tetris DX (World) (SGB Enhanced) (GB Compatible)",
];

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn find(stem: &str, list: &[&str]) -> Option<String> {
    best_match(stem, &names(list)).map(str::to_string)
}

#[test]
fn a_no_intro_name_matches_itself() {
    assert_eq!(find("Metroid - Zero Mission (Japan)", GBA).as_deref(), Some("Metroid - Zero Mission (Japan)"));
}

#[test]
fn exact_matching_ignores_case_and_libretro_substitutions() {
    let list = ["Mario _ Luigi - Superstar Saga (USA)"];
    assert_eq!(find("mario & luigi - superstar saga (usa)", &list).as_deref(), Some(list[0]));
}

#[test]
fn a_bare_title_prefers_usa_and_the_original_release() {
    assert_eq!(find("Metroid - Zero Mission", GBA).as_deref(), Some("Metroid - Zero Mission (USA)"));
    assert_eq!(find("Pokemon - Emerald Version", GBA).as_deref(), Some("Pokemon - Emerald Version (USA, Europe)"));
}

#[test]
fn the_region_in_the_rom_name_wins() {
    assert_eq!(
        find("Metroid - Zero Mission (Europe)", GBA).as_deref(),
        Some("Metroid - Zero Mission (Europe) (En,Fr,De,Es,It)")
    );
    assert_eq!(find("Tetris Worlds (J)", GBA).as_deref(), Some("Tetris Worlds (Japan)"));
}

#[test]
fn goodtools_and_scene_names_match() {
    assert_eq!(
        find("Legend of Zelda, The - The Minish Cap (U) [!]", GBA).as_deref(),
        Some("Legend of Zelda, The - The Minish Cap (USA)")
    );
    assert_eq!(
        find("the_legend_of_zelda_the_minish_cap", GBA).as_deref(),
        Some("Legend of Zelda, The - The Minish Cap (USA)")
    );
    assert_eq!(
        find("Zelda - Link's Awakening DX", GBC),
        None,
        "a different title must not match"
    );
}

#[test]
fn a_shortened_name_finds_the_full_title() {
    assert_eq!(find("pokemon_emerald", GBA).as_deref(), Some("Pokemon - Emerald Version (USA, Europe)"));
    assert_eq!(find("Pokemon Crystal", GBC).as_deref(), Some("Pokemon - Crystal Version (USA)"));
}

#[test]
fn a_longer_rom_name_never_gets_a_shorter_title() {
    assert_eq!(find("Golden Sun 2", GBA), None);
    assert_eq!(find("Golden Sun - The Lost Age", GBA).as_deref(), Some("Golden Sun - The Lost Age (USA, Europe)"));
}

#[test]
fn demos_and_short_vague_names_are_avoided() {
    assert_eq!(find("Zelda", GBA), None);
    assert_ne!(
        find("Legend of Zelda, The - The Minish Cap", GBA).as_deref(),
        Some("Legend of Zelda, The - The Minish Cap (USA) (Demo) (Kiosk)")
    );
}

#[test]
fn accents_and_ampersands_are_normalised() {
    assert_eq!(title_key("Pokémon - Émeraude"), title_key("Pokemon - Emeraude"));
    assert_eq!(title_key("Mario & Luigi"), title_key("Mario and Luigi"));
    assert_eq!(title_key("Legend of Zelda, The (USA) [!]"), "legendofzelda");
}

#[test]
fn libretro_replaces_reserved_characters() {
    assert_eq!(libretro_name("Mario & Luigi: Superstar Saga?"), "Mario _ Luigi_ Superstar Saga_");
}

#[test]
fn hacks_match_only_when_listed_and_win_when_the_rom_says_hack() {
    let list = [
        "Pokemon - Emerald Version (USA, Europe)",
        "Pokemon - Glazed Version (Hack)",
        "Pokemon Glazed [Hack by Redriders180]",
        "Pokemon - Liquid Crystal (Hack)",
    ];
    assert_eq!(find("Pokemon Glazed", &list).as_deref(), Some("Pokemon Glazed [Hack by Redriders180]"));
    assert_eq!(find("Pokemon - Liquid Crystal (Hack)", &list).as_deref(), Some("Pokemon - Liquid Crystal (Hack)"));
    assert_eq!(find("Pokemon Radical Red", &list), None, "an unlisted hack keeps its own label");
    assert_eq!(find("Pokemon - Emerald (Hack)", &list), None, "never the original's art for a hack");
    let both = ["Tetris (World) (Beta)", "Tetris (World) (Hack)"];
    assert_eq!(find("Tetris (Hack)", &both).as_deref(), Some("Tetris (World) (Hack)"));
}

#[test]
fn goodtools_language_regions_pick_that_release() {
    let list = [
        "Pokemon - Red Version (USA, Europe) (SGB Enhanced)",
        "Pokemon - Rote Edition (Germany) (SGB Enhanced)",
        "Pokemon - Red Version (Germany) (SGB Enhanced)",
    ];
    assert_eq!(find("Pokemon - Red Version (G)", &list).as_deref(), Some(list[2]));
    assert_eq!(find("Pokemon - Red Version (U)", &list).as_deref(), Some(list[0]));
}

#[test]
fn a_same_title_match_never_takes_a_longer_title() {
    let list = names(&["Super Metroid Legacy (World)", "Super Mario Kart (USA)", "Super Mario Kart - Horizons (World)"]);
    assert_eq!(best_match("Super Metroid (Japan, USA) (En,Ja)", &list), Some("Super Metroid Legacy (World)"));
    assert_eq!(same_title_match("Super Metroid (Japan, USA) (En,Ja)", &list), None);
    assert_eq!(same_title_match("Super Mario Kart (USA)", &list), Some("Super Mario Kart (USA)"));
    assert_eq!(same_title_match("super_mario_kart", &list), Some("Super Mario Kart (USA)"));
}
