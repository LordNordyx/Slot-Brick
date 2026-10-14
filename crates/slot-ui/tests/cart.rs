use slot_store::{scan, Platform};
use slot_ui::{
    cart_face, clean_label, foot_y, gb_label_panel, gb_silhouette, label_colour, label_panel,
    label_text, rest_y, seated_box, shell_for, silhouette, Finish, GbShell, CART_H, CART_W,
    GB_CART_H, GB_CART_W, GB_LABEL_H, GB_LABEL_W, GB_LABEL_X, GB_LABEL_Y, LABEL_H, LABEL_W,
    LABEL_X, LABEL_Y, MOUTH_H, NES_CART_H, NES_CART_W, OUT_H, PLATE_H,
};
use tempfile::TempDir;

fn gx(v: u32) -> u32 {
    let w = seated_box(Platform::Gb).0;
    (v * GB_CART_W + w / 2) / w
}

fn gy(v: u32) -> u32 {
    let h = seated_box(Platform::Gb).1;
    (v * GB_CART_H + h / 2) / h
}

fn tmp_root() -> TempDir {
    let d = tempfile::tempdir().expect("tempdir");
    for sub in [
        "Games",
        "Games/GBA",
        "Labels",
        "Saves",
        "States",
        "System",
        "Config",
    ] {
        std::fs::create_dir(d.path().join(sub)).expect("create content dir");
    }
    d
}

fn write_gb_rom(d: &TempDir, dir: &str, name: &str, cgb: u8) {
    let mut rom = vec![0u8; 0x150];
    rom[0x143] = cgb;
    let games = d.path().join("Games").join(dir);
    std::fs::create_dir_all(&games).expect("create games dir");
    std::fs::write(games.join(name), rom).expect("write rom");
}

fn write_rom(d: &TempDir, name: &str, title: &str) {
    let mut rom = vec![0u8; 0x100];
    rom[0xa0..0xa0 + title.len()].copy_from_slice(title.as_bytes());
    std::fs::write(d.path().join("Games/GBA").join(name), rom).expect("write rom");
}

fn write_label(d: &TempDir, name: &str, w: u32, h: u32, px: impl Fn(u32, u32) -> [u8; 3]) {
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            rgb.extend_from_slice(&px(x, y));
        }
    }
    std::fs::create_dir_all(d.path().join("Labels/GBA")).expect("create labels dir");
    let f = std::fs::File::create(d.path().join("Labels/GBA").join(name)).expect("create label");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .expect("png header")
        .write_image_data(&rgb)
        .expect("png data");
}

fn pixel(face: &slot_ui::CartFace, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * face.w + x) * 4) as usize;
    [face.rgba[i], face.rgba[i + 1], face.rgba[i + 2]]
}

fn pixel_alpha(face: &slot_ui::CartFace, x: u32, y: u32) -> u8 {
    face.rgba[((y * face.w + x) * 4 + 3) as usize]
}

fn label_pixel(face: &slot_ui::CartFace, x: u32, y: u32) -> [u8; 3] {
    pixel(face, LABEL_X + x, LABEL_Y + y)
}

#[test]
fn a_malformed_label_falls_back_to_a_generated_one() {
    let d = tmp_root();
    write_rom(&d, "Broken.gba", "BROKEN");
    std::fs::create_dir_all(d.path().join("Labels/GBA")).unwrap();
    std::fs::write(d.path().join("Labels/GBA/Broken.png"), b"not a png").unwrap();
    let cart = &scan(d.path()).unwrap()[0];
    let face = cart_face(cart);
    assert_eq!((face.w, face.h), (CART_W, CART_H));
    assert!(face.rgba.iter().any(|b| *b != 0), "face is blank");
}

#[test]
fn label_colour_is_stable_across_calls() {
    assert_eq!(label_colour("POKEMON EMER"), label_colour("POKEMON EMER"));
    assert_ne!(label_colour("POKEMON EMER"), label_colour("ADVANCEWARS"));
}

#[test]
fn a_label_that_decodes_is_what_the_face_shows() {
    let d = tmp_root();
    write_rom(&d, "Labelled.gba", "LABELLED");
    write_label(&d, "Labelled.png", 64, 64, |_, _| [0xd0, 0x20, 0xa0]);
    let face = cart_face(&scan(d.path()).unwrap()[0]);
    assert_eq!((face.w, face.h), (CART_W, CART_H));
    for y in [0, LABEL_H / 2, LABEL_H - 1] {
        for x in [0, LABEL_W / 2, LABEL_W - 1] {
            assert_eq!(label_pixel(&face, x, y), [0xd0, 0x20, 0xa0], "at {x},{y}");
        }
    }
}

#[test]
fn a_label_of_the_wrong_aspect_is_cropped_not_squashed() {
    let d = tmp_root();
    write_rom(&d, "Tall.gba", "TALL");
    write_label(&d, "Tall.png", 160, 480, |_, y| match y / 160 {
        0 => [0xff, 0x00, 0x00],
        1 => [0xff, 0xff, 0xff],
        _ => [0x00, 0x00, 0xff],
    });
    let face = cart_face(&scan(d.path()).unwrap()[0]);
    for y in 0..LABEL_H {
        for x in 0..LABEL_W {
            assert_eq!(label_pixel(&face, x, y), [0xff, 0xff, 0xff], "at {x},{y}");
        }
    }
}

