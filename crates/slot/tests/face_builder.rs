use std::time::{Duration, Instant};

use slot::face_builder::{BuiltFaces, FaceBuilder};
use slot_store::{Cart, Platform};
use slot_ui::{CART_H, CART_W, TURN_PAD};

fn cart(stem: &str) -> Cart {
    Cart {
        platform: Platform::Gba,
        stem: stem.into(),
        rom: format!("Games/GBA/{stem}.gba").into(),
        label: None,
        title: stem.to_uppercase(),
        code: String::new(),
        shell: None,
        famicom_label: None,
    }
}

fn collect(builder: &FaceBuilder, want: usize) -> Vec<BuiltFaces> {
    let mut got = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    while got.len() < want && Instant::now() < deadline {
        if let Some(faces) = builder.take() {
            got.push(faces);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    got
}

#[test]
fn a_request_comes_back_as_the_open_carts_faces() {
    let builder = FaceBuilder::spawn();
    builder.request(cart("Metroid Fusion"));
    let got = collect(&builder, 1);
    assert_eq!(got.len(), 1, "the worker never answered");
    assert_eq!(got[0].stem, "Metroid Fusion");
    assert_eq!((got[0].board.w, got[0].board.h), (372, 209));
    assert_eq!(
        (got[0].lid.w, got[0].lid.h),
        (CART_W + 2 * TURN_PAD, CART_H + 2 * TURN_PAD)
    );
}

#[test]
fn the_newest_request_of_a_burst_is_the_last_built() {
    let builder = FaceBuilder::spawn();
    for stem in ["Advance Wars", "Drill Dozer", "Metroid Fusion"] {
        builder.request(cart(stem));
    }
    let mut seen = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Some(faces) = builder.take() {
            let done = faces.stem == "Metroid Fusion";
            seen.push(faces.stem);
            if done {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(seen.last().map(String::as_str), Some("Metroid Fusion"));
    assert!(
        seen.len() <= 2,
        "the burst did not collapse to its newest: {seen:?} came back"
    );
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        builder.take().is_none(),
        "a build older than the newest came back after it"
    );
}

#[test]
fn the_open_carts_lid_wears_its_games_plastic_and_any_chosen_one() {
    let d = tempfile::tempdir().unwrap();
    let rom = d.path().join("Drill Dozer.gba");
    let mut bytes = vec![0u8; 0x200];
    bytes[0xa0..0xab].copy_from_slice(b"DRILL DOZER");
    bytes[0xac..0xb0].copy_from_slice(b"V49E");
    std::fs::write(&rom, bytes).unwrap();
    let mut dozer = Cart { rom, code: String::new(), ..cart("Drill Dozer") };
    let builder = FaceBuilder::spawn();
    let lid_of = |c: &Cart| {
        builder.request(c.clone());
        collect(&builder, 1).remove(0).lid
    };
    let own = lid_of(&dozer);
    let mut header_read = dozer.clone();
    header_read.read_header();
    let want = slot_ui::padded(&slot_ui::cart_face(&header_read), TURN_PAD);
    assert!(own.rgba == want.rgba, "the lid ignored the game code: Drill Dozer is red, not the default");
    dozer.shell = Some(slot_store::ShellChoice {
        outline: slot_store::Outline::Auto,
        colour: [0x20, 0x40, 0xc0],
        finish: slot_store::ShellFinish::Glitter,
    });
    assert!(lid_of(&dozer).rgba != own.rgba, "the lid ignored the chosen plastic");
}
