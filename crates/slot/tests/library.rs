mod common;

use common::{app_booting_at, tmp_root_with_carts, write_gb_cart};
use slot::app::{App, Phase};
use slot_input::{Action, Btn};
use slot_store::{read_slot_state, write_slot_state, Library, LibrarySort, Platform, SlotState, LIBRARY_FILE};
use slot_ui::QuickRow;

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
    assert_eq!(a.played_text().as_deref(), Some("150M"));
    a.apply(Action::GbaDown(Btn::Right));
    a.apply(Action::GbaUp(Btn::Right));
    assert_eq!(a.selected_stem(), Some("Gamma"));
    assert_eq!(a.played_text().as_deref(), Some("66M"));
    a.apply(Action::GbaDown(Btn::Right));
    a.apply(Action::GbaUp(Btn::Right));
    assert_eq!(a.played_text(), None, "Alpha was never played");
    let (_d2, b) = library_app(PLAYED, SlotState { library_sort: LibrarySort::MostPlayed, play_time_hours: true, ..SlotState::default() });
    assert_eq!(b.played_text().as_deref(), Some("2H30M"));
}

#[test]
fn the_title_card_names_the_selected_game_unless_turned_off() {
    let (_d, mut a) = library_app("", SlotState::default());
    assert_eq!(a.title_card_text().as_deref(), Some("Alpha"));
    a.apply(Action::GbaDown(Btn::Right));
    a.apply(Action::GbaUp(Btn::Right));
    assert_eq!(a.title_card_text().as_deref(), Some("Beta"));
    let (_d2, b) = library_app("", SlotState { title_card: false, ..SlotState::default() });
    assert_eq!(b.title_card_text(), None);
}

fn press(a: &mut App, btn: Btn) {
    a.apply(Action::GbaDown(btn));
    a.apply(Action::GbaUp(btn));
}

/// HOME → Library → Systems, with the bar on `row`.
fn open_systems_at(a: &mut App, row: QuickRow) {
    while a.quick_menu().is_some() {
        press(a, Btn::B);
    }
    a.apply(Action::QuickMenu);
    for _ in 0..QuickRow::Library.position() {
        press(a, Btn::Down);
    }
    press(a, Btn::A);
    for _ in 0..QuickRow::LibrarySystems.position() {
        press(a, Btn::Down);
    }
    press(a, Btn::A);
    for _ in 0..row.position() {
        press(a, Btn::Down);
    }
    assert_eq!(a.quick_menu(), Some(row));
}

fn two_systems(state: SlotState) -> (tempfile::TempDir, App) {
    let d = tmp_root_with_carts(&["Alpha", "Beta"]);
    write_gb_cart(&d, "Pocket", "POCKET");
    write_slot_state(d.path(), &SlotState { clock_set: true, ..state }).unwrap();
    let (a, _) = app_booting_at(d.path(), 1_800_000_000);
    (d, a)
}

#[test]
fn a_hidden_system_has_no_shelf_and_stays_hidden() {
    let (d, mut a) = two_systems(SlotState::default());
    assert_eq!(order(&a), "Alpha Beta Pocket");
    open_systems_at(&mut a, QuickRow::ShowGba);
    assert_eq!(a.quick_value(QuickRow::ShowGba), Some(slot_ui::QuickValue::On));
    press(&mut a, Btn::Right);
    assert_eq!(a.quick_value(QuickRow::ShowGba), Some(slot_ui::QuickValue::Off));
    assert_eq!(order(&a), "Pocket");
    assert!(!read_slot_state(d.path()).shows(Platform::Gba));
    // Back on the shelf, the Game Boy's is the one there is.
    for _ in 0..3 {
        press(&mut a, Btn::B);
    }
    assert!(matches!(a.phase(), Phase::Shelf));
    assert_eq!(a.selected_stem(), Some("Pocket"));
    let (_d2, b) = two_systems(read_slot_state(d.path()));
    assert_eq!(order(&b), "Pocket");
}

#[test]
fn the_last_system_with_games_cannot_be_hidden() {
    let mut state = SlotState::default();
    state.hidden_systems[Platform::Gba.index()] = true;
    let (d, mut a) = two_systems(state);
    open_systems_at(&mut a, QuickRow::ShowGb);
    press(&mut a, Btn::Right);
    assert_eq!(a.quick_value(QuickRow::ShowGb), Some(slot_ui::QuickValue::On));
    assert_eq!(order(&a), "Pocket");
    assert!(read_slot_state(d.path()).shows(Platform::Gb));
    // A system with no games can be hidden all the same.
    open_systems_at(&mut a, QuickRow::ShowNes);
    press(&mut a, Btn::Right);
    assert!(!read_slot_state(d.path()).shows(Platform::Nes));
}

#[test]
fn with_every_system_that_has_games_hidden_all_are_shown() {
    let mut state = SlotState::default();
    state.hidden_systems[Platform::Gba.index()] = true;
    state.hidden_systems[Platform::Gb.index()] = true;
    let (d, mut a) = two_systems(state);
    assert_eq!(order(&a), "Alpha Beta Pocket");
    // Showing one again is never refused.
    open_systems_at(&mut a, QuickRow::ShowGb);
    press(&mut a, Btn::Right);
    assert!(read_slot_state(d.path()).shows(Platform::Gb));
    assert_eq!(order(&a), "Pocket");
}

#[test]
fn favorites_only_counts_the_favorites_on_shown_systems() {
    let mut state = SlotState { favorites_only: true, ..SlotState::default() };
    state.hidden_systems[Platform::Gb.index()] = true;
    let d = tmp_root_with_carts(&["Alpha", "Beta"]);
    write_gb_cart(&d, "Pocket", "POCKET");
    write_slot_state(d.path(), &SlotState { clock_set: true, ..state }).unwrap();
    std::fs::write(d.path().join(LIBRARY_FILE), "GB\tPocket\t0\t0\t1\n").unwrap();
    let (a, _) = app_booting_at(d.path(), 1_800_000_000);
    assert_eq!(order(&a), "Alpha Beta", "the only favorite is on a hidden shelf");
}