#[test]
fn a_long_title_stays_inside_the_label() {
    for stem in [
        "Supercalifragilisticexpialidocious Anniversary Edition",
        "Super Mario Kart",
    ] {
        let d = tmp_root();
        std::fs::write(d.path().join(format!("Games/GBA/{stem}.gba")), [0u8; 8]).unwrap();
        let face = cart_face(&scan(d.path()).unwrap()[0]);
        let bg = label_colour(stem);
        let margin = 5;
        for y in 0..LABEL_H {
            for x in 0..LABEL_W {
                let edge =
                    x < margin || y < margin || x >= LABEL_W - margin || y >= LABEL_H - margin;
                if edge {
                    assert_eq!(
                        label_pixel(&face, x, y),
                        bg,
                        "{stem}: text spills into the border at {x},{y}"
                    );
                }
            }
        }
        assert!(
            (0..LABEL_H * LABEL_W).any(|i| label_pixel(&face, i % LABEL_W, i / LABEL_W) != bg),
            "{stem}: no text was drawn"
        );
    }
}

#[test]
fn the_cart_box_matches_the_traced_outline() {
    let ratio = CART_W as f32 / CART_H as f32;
    assert!(
        (ratio - 1.703).abs() < 0.02,
        "aspect {ratio:.3}, the svg is being stretched"
    );
}

#[test]
fn the_label_is_wide_and_sits_low() {
    let (x0, y0, x1, y1) = label_panel(CART_W, CART_H);
    let (w, h) = (CART_W as f32, CART_H as f32);
    let side = x0 as f32 / w;
    let top = y0 as f32 / h;
    let bottom = 1.0 - y1 as f32 / h;
    let width = (x1 - x0) as f32 / w;

    assert!(
        width > 0.70,
        "the label is only {:.0}% of the cart wide, that reads as a panel not a label",
        width * 100.0
    );
    assert!(
        top > 0.18,
        "only {:.0}% of grip band above the label",
        top * 100.0
    );
    assert!(
        top > side * 1.5,
        "top band {top:.2} against side margin {side:.2}: that is a uniform border"
    );
    assert!(
        top > bottom * 1.6,
        "the label is not sitting low, top {top:.2} bottom {bottom:.2}"
    );
}

#[test]
fn the_body_is_narrower_than_its_grip_ears() {
    let m = silhouette(CART_W, CART_H);
    let solid = |x: u32, y: u32| m[(y * CART_W + x) as usize] > 128;
    let width_at = |y: u32| (0..CART_W).filter(|&x| solid(x, y)).count();

    let ears = width_at(CART_H / 12);
    let body = width_at(CART_H / 2);
    assert!(
        ears > body,
        "top spans {ears}px and the body {body}px: the grip ears are missing"
    );
    assert!(
        ears - body >= 3,
        "the ears stick out by only {}px, which will not read at all",
        ears - body
    );
    assert!(solid(CART_W / 2, CART_H / 2), "the middle is not solid");
    assert!(
        solid(CART_W / 2, CART_H - 2),
        "the bottom edge is not straight"
    );
}

#[test]
fn cart_faces_are_clipped_to_the_silhouette() {
    let d = tmp_root();
    write_rom(&d, "Emerald.gba", "EMERALD");
    let cart = &scan(d.path()).unwrap()[0];
    let f = cart_face(cart);
    let px = |x: u32, y: u32| f.rgba[((y * f.w + x) * 4 + 3) as usize];
    assert_eq!(
        px(2, 2),
        0,
        "the rounded corner is opaque, the mask was not applied"
    );
    assert!(px(f.w / 2, f.h / 2) > 250);
}

#[test]
fn a_supplied_label_is_clipped_to_the_silhouette_too() {
    let d = tmp_root();
    write_rom(&d, "Labelled.gba", "LABELLED");
    write_label(&d, "Labelled.png", LABEL_W, LABEL_H, |_, _| {
        [0x40, 0x80, 0xc0]
    });
    let f = cart_face(&scan(d.path()).unwrap()[0]);
    let px = |x: u32, y: u32| f.rgba[((y * f.w + x) * 4 + 3) as usize];
    assert_eq!(px(2, 2), 0, "the label escaped the rounded corner");
    assert!(px(f.w / 2, f.h / 2) > 250);
}

#[test]
fn the_label_is_the_filename_without_its_tags() {
    let cases = [
        (
            "Pokemon - Emerald Version (USA, Europe)",
            "Pokemon Emerald Version",
        ),
        (
            "Pokemon - LeafGreen Version (USA, Europe) (Rev 1)",
            "Pokemon LeafGreen Version",
        ),
        ("Shrek (USA) (Rev 6)", "Shrek"),
        ("Metroid Fusion", "Metroid Fusion"),
        ("Button Test", "Button Test"),
        ("Pokemon - Corrupt", "Pokemon Corrupt"),
        ("Some Game [!]", "Some Game"),
        ("Spaced   Out  (USA)", "Spaced Out"),
    ];
    for (stem, want) in cases {
        assert_eq!(clean_label(stem), want, "for {stem}");
    }
}

#[test]
fn an_unspaced_hyphen_survives() {
    assert_eq!(clean_label("Spider-Man 2 (USA)"), "Spider-Man 2");
    assert_eq!(
        clean_label("Wario Land 4 - Time Attack"),
        "Wario Land 4 Time Attack"
    );
}

