mod common;

use common::{app_booting_at, tmp_root_with_carts};
use slot::app::{App, Phase};
use slot_input::{Action, Btn};
use slot_store::{write_slot_state, Library, LibrarySort, SlotState, LIBRARY_FILE};

fn order(a: &App) -> String {
    a.carts().map(|c| c.stem.as_str()).collect::<Vec<_>>().join(" ")
}

fn library_app(library: &str, state: SlotState) -> (tempfile::TempDir, App) {
    let d = tmp_root_with_carts(&["Alpha", "Beta", "Gamma"]);
    write_slot_state(d.path(), &SlotState { clock_set: true, ..state }).unwrap();
    std::fs::write(d.path().join(LIBRARY_FILE), library).unwrap();
    let (a, _) = app_booting_at(d.path(), 1_800_000_000);
    assert!(matches!(a.phase(), Phase::Shelf), "{:?}", a.phase());
    (d, a)
}

const PLAYED: &str = "GBA\tGamma\t4000\t20\t1\nGBA\tBeta\t9000\t10\t0\n";

#[test]
fn select_steps_the_shelf_through_its_orders_and_names_each() {
    let (d, mut a) = library_app(PLAYED, SlotState::default());
    assert_eq!(order(&a), "Alpha Beta Gamma");
    a.apply(Action::GbaDown(Btn::Select));
    assert_eq!(order(&a), "Gamma Beta Alpha");
    assert_eq!(a.slot_text().as_deref(), Some("LAST PLAYED"));
    a.apply(Action::GbaDown(Btn::Select));
    assert_eq!(order(&a), "Beta Gamma Alpha");
    assert_eq!(a.slot_text().as_deref(), Some("MOST PLAYED"));
    a.apply(Action::GbaDown(Btn::Select));
    assert_eq!(order(&a), "Alpha Beta Gamma");
    a.settle_saves();
    assert_eq!(slot_store::read_slot_state(d.path()).library_sort, LibrarySort::Az);
}

#[test]
fn the_order_chosen_is_kept_and_the_selected_game_stays_selected() {
    let (_d, mut a) = library_app(PLAYED, SlotState { library_sort: LibrarySort::MostPlayed, ..SlotState::default() });
    assert_eq!(order(&a), "Beta Gamma Alpha");
    a.apply(Action::GbaDown(Btn::Right));
    a.apply(Action::GbaUp(Btn::Right));
    let picked = a.selected_stem().map(str::to_string);
    a.apply(Action::GbaDown(Btn::Select));
    assert_eq!(a.selected_stem().map(str::to_string), picked);
}

#[test]
fn y_stars_a_favorite_and_favorites_only_shows_just_those() {
    let (d, mut a) = library_app("", SlotState::default());
    assert_eq!(a.selected_stem(), Some("Alpha"));
    a.apply(Action::GbaDown(Btn::Y));
    a.settle_saves();
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(Library::read(d.path()).get(slot_store::Platform::Gba, "Alpha").favorite);
    let (_d2, b) = library_app("GBA\tGamma\t0\t0\t1\n", SlotState { favorites_only: true, ..SlotState::default() });
    assert_eq!(order(&b), "Gamma");
    // With no favorites at all, Favorites still shows every game rather than none.
    let (_d3, c) = library_app("", SlotState { favorites_only: true, ..SlotState::default() });
    assert_eq!(order(&c), "Alpha Beta Gamma");
}

#[test]
fn the_shelf_prints_how_long_the_selected_game_was_played() {
    let (_d, mut a) = library_app(PLAYED, SlotState { library_sort: LibrarySort::MostPlayed, ..SlotState::default() });
    assert_eq!(a.played_text().as_deref(), Some("150 MIN"));
    a.apply(Action::GbaDown(Btn::Right));
    a.apply(Action::GbaUp(Btn::Right));
    assert_eq!(a.selected_stem(), Some("Gamma"));
    assert_eq!(a.played_text().as_deref(), Some("66 MIN"));
    a.apply(Action::GbaDown(Btn::Right));
    a.apply(Action::GbaUp(Btn::Right));
    assert_eq!(a.played_text(), None, "Alpha was never played");
    let (_d2, b) = library_app(PLAYED, SlotState { library_sort: LibrarySort::MostPlayed, play_time_hours: true, ..SlotState::default() });
    assert_eq!(b.played_text().as_deref(), Some("2 H 30 MIN"));
}
