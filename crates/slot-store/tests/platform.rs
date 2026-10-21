use slot_store::Platform;

#[test]
fn every_platform_has_a_directory() {
    let names: Vec<&str> = Platform::ALL.iter().map(|p| p.dir_name()).collect();
    assert_eq!(names, vec!["GBA", "GB", "GBC", "NES", "SNES", "PS1"]);
}

#[test]
fn every_platform_is_a_shelf_of_its_own() {
    assert_eq!(
        Platform::ALL,
        [Platform::Gba, Platform::Gb, Platform::Gbc, Platform::Nes, Platform::Snes, Platform::Ps1]
    );
    for (i, p) in Platform::ALL.iter().enumerate() {
        assert!(
            !Platform::ALL[..i].contains(p),
            "{p:?} is named twice in the ring, so two shelves would hold one platform"
        );
    }
}

#[test]
fn each_platform_takes_only_its_own_extensions() {
    assert!(Platform::Gba.accepts(std::path::Path::new("Metroid Fusion.gba")));
    assert!(!Platform::Gba.accepts(std::path::Path::new("Tetris.gb")));
    assert!(Platform::Gb.accepts(std::path::Path::new("Tetris.gb")));
    assert!(Platform::Gb.accepts(std::path::Path::new("Tetris.gbc")));
    assert!(!Platform::Gb.accepts(std::path::Path::new("Metroid Fusion.gba")));
    assert!(Platform::Gba.accepts(std::path::Path::new("Shrek.GBA")));
    assert!(Platform::Nes.accepts(std::path::Path::new("Super Mario Bros. (World).nes")));
    assert!(!Platform::Nes.accepts(std::path::Path::new("Tetris.gb")));
    assert!(!Platform::Gba.accepts(std::path::Path::new("Zelda.NES")));
    for disc in ["Ridge Racer.cue", "Ridge Racer.chd", "Ridge Racer.PBP", "Final Fantasy VII.m3u"] {
        assert!(Platform::Ps1.accepts(std::path::Path::new(disc)), "{disc}");
    }
    assert!(!Platform::Ps1.accepts(std::path::Path::new("Ridge Racer.sfc")));
}

#[test]
fn a_nes_picture_fits_its_canvas_and_the_game_boys_keep_theirs() {
    assert_eq!(Platform::Nes.picture(), (256, 224));
    assert_eq!(Platform::Nes.canvas(), (256, 240));
    for p in [Platform::Gba, Platform::Gb, Platform::Gbc] {
        assert_eq!(p.canvas(), (240, 160), "{p:?}");
    }
    // Every PlayStation frame is stretched over a 4:3 canvas, its picture all of it.
    assert_eq!(Platform::Ps1.picture(), Platform::Ps1.canvas());
}