#[test]
fn a_name_that_is_all_tags_falls_back_rather_than_going_blank() {
    assert_eq!(clean_label("(USA) (Rev 1)"), "(USA) (Rev 1)");
    assert_eq!(clean_label(""), "");
}

#[test]
fn the_header_title_no_longer_reaches_the_label() {
    let d = tmp_root();
    write_rom(
        &d,
        "Pokemon - Emerald Version (USA, Europe).gba",
        "POKEMON EMER",
    );
    let cart = &scan(d.path()).unwrap()[0];
    assert_eq!(label_text(cart), "Pokemon Emerald Version");
}

#[test]
fn two_regions_of_one_game_get_the_same_generated_colour() {
    let a = label_colour(&clean_label("Pokemon - Ruby Version (USA, Europe) (Rev 2)"));
    let b = label_colour(&clean_label("Pokemon - Ruby Version (Japan)"));
    assert_eq!(a, b, "the same game came out two colours");
}

#[test]
fn a_rom_with_no_header_title_is_labelled_from_its_stem() {
    let d = tmp_root();
    std::fs::write(d.path().join("Games/GBA/Homebrew Demo.gba"), [0u8; 8]).unwrap();
    let face = cart_face(&scan(d.path()).unwrap()[0]);
    let bg = label_colour("Homebrew Demo");
    assert!(
        (0..LABEL_H * LABEL_W).any(|i| label_pixel(&face, i % LABEL_W, i / LABEL_W) != bg),
        "an untitled rom got a blank label"
    );
}

#[test]
fn the_game_boy_pak_body_has_the_scanned_aspect() {
    let (sw, _) = seated_box(Platform::Gba);
    let body = |w: u32, h: u32| (w as f64 * 227.0 / 240.0) / h as f64;
    assert!(
        (body(GB_CART_W, GB_CART_H) - 0.877).abs() < 0.01,
        "the pak body is {:.3}:1, not a real pak's 0.877",
        body(GB_CART_W, GB_CART_H)
    );
    let (gw, gh) = seated_box(Platform::Gb);
    assert_eq!(
        gw, sw,
        "both paks are 57 mm wide, so they seat at one width"
    );
    assert!((body(gw, gh) - 0.877).abs() < 0.01);
}

#[test]
fn the_game_boy_outline_has_parallel_sides_and_no_grip_ears() {
    for shell in [GbShell::Notched, GbShell::Rounded] {
        let gb = gb_silhouette(shell, GB_CART_W, GB_CART_H);
        let width_at = |y: u32| {
            (0..GB_CART_W)
                .filter(|&x| gb[(y * GB_CART_W + x) as usize] > 128)
                .count()
        };
        let (top, middle, bottom) = (
            width_at(GB_CART_H / 8),
            width_at(GB_CART_H / 2),
            width_at(GB_CART_H * 7 / 8),
        );
        assert_eq!(top, middle, "{shell:?}: wider at the top than the middle");
        assert_eq!(middle, bottom, "{shell:?}: tapers toward the bottom");

        let gba = silhouette(CART_W, CART_H);
        let gba_body = (0..CART_W)
            .filter(|&x| gba[((CART_H / 2) * CART_W + x) as usize] > 128)
            .count();
        let gba_body = (gba_body * GB_CART_W as usize + CART_W as usize / 2) / CART_W as usize;
        assert!(
            middle.abs_diff(gba_body) <= 2,
            "{shell:?}: the pak's body is {middle}px against the GBA body's {gba_body}px, \
             and both are 57 mm"
        );
    }
}

#[test]
fn the_colour_only_shell_loses_the_notch_and_rounds_the_corners() {
    let notched = gb_silhouette(GbShell::Notched, GB_CART_W, GB_CART_H);
    let rounded = gb_silhouette(GbShell::Rounded, GB_CART_W, GB_CART_H);
    fn covered(m: &[u8], y: u32) -> Vec<u32> {
        (0..GB_CART_W)
            .filter(|&x| m[(y * GB_CART_W + x) as usize] > 128)
            .collect()
    }
    let width_at = |m: &[u8], y: u32| covered(m, y).len();
    let left_at = |m: &[u8], y: u32| covered(m, y)[0];
    let right_at = |m: &[u8], y: u32| *covered(m, y).last().expect("an empty row");

    let y = 6;
    assert!(
        right_at(&rounded, y) > right_at(&notched, y) + 10,
        "the clear pak ends at {} against the notched {} at row {y}: the notch is not cut",
        right_at(&rounded, y),
        right_at(&notched, y)
    );
    assert!(
        left_at(&rounded, 0) > left_at(&notched, 0) + 10,
        "the top row starts at {} against the notched {}: the corners are not rounded",
        left_at(&rounded, 0),
        left_at(&notched, 0)
    );
    for y in 30..GB_CART_H {
        assert_eq!(
            width_at(&notched, y),
            width_at(&rounded, y),
            "the shells disagree at row {y}, which is below the shoulder"
        );
    }
}

