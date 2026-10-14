use slot::app::App;
use slot_store::{Cart, Platform};
use slot_ui::TexId;

fn cart(platform: Platform, stem: &str) -> Cart {
    Cart {
        platform,
        stem: stem.into(),
        rom: format!("/x/{stem}.gba").into(),
        label: None,
        title: String::new(),
        code: String::new(),
        shell: None,
        famicom_label: None,
    }
}

fn library(gba: usize, gb: usize) -> App {
    let mut carts: Vec<Cart> = (0..gba)
        .map(|i| cart(Platform::Gba, &format!("G{i:04}")))
        .collect();
    carts.extend((0..gb).map(|i| cart(Platform::Gb, &format!("B{i:04}"))));
    App::new(carts)
}

#[test]
fn a_big_library_asks_only_for_the_carts_near_the_cursor() {
    let app = library(500, 0);
    let wants = app.face_wants();
    assert_eq!(
        wants.len(),
        17,
        "eight either side of the selected cart, and it"
    );
    let screen: Vec<usize> = wants
        .iter()
        .filter(|w| w.on_screen)
        .map(|w| w.index)
        .collect();
    assert_eq!(screen, vec![0, 1, 499, 2, 498, 3, 497]);
    assert!(
        wants[..7].iter().all(|w| w.on_screen),
        "an off-screen cart was asked for before an on-screen one"
    );
}

#[test]
fn the_neighbouring_shelf_is_asked_for_after_what_is_on_screen() {
    let app = library(40, 30);
    let wants = app.face_wants();
    let gba = wants[0].shelf;
    assert!(wants[..7].iter().all(|w| w.shelf == gba && w.on_screen));
    assert!(
        wants[7..14].iter().all(|w| w.shelf != gba && !w.on_screen),
        "the next shelf's on-screen carts are not queued right after this one's"
    );
}

#[test]
fn a_face_already_held_is_not_asked_for_again() {
    let mut app = library(500, 0);
    app.set_cart_face(0, 0, TexId::from_raw(1));
    assert!(app.face_wants().iter().all(|w| w.index != 0));
}

#[test]
fn faces_far_from_the_cursor_are_shed_and_near_ones_kept() {
    let mut app = library(500, 0);
    app.set_cart_face(0, 0, TexId::from_raw(1));
    app.set_cart_face(0, 12, TexId::from_raw(2));
    app.set_cart_face(0, 13, TexId::from_raw(3));
    app.set_cart_face(0, 250, TexId::from_raw(4));
    let mut shed = app.shed_faces();
    shed.sort_by_key(|t| format!("{t:?}"));
    assert_eq!(shed, vec![TexId::from_raw(3), TexId::from_raw(4)]);
    assert!(app.shed_faces().is_empty());
}

#[test]
fn a_scanned_gba_cart_wears_its_games_plastic_on_the_shelf() {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join("Games/GBA")).unwrap();
    let mut rom = vec![0u8; 0x100];
    rom[0xa0..0xa0 + 11].copy_from_slice(b"DRILL DOZER");
    rom[0xac..0xb0].copy_from_slice(b"V49E");
    std::fs::write(d.path().join("Games/GBA/Drill Dozer.gba"), rom).unwrap();
    let mut cart = slot_store::scan(d.path()).unwrap().remove(0);
    assert!(cart.code.is_empty(), "the scan reads no headers");
    let face = slot::cart_faces::shelf_face(d.path(), &mut cart);
    let mut read = cart.clone();
    read.read_header();
    assert!(face.rgba == slot_ui::cart_face(&read).rgba, "the shelf face ignores the game code");
    let grey = Cart { code: String::new(), ..read };
    assert!(face.rgba != slot_ui::cart_face(&grey).rgba, "Drill Dozer's red is the default grey");
}