#[test]
fn the_game_boy_label_well_is_near_square_and_sits_under_the_lettering_plate() {
    let (x0, y0, x1, y1) = gb_label_panel(GB_CART_W, GB_CART_H);
    let aspect = (x1 - x0) as f32 / (y1 - y0) as f32;
    assert!(
        (aspect - 1.135).abs() < 0.02,
        "the well is {aspect:.2}:1, which is not the 42x37 label's shape"
    );
    assert_eq!(x0, GB_CART_W - x1, "the well is not centred across the pak");
    let (above, below) = (y0, GB_CART_H - y1);
    assert!(
        above > below * 2,
        "{above}px above the label and {below}px below: the shoulder has lost its plate"
    );
}

#[test]
fn the_colour_only_shell_has_a_rolled_top_edge_and_the_older_mould_does_not() {
    let d = tmp_root();
    write_gb_rom(&d, "GBC", "Rolled.gbc", 0xc0);
    write_gb_rom(&d, "GB", "Flat.gb", 0x00);
    let carts = scan(d.path()).expect("scan");
    let face = |stem: &str| {
        cart_face(
            carts
                .iter()
                .find(|c| c.stem == stem)
                .unwrap_or_else(|| panic!("no cart {stem}")),
        )
    };
    let down = |f: &slot_ui::CartFace, y: u32| pixel(f, GB_CART_W / 2, y)[1] as i32;

    let rolled = face("Rolled");
    let band = (gy(2)..=gy(9))
        .map(|y| down(&rolled, y))
        .min()
        .expect("a band");
    let brk = (gy(10)..=gy(13))
        .map(|y| down(&rolled, y))
        .min()
        .expect("a break");
    let shoulder = down(&rolled, gy(14));
    assert!(
        band > shoulder + 10,
        "the class C top edge is {band} against a {shoulder} shoulder: the roll carries no light"
    );
    assert!(
        brk < shoulder - 10,
        "the break under the roll is {brk} against a {shoulder} shoulder: the roll does not turn, \
         it just fades"
    );

    let flat = face("Flat");
    let shoulder = down(&flat, gy(14));
    for y in 0..=gy(13) {
        assert_eq!(
            down(&flat, y),
            shoulder,
            "the class A/B top edge moved at row {y}: the roll is not class C only"
        );
    }
}

#[test]
fn a_game_boy_cart_face_is_drawn_at_the_game_boy_size() {
    let d = tmp_root();
    write_gb_rom(&d, "GB", "Tetris.gb", 0x00);
    let face = cart_face(&scan(d.path()).unwrap()[0]);
    assert_eq!((face.w, face.h), (GB_CART_W, GB_CART_H));
    assert!(face.rgba.iter().any(|b| *b != 0), "face is blank");
}

#[test]
fn a_nes_cart_is_drawn_at_its_own_size_in_grey_with_a_famicom_one_coloured() {
    let d = tmp_root();
    std::fs::create_dir(d.path().join("Games/NES")).unwrap();
    let mut rom = b"NES\x1a".to_vec();
    rom.resize(0x4010, 0);
    std::fs::write(d.path().join("Games/NES/Zelda (USA).nes"), &rom).unwrap();
    std::fs::write(d.path().join("Games/NES/Zelda (Japan).nes"), &rom).unwrap();
    let carts = scan(d.path()).unwrap();
    let face = |stem: &str| cart_face(carts.iter().find(|c| c.stem == stem).unwrap());
    let (usa, japan) = (face("Zelda (USA)"), face("Zelda (Japan)"));
    assert_eq!((usa.w, usa.h), (NES_CART_W, NES_CART_H));
    // Below the grip, beside the label: bare plastic.
    let px = |f: &slot_ui::CartFace| {
        let i = ((f.h * 6 / 10 * f.w + f.w / 25) * 4) as usize;
        [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2], f.rgba[i + 3]]
    };
    let (grey, coloured) = (px(&usa), px(&japan));
    assert_eq!(grey[3], 255, "the side of the cart is see-through");
    assert!(grey[0].abs_diff(grey[2]) < 16, "an NES cart is not grey: {grey:?}");
    assert_ne!(grey, coloured, "the Famicom release wears NES grey");
    let corner = (0..4).map(|c| usa.rgba[c]).collect::<Vec<_>>();
    assert_eq!(corner[3], 0, "the rounded corner is filled");
}

#[test]
fn every_cartridge_is_centred_on_the_row_and_clears_both_the_plate_and_the_slot() {
    let lip = OUT_H as f32 - MOUTH_H;
    for (name, h, span) in [
        ("the GBA cart", CART_H, OUT_H as f32),
        ("the Game Boy pak", GB_CART_H, lip),
        ("the NES cart", NES_CART_H, lip),
    ] {
        let (top, foot) = (rest_y(h as f32), foot_y(h as f32));
        assert_eq!(
            top + foot,
            span,
            "{name} stands at {top}..{foot}, which is not centred in {span}px"
        );
        assert!(
            top > PLATE_H,
            "{name}'s top edge at {top} is under the {PLATE_H}px HUD plate"
        );
        assert!(
            foot < lip,
            "{name}'s foot at {foot} has reached the lip at {lip}: it is standing in the slot"
        );
    }
}

#[test]
fn each_of_the_three_cgb_flags_gets_its_own_plastic() {
    let d = tmp_root();
    write_gb_rom(&d, "GB", "Tetris.gb", 0x00);
    write_gb_rom(&d, "GBC", "Colour Enhanced.gbc", 0x80);
    write_gb_rom(&d, "GBC", "Colour Only.gbc", 0xc0);
    let carts = scan(d.path()).unwrap();
    let by_stem = |stem: &str| {
        carts
            .iter()
            .find(|c| c.stem == stem)
            .unwrap_or_else(|| panic!("{stem} was not scanned"))
    };

    let grey = shell_for(by_stem("Tetris"));
    let black = shell_for(by_stem("Colour Enhanced"));
    let clear = shell_for(by_stem("Colour Only"));

    assert_eq!(grey.finish, Finish::Solid);
    assert_eq!(
        black.finish,
        Finish::Solid,
        "the 0x80 pak is black plastic, not clear"
    );
    assert_eq!(clear.finish, Finish::Translucent);

    let luma = |s: slot_ui::Shell| s.colour.iter().map(|c| *c as u32).sum::<u32>();
    assert!(
        luma(black) + 120 < luma(grey),
        "the black pak at {} is not darker than the grey one at {}",
        luma(black),
        luma(grey)
    );
    for (a, b) in [(grey, black), (black, clear), (grey, clear)] {
        assert_ne!(
            a.colour, b.colour,
            "two of the three plastics are one colour"
        );
    }
}

#[test]
fn the_notch_follows_the_cgb_flag_and_not_the_folder() {
    let d = tmp_root();
    write_gb_rom(&d, "GB", "Filed As Mono.gb", 0xc0);
    write_gb_rom(&d, "GBC", "Filed As Colour.gbc", 0x80);
    let carts = scan(d.path()).unwrap();
    let notched_away = |stem: &str| {
        let cart = carts.iter().find(|c| c.stem == stem).expect("scanned");
        pixel_alpha(&cart_face(cart), GB_CART_W - 20, 8) < 8
    };
    assert!(
        !notched_away("Filed As Mono"),
        "a 0xc0 rom in the GB folder was drawn with a notch it never had"
    );
    assert!(
        notched_away("Filed As Colour"),
        "a 0x80 rom in the GBC folder lost the notch its shell was moulded with"
    );
}

#[test]
fn only_the_notched_shell_has_lines_across_its_shoulder() {
    let d = tmp_root();
    write_gb_rom(&d, "GB", "Grey.gb", 0x00);
    write_gb_rom(&d, "GB", "Black.gb", 0x80);
    write_gb_rom(&d, "GBC", "Clear.gbc", 0xc0);
    let carts = scan(d.path()).unwrap();
    let steps = |stem: &str| {
        let cart = carts.iter().find(|c| c.stem == stem).expect("scanned");
        let face = cart_face(cart);
        let lum = |x: u32, y: u32| {
            let p = pixel(&face, x, y);
            p.iter().map(|c| u32::from(*c)).sum::<u32>() / 3
        };
        (gx(16)..gx(32))
            .flat_map(|x| (gy(21)..gy(58)).map(move |y| (x, y)))
            .filter(|(x, y)| lum(*x, *y).abs_diff(lum(*x, y - 1)) > 10)
            .count()
    };
    for stem in ["Grey", "Black"] {
        assert!(
            steps(stem) > 200,
            "the {stem} pak's shoulder came up smooth: {} stepped pixels, and five ribs a \
             side should leave hundreds",
            steps(stem)
        );
    }
    assert!(
        steps("Clear") < 10,
        "the Colour pak has lines across its header, which that shell does not have: {}",
        steps("Clear")
    );
}

#[test]
fn the_clear_shell_lightens_at_its_rim_and_the_plain_one_does_not() {
    let d = tmp_root();
    write_gb_rom(&d, "GB", "Tetris.gb", 0x00);
    write_gb_rom(&d, "GBC", "Colour Only.gbc", 0xc0);
    let carts = scan(d.path()).unwrap();
    let luma = |face: &slot_ui::CartFace, x: u32, y: u32| {
        let p = pixel(face, x, y);
        p[0] as u32 + p[1] as u32 + p[2] as u32
    };
    let y = GB_CART_H / 2;
    for cart in &carts {
        let face = cart_face(cart);
        let (rim, body) = (luma(&face, gx(8), y), luma(&face, gx(20), y));
        match cart.stem.as_str() {
            "Tetris" => assert_eq!(rim, body, "the plain pak has a lit rim"),
            _ => assert!(
                rim > body + 30,
                "the clear pak's rim is {rim} against {body} inside: it reads as solid"
            ),
        }
    }
}

#[test]
fn a_game_boy_title_stays_inside_its_near_square_label() {
    for stem in [
        "Supercalifragilisticexpialidocious Anniversary Edition",
        "Wario Land 3",
        "Tetris",
    ] {
        let d = tmp_root();
        write_gb_rom(&d, "GB", &format!("{stem}.gb"), 0x00);
        let face = cart_face(&scan(d.path()).unwrap()[0]);
        let bg = label_colour(stem);
        let at = |x: u32, y: u32| pixel(&face, GB_LABEL_X + x, GB_LABEL_Y + y);
        let margin = 5;
        for y in 0..GB_LABEL_H {
            for x in 0..GB_LABEL_W {
                let edge = x < margin
                    || y < margin
                    || x >= GB_LABEL_W - margin
                    || y >= GB_LABEL_H - margin;
                if edge {
                    assert_eq!(
                        at(x, y),
                        bg,
                        "{stem}: text spills into the border at {x},{y}"
                    );
                }
            }
        }
        assert!(
            (0..GB_LABEL_H * GB_LABEL_W).any(|i| at(i % GB_LABEL_W, i / GB_LABEL_W) != bg),
            "{stem}: no text was drawn"
        );
    }
}

#[test]
fn the_cart_shadow_is_the_cart_in_black() {
    let s = slot_ui::cart_shadow();
    assert_eq!((s.w, s.h), (CART_W, CART_H));
    let mask = silhouette(CART_W, CART_H);
    for (px, cover) in s.rgba.chunks_exact(4).zip(&mask) {
        assert_eq!(&px[..3], &[0, 0, 0], "the shadow is not black");
        assert_eq!(px[3], *cover, "the shadow is not the cart's shape");
    }
    assert!(
        s.rgba.chunks_exact(4).any(|p| p[3] > 250),
        "the shadow is transparent everywhere, so it backs nothing"
    );
}

#[test]
fn each_game_boy_shell_gets_a_black_shadow_of_its_own_exact_outline() {
    for shell in [GbShell::Notched, GbShell::Rounded] {
        let s = slot_ui::gb_cart_shadow(shell);
        assert_eq!((s.w, s.h), (GB_CART_W, GB_CART_H));
        let own = gb_silhouette(shell, GB_CART_W, GB_CART_H);
        for (i, px) in s.rgba.chunks_exact(4).enumerate() {
            assert_eq!(&px[..3], &[0, 0, 0], "{shell:?}: the shadow is not black");
            assert_eq!(
                px[3], own[i],
                "{shell:?}: the shadow is not the shell's own shape at pixel {i}"
            );
        }
    }
    let notched = gb_silhouette(GbShell::Notched, GB_CART_W, GB_CART_H);
    let rounded = gb_silhouette(GbShell::Rounded, GB_CART_W, GB_CART_H);
    let bare: u32 = notched
        .iter()
        .zip(&rounded)
        .map(|(n, r)| u32::from(n.abs_diff(*r)))
        .sum();
    assert!(
        bare / 255 > 100,
        "the two shells now differ by {} px, so the shared backing was harmless after all",
        bare / 255
    );
}

#[test]
fn a_clear_pak_shows_its_board_and_a_solid_one_does_not() {
    let d = tmp_root();
    write_gb_rom(&d, "GBC", "Clear.gbc", 0xc0);
    write_gb_rom(&d, "GB", "Grey.gb", 0x00);
    let carts = scan(d.path()).unwrap();
    let face = |stem: &str| cart_face(carts.iter().find(|c| c.stem == stem).expect("scanned"));
    let (clear, grey) = (face("Clear"), face("Grey"));
    let (board, contact) = ((gx(24), GB_CART_H / 2), (gx(26), GB_CART_H - gy(8)));

    let [r, g, b] = pixel(&clear, board.0, board.1);
    assert!(
        g > r + 8 && g > b,
        "no board beside the label: {:?}",
        [r, g, b]
    );
    let [r, _, b] = pixel(&clear, contact.0, contact.1);
    assert!(r > b + 20, "no gold contact at the bottom: {:?}", [r, b]);

    for (x, y) in [board, contact] {
        let [r, g, b] = pixel(&grey, x, y);
        assert!(
            r.abs_diff(g) < 12 && g.abs_diff(b) < 12,
            "the grey pak shows something through it at {x},{y}: {:?}",
            [r, g, b]
        );
    }
}

#[test]
fn every_mould_is_lettered_with_its_platform() {
    let d = tmp_root();
    write_rom(&d, "Advance.gba", "ADVANCE");
    write_gb_rom(&d, "GB", "Grey.gb", 0x00);
    write_gb_rom(&d, "GBC", "Clear.gbc", 0xc0);
    let carts = scan(d.path()).unwrap();
    for (stem, rows, plain) in [
        ("Advance", 19..28, (120, 3)),
        ("Grey", 30..48, (197, 40)),
        ("Clear", 26..44, (197, 40)),
    ] {
        let face = cart_face(carts.iter().find(|c| c.stem == stem).expect("scanned"));
        let lum = |(x, y): (u32, u32)| {
            pixel(&face, x, y)
                .iter()
                .map(|c| u32::from(*c))
                .sum::<u32>()
        };
        let base = lum(plain);
        let band: Vec<u32> = rows
            .flat_map(|y| (60..180).map(move |x| (x, y)))
            .map(lum)
            .collect();
        assert!(
            band.iter().any(|l| *l > base + 30) && band.iter().any(|l| *l + 30 < base),
            "the {stem} face has no raised lettering where its platform name goes"
        );
    }
}

fn clear_gba(d: &TempDir) -> slot_ui::CartFace {
    let mut rom = vec![0u8; 0x100];
    rom[0xac..0xb0].copy_from_slice(b"U3IE");
    std::fs::write(d.path().join("Games/GBA/Boktai.gba"), rom).expect("write rom");
    let carts = scan(d.path()).unwrap();
    let mut cart = carts.into_iter().find(|c| c.stem == "Boktai").expect("scanned");
    // The scan reads no headers; the shelf reads it before drawing.
    cart.read_header();
    assert_eq!(shell_for(&cart).finish, Finish::Translucent);
    cart_face(&cart)
}

fn bevel() -> u32 {
    (3 * CART_W + seated_box(Platform::Gba).0 / 2) / seated_box(Platform::Gba).0
}

#[test]
fn a_clear_gba_cart_keeps_its_board_behind_the_label() {
    let d = tmp_root();
    let face = clear_gba(&d);
    let green = |[r, g, b]: [u8; 3]| g as i32 - (r as i32 + b as i32) / 2;
    let y = LABEL_Y + LABEL_H / 2;
    let beside = pixel(&face, LABEL_X - bevel() - 2, y);
    let above = pixel(&face, LABEL_X - bevel() - 2, CART_H / 20);
    assert!(
        green(beside) - green(above) < 8,
        "board shows beside the label: {beside:?} against plastic {above:?}"
    );
}

#[test]
fn a_clear_cart_lights_its_label_recess_no_brighter_than_its_plastic() {
    let d = tmp_root();
    let face = clear_gba(&d);
    let luma = |[r, g, b]: [u8; 3]| (r as u32 * 54 + g as u32 * 183 + b as u32 * 19) / 256;
    let y = LABEL_Y + LABEL_H / 2;
    let lit = pixel(&face, LABEL_X + LABEL_W + 1, y);
    let plastic = pixel(&face, LABEL_X + LABEL_W + bevel() + 2, y);
    assert!(
        luma(lit) <= luma(plastic) * 13 / 10 + 4,
        "the recess edge {lit:?} glares against the plastic {plastic:?}"
    );
}

#[test]
fn a_pal_snes_cart_is_narrower_and_the_menu_can_swap_the_shape() {
    use slot_store::{Outline, ShellChoice, ShellFinish};
    use slot_ui::{snes_shell_of, SnesShell};
    let snes = |stem: &str, outline: Option<Outline>| slot_store::Cart {
        platform: Platform::Snes,
        stem: stem.into(),
        rom: format!("Games/SNES/{stem}.sfc").into(),
        label: None,
        code: String::new(),
        shell: outline.map(|outline| ShellChoice { outline, colour: [0x80; 3], finish: ShellFinish::Solid }),
        title: String::new(),
        famicom_label: None,
    };
    let side = |c: &slot_store::Cart| {
        let f = cart_face(c);
        f.rgba[(((f.h / 2) * f.w + 4) * 4 + 3) as usize]
    };
    let us = snes("Mario (USA)", None);
    let pal = snes("Mario (Europe)", None);
    assert_eq!(snes_shell_of(&us), Some(SnesShell::Us));
    assert_eq!(snes_shell_of(&pal), Some(SnesShell::Pal));
    assert!(side(&us) > 250, "the US cart's wing is missing");
    assert_eq!(side(&pal), 0, "the PAL cart fills the US cart's width");

    let forced = snes("Mario (USA)", Some(Outline::Rounded));
    assert_eq!(snes_shell_of(&forced), Some(SnesShell::Pal));
    let forced = snes("Mario (Europe)", Some(Outline::Notched));
    assert_eq!(snes_shell_of(&forced), Some(SnesShell::Us));
    assert_eq!(snes_shell_of(&snes("Mario (Europe)", Some(Outline::Auto))), Some(SnesShell::Pal));
}

#[test]
fn a_ripped_rom_with_no_region_in_its_name_goes_by_its_header() {
    use slot_ui::{nes_famicom_of, shell_for, snes_shell_of, SnesShell, NES_SHELL, SNES_PAL_SHELL};
    let d = tmp_root();
    // Terranigma's header: HiROM, sold in Europe, the checksum checking out.
    let mut rom = vec![0u8; 0x10000];
    rom[0xffc0 + 0x15] = 0x21;
    rom[0xffc0 + 0x19] = 0x02;
    rom[0xffc0 + 0x1c..0xffc0 + 0x20].copy_from_slice(&[0x34, 0x12, 0xcb, 0xed]);
    let sfc = d.path().join("Terranigma.sfc");
    std::fs::write(&sfc, rom).unwrap();
    let cart = |platform, stem: &str, rom: std::path::PathBuf| slot_store::Cart {
        platform, stem: stem.into(), rom, label: None, code: String::new(), shell: None, title: String::new(),
        famicom_label: None,
    };
    let terranigma = cart(Platform::Snes, "Terranigma", sfc);
    assert_eq!(snes_shell_of(&terranigma), Some(SnesShell::Pal));
    assert_eq!(shell_for(&terranigma), SNES_PAL_SHELL);

    // Akumajou Densetsu: VRC6, a board only Famicom carts had.
    let mut nes = vec![b'N', b'E', b'S', 0x1a, 16, 16, 0x80, 0x10, 0, 0, 0, 0, 0, 0, 0, 0];
    nes.resize(16 + 0x4000, 0);
    let path = d.path().join("Akumajou Densetsu.nes");
    std::fs::write(&path, nes).unwrap();
    let akumajou = cart(Platform::Nes, "Akumajou Densetsu", path);
    assert!(nes_famicom_of(&akumajou));
    assert_ne!(shell_for(&akumajou), NES_SHELL);
}

#[test]
fn a_colour_saved_for_a_nes_or_snes_cart_is_ignored() {
    use slot_store::{Outline, ShellChoice, ShellFinish};
    use slot_ui::{shell_for, NES_SHELL, SNES_SHELL};
    let pink = Some(ShellChoice { outline: Outline::Auto, colour: [0xe2, 0x8c, 0xa9], finish: ShellFinish::Glitter });
    for (platform, grey) in [(Platform::Nes, NES_SHELL), (Platform::Snes, SNES_SHELL)] {
        let cart = slot_store::Cart {
            platform, stem: "Mario (USA)".into(), rom: "missing".into(), label: None,
            code: String::new(), shell: pink, title: String::new(),
            famicom_label: None,
        };
        assert_eq!(shell_for(&cart), grey, "{platform:?} took the chosen colour");
    }
}

#[test]
fn a_famicom_cart_is_small_and_landscape_and_wears_its_chosen_colour() {
    use slot_store::{Outline, ShellChoice, ShellFinish};
    use slot_ui::{nes_famicom_of, shell_for, NES_SHELL};
    let nes = |stem: &str, shell: Option<ShellChoice>| slot_store::Cart {
        platform: Platform::Nes, stem: stem.into(), rom: "missing.nes".into(), label: None,
        code: String::new(), shell, title: String::new(),
        famicom_label: None,
    };
    let opaque = |c: &slot_store::Cart, fx: f32, fy: f32| {
        let f = cart_face(c);
        let (x, y) = ((f.w as f32 * fx) as u32, (f.h as f32 * fy) as u32);
        f.rgba[((y * f.w + x) * 4 + 3) as usize] > 250
    };
    let us = nes("Mario (USA)", None);
    let jp = nes("Mario (Japan)", None);
    assert!(!nes_famicom_of(&us) && nes_famicom_of(&jp));
    assert!(opaque(&us, 0.5, 0.2) && opaque(&us, 0.01, 0.5), "the NES cart lost its body");
    assert!(!opaque(&jp, 0.5, 0.2), "the Famicom cart is as tall as a NES cart");
    assert!(!opaque(&jp, 0.01, 0.7), "the Famicom cart is as wide as a NES cart");
    assert!(opaque(&jp, 0.5, 0.7), "the Famicom cart is missing");

    let pink = |outline| Some(ShellChoice { outline, colour: [0xe2, 0x8c, 0xa9], finish: ShellFinish::Glitter });
    let fc = shell_for(&nes("Mario (USA)", pink(Outline::Rounded)));
    assert_eq!(fc.colour, [0xe2, 0x8c, 0xa9]);
    assert_eq!(fc.finish, Finish::Solid, "a Famicom cart took a finish");
    assert_eq!(shell_for(&nes("Mario (Japan)", pink(Outline::Notched))), NES_SHELL);
}

#[test]
fn a_tall_label_on_a_wide_cart_is_shown_whole_and_a_wide_one_still_fills() {
    let d = tmp_root();
    // A NES label: tall, its title in blue across the top.
    write_label(&d, "tall.png", 100, 200, |_, y| if y < 20 { [0, 0, 255] } else { [255, 0, 0] });
    let tall = d.path().join("Labels/GBA/tall.png");
    let blue = |rgba: &[u8]| rgba.chunks_exact(4).filter(|p| p[2] > 200 && p[0] < 50).count();
    assert_eq!(blue(&slot_ui::cover(&tall, 300, 150).unwrap()), 0, "cover kept the top: the test proves nothing");
    let fitted = slot_ui::label_fit(&tall, 300, 150).unwrap();
    assert!(blue(&fitted) > 300, "the title was cut off a tall label on a wide cart");
    write_label(&d, "wide.png", 200, 100, |x, _| [x as u8, 0, 0]);
    let wide = d.path().join("Labels/GBA/wide.png");
    assert_eq!(slot_ui::label_fit(&wide, 300, 140), slot_ui::cover(&wide, 300, 140), "a wide label was not filled");
}

#[test]
fn a_nes_game_wears_the_label_of_the_cart_it_is_on() {
    use slot_store::{Outline, ShellChoice, ShellFinish};
    use slot_ui::shown_label;
    let on = |outline: Option<Outline>, label: Option<&str>, famicom: Option<&str>| slot_store::Cart {
        platform: Platform::Nes, stem: "Mega Man (USA)".into(), rom: "missing.nes".into(),
        label: label.map(Into::into), famicom_label: famicom.map(Into::into), code: String::new(),
        shell: outline.map(|outline| ShellChoice { outline, colour: [0x80; 3], finish: ShellFinish::Solid }),
        title: String::new(),
    };
    let both = (Some("nes.png"), Some("fc.png"));
    assert_eq!(shown_label(&on(None, both.0, both.1)), Some(std::path::Path::new("nes.png")));
    assert_eq!(shown_label(&on(Some(Outline::Rounded), both.0, both.1)), Some(std::path::Path::new("fc.png")));
    assert_eq!(shown_label(&on(Some(Outline::Notched), both.0, both.1)), Some(std::path::Path::new("nes.png")));
    // With one label, both carts wear it.
    assert_eq!(shown_label(&on(Some(Outline::Rounded), Some("nes.png"), None)), Some(std::path::Path::new("nes.png")));
    assert_eq!(shown_label(&on(None, None, Some("fc.png"))), Some(std::path::Path::new("fc.png")));
}
