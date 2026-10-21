mod common;

use common::{
    app_booting_at, app_booting_with_clock, app_playing_in, tmp_root_with_carts, Clock,
    CLOCK_IS_SET,
};
use slot::app::{App, Phase};
use slot_input::{Action, Btn};
use slot_store::{read_slot_state, write_slot_state, Platform, Shader, SlotState};
use slot_ui::{
    edge, Draw, Icon, QuickMenuFaces, QuickRow, QuickValue, TexId, MENU_PAD, OUT_W, QUICK_EDGE,
    QUICK_TOP,
};
use tempfile::TempDir;

fn on_carousel_with(state: SlotState) -> (TempDir, App, Clock) {
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    write_slot_state(
        d.path(),
        &SlotState {
            clock_set: true,
            ..state
        },
    )
    .expect("write slot.state");
    let (a, clock) = app_booting_at(d.path(), CLOCK_IS_SET);
    assert!(
        matches!(a.phase(), Phase::Shelf),
        "not on the carousel: {:?}",
        a.phase()
    );
    (d, a, clock)
}

fn on_carousel() -> (TempDir, App, Clock) {
    on_carousel_with(SlotState::default())
}

fn press(a: &mut App, btn: Btn) {
    a.apply(Action::GbaDown(btn));
    a.apply(Action::GbaUp(btn));
}

fn open_at(a: &mut App, row: QuickRow) {
    // From the menu closed, down through each page on the way: Screen, then a system, then the row.
    while a.quick_menu().is_some() {
        press(a, Btn::B);
    }
    a.apply(Action::QuickMenu);
    let mut path = vec![row];
    while let Some(up) = path.last().and_then(|r| r.parent()) {
        path.push(up);
    }
    for (i, step) in path.iter().rev().enumerate() {
        if i > 0 {
            press(a, Btn::A);
        }
        for _ in 0..step.position() {
            press(a, Btn::Down);
        }
    }
    assert_eq!(a.quick_menu(), Some(row), "the bar never reached {row:?}");
}

#[test]
fn menu_opens_the_quick_menu_with_its_top_row_selected_every_time() {
    let (_d, mut a, _) = on_carousel();
    a.apply(Action::QuickMenu);
    assert_eq!(a.quick_menu(), Some(QuickRow::MAIN[0]));
    press(&mut a, Btn::Down);
    a.apply(Action::QuickMenu);
    a.apply(Action::QuickMenu);
    assert_eq!(
        a.quick_menu(),
        Some(QuickRow::MAIN[0]),
        "the menu opened where it was last left"
    );
}

#[test]
fn menu_or_b_closes_it_back_onto_the_carousel_where_you_were() {
    for close in [Action::QuickMenu, Action::GbaDown(Btn::B)] {
        let (_d, mut a, _) = on_carousel();
        press(&mut a, Btn::Right);
        assert_eq!(
            a.selected_stem(),
            Some("Fusion"),
            "the carousel never moved"
        );
        a.apply(Action::QuickMenu);
        a.apply(close);
        assert!(
            matches!(a.phase(), Phase::Shelf),
            "{close:?} left {:?}",
            a.phase()
        );
        assert_eq!(a.selected_stem(), Some("Fusion"), "{close:?} lost the cart");
    }
}

#[test]
fn the_quick_menu_is_only_on_the_carousel() {
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::QuickMenu);
    assert!(
        matches!(a.phase(), Phase::Playing { .. }),
        "MENU raised the quick menu over a game: {:?}",
        a.phase()
    );
    assert_eq!(a.quick_menu(), None);
}

#[test]
fn up_and_down_move_the_bar_and_wrap_at_the_ends() {
    let (_d, mut a, _) = on_carousel();
    a.apply(Action::QuickMenu);
    press(&mut a, Btn::Up);
    assert_eq!(
        a.quick_menu(),
        QuickRow::MAIN.last().copied(),
        "up from the top did not wrap to the bottom"
    );
    for want in QuickRow::MAIN.into_iter().chain([QuickRow::MAIN[0]]) {
        press(&mut a, Btn::Down);
        assert_eq!(a.quick_menu(), Some(want));
    }
}

#[test]
fn fast_forward_steps_through_its_speeds_wraps_at_both_ends_and_saves_each_one() {
    let (d, mut a, _) = on_carousel();
    open_at(&mut a, QuickRow::FastForward);
    for (btn, want) in [
        (Btn::Left, 4),
        (Btn::Left, 3),
        (Btn::Left, 2),
        (Btn::Left, 6),
        (Btn::Right, 2),
        (Btn::Right, 3),
        (Btn::Right, 4),
        (Btn::Right, 6),
        (Btn::Right, 2),
    ] {
        press(&mut a, btn);
        assert_eq!(a.ff_speed(), want, "{btn:?}");
        assert_eq!(read_slot_state(d.path()).ff_speed, want, "not on the card");
        assert_eq!(
            a.quick_value(QuickRow::FastForward),
            QuickValue::speed(want)
        );
    }
}

#[test]
fn rumble_and_fast_forward_sound_flip_on_either_arrow_and_save() {
    let (d, mut a, _) = on_carousel();
    let card = |d: &TempDir| {
        let s = read_slot_state(d.path());
        (s.ff_sound, s.rumble)
    };
    open_at(&mut a, QuickRow::FastForwardSound);
    press(&mut a, Btn::Left);
    assert_eq!(card(&d), (true, true));
    assert_eq!(
        a.quick_value(QuickRow::FastForwardSound),
        Some(QuickValue::On)
    );
    press(&mut a, Btn::Right);
    assert_eq!(card(&d), (false, true));
    press(&mut a, Btn::Down);
    press(&mut a, Btn::Down);
    press(&mut a, Btn::Down);
    press(&mut a, Btn::Right);
    assert_eq!(card(&d), (false, false));
    assert!(!a.rumble_enabled());
    assert_eq!(a.quick_value(QuickRow::Rumble), Some(QuickValue::Off));
    press(&mut a, Btn::Left);
    assert_eq!(card(&d), (false, true));
}

#[test]
fn colour_correction_flips_on_either_arrow_and_saves_for_its_own_system_only() {
    let (d, mut a, _) = on_carousel();
    open_at(&mut a, QuickRow::GbaColourCorrection);
    assert_eq!(
        a.quick_value(QuickRow::GbaColourCorrection),
        Some(QuickValue::Off),
        "the row did not open on the default, which is off"
    );
    for (btn, want) in [
        (Btn::Right, true),
        (Btn::Left, false),
        (Btn::Left, true),
        (Btn::Right, false),
    ] {
        press(&mut a, btn);
        let s = read_slot_state(d.path());
        assert_eq!(s.colour_correction_gba, want, "{btn:?} never reached the card");
        assert!(!s.colour_correction_gb, "{btn:?} changed the Game Boys' too");
        assert_eq!(
            a.quick_value(QuickRow::GbaColourCorrection),
            Some(QuickValue::flag(want))
        );
    }
    open_at(&mut a, QuickRow::GbColourCorrection);
    press(&mut a, Btn::Right);
    let s = read_slot_state(d.path());
    assert_eq!((s.colour_correction_gb, s.colour_correction_gba), (true, false));
}

#[test]
fn the_game_page_flips_eject_save_turbo_and_rewind_and_saves_each() {
    let (d, mut a, _) = on_carousel();
    open_at(&mut a, QuickRow::Rewind);
    let card = |d: &TempDir| {
        let s = read_slot_state(d.path());
        (s.eject_save, s.turbo, s.rewind)
    };
    assert_eq!(card(&d), (true, true, true));
    press(&mut a, Btn::Right);
    assert_eq!(card(&d), (true, true, false));
    assert!(!a.may_rewind(), "rewind still offered with it off");
    press(&mut a, Btn::Down);
    press(&mut a, Btn::Left);
    assert_eq!(card(&d), (true, false, false));
    assert!(!a.turbo());
    press(&mut a, Btn::Down);
    press(&mut a, Btn::Down);
    press(&mut a, Btn::Right);
    assert_eq!(card(&d), (false, false, false));
    press(&mut a, Btn::B);
    assert_eq!(a.quick_menu(), Some(QuickRow::Game));
}

#[test]
fn the_game_page_steps_the_autosave_and_the_low_battery_shutdown_and_saves_each() {
    let (d, mut a, _) = on_carousel();
    let card = |d: &TempDir| {
        let s = read_slot_state(d.path());
        (s.autosave_min, s.battery_shutdown)
    };
    assert_eq!(card(&d), (1, 5));
    open_at(&mut a, QuickRow::AutoSave);
    press(&mut a, Btn::Right);
    assert_eq!(card(&d), (5, 5));
    assert_eq!(a.quick_value(QuickRow::AutoSave), Some(QuickValue::Min5));
    press(&mut a, Btn::Left);
    press(&mut a, Btn::Left);
    assert_eq!(card(&d), (0, 5));
    assert_eq!(a.quick_value(QuickRow::AutoSave), Some(QuickValue::Off));
    press(&mut a, Btn::Down);
    press(&mut a, Btn::Right);
    assert_eq!(card(&d), (0, 10));
    press(&mut a, Btn::Right);
    assert_eq!(card(&d), (0, 0));
    assert_eq!(a.quick_value(QuickRow::BatteryShutdown), Some(QuickValue::Off));
}

#[test]
fn screen_opens_a_page_per_system_each_with_its_own_page_and_b_comes_back_up() {
    let (_d, mut a, _) = on_carousel();
    open_at(&mut a, QuickRow::Screen);
    press(&mut a, Btn::A);
    assert_eq!(a.quick_menu(), Some(QuickRow::ScreenGb));
    for want in [QuickRow::ScreenGba, QuickRow::ScreenNes, QuickRow::ScreenSnes, QuickRow::ScreenPs1, QuickRow::ScreenGb] {
        press(&mut a, Btn::Down);
        assert_eq!(a.quick_menu(), Some(want), "the page did not wrap on itself");
    }
    press(&mut a, Btn::A);
    assert_eq!(a.quick_menu(), Some(QuickRow::GbShader));
    for want in [
        QuickRow::GbColourCorrection,
        QuickRow::GbPalettes,
        QuickRow::GbOverlay,
        QuickRow::GbcOverlay,
        QuickRow::GbShader,
    ] {
        press(&mut a, Btn::Down);
        assert_eq!(a.quick_menu(), Some(want), "the Game Boy page did not wrap on itself");
    }
    press(&mut a, Btn::B);
    assert_eq!(a.quick_menu(), Some(QuickRow::ScreenGb));
    for (system, first) in [
        (QuickRow::ScreenGba, QuickRow::GbaShader),
        (QuickRow::ScreenNes, QuickRow::NesShader),
        (QuickRow::ScreenSnes, QuickRow::SnesShader),
        (QuickRow::ScreenPs1, QuickRow::Ps1Shader),
    ] {
        press(&mut a, Btn::Down);
        assert_eq!(a.quick_menu(), Some(system));
        press(&mut a, Btn::A);
        assert_eq!(a.quick_menu(), Some(first));
        press(&mut a, Btn::B);
        assert_eq!(a.quick_menu(), Some(system), "B did not come back to {system:?}");
    }
    press(&mut a, Btn::B);
    assert_eq!(a.quick_menu(), Some(QuickRow::Screen));
    press(&mut a, Btn::B);
    assert_eq!(a.quick_menu(), None);
}

#[test]
fn nes_and_snes_shaders_are_set_from_their_own_pages() {
    let (d, mut a, _) = on_carousel();
    open_at(&mut a, QuickRow::NesShader);
    assert_eq!(a.quick_value(QuickRow::NesShader), Some(QuickValue::Off));
    press(&mut a, Btn::Right);
    assert_eq!(read_slot_state(d.path()).shader_nes, Shader::Grid);
    assert_eq!(read_slot_state(d.path()).shader_snes, Shader::Off, "the SNES took the NES one");
    open_at(&mut a, QuickRow::SnesShader);
    press(&mut a, Btn::Left);
    assert_eq!(read_slot_state(d.path()).shader_snes, Shader::Grid);
    assert_eq!(a.quick_value(QuickRow::SnesShader), Some(QuickValue::Grid));
}

#[test]
fn each_platform_keeps_its_own_shader_wraps_and_starts_on_grid_and_simpletex() {
    let (d, mut a, _) = on_carousel();
    open_at(&mut a, QuickRow::GbaShader);
    assert_eq!(a.quick_value(QuickRow::GbaShader), Some(QuickValue::Grid));
    assert_eq!(
        a.quick_value(QuickRow::GbShader),
        Some(QuickValue::Simpletex)
    );
    for (btn, want, shown) in [
        (Btn::Right, Shader::Dot, QuickValue::Dot),
        (Btn::Right, Shader::Off, QuickValue::Off),
        (Btn::Left, Shader::Dot, QuickValue::Dot),
        (Btn::Left, Shader::Grid, QuickValue::Grid),
        (Btn::Left, Shader::Lcd3x, QuickValue::Lcd3x),
        (Btn::Left, Shader::Off, QuickValue::Off),
    ] {
        press(&mut a, btn);
        assert_eq!(a.quick_value(QuickRow::GbaShader), Some(shown), "{btn:?}");
        assert_eq!(
            read_slot_state(d.path()).shader_gba,
            want,
            "{btn:?} never reached the card"
        );
    }
    assert_eq!(read_slot_state(d.path()).shader_gb, Shader::Simpletex);
    open_at(&mut a, QuickRow::GbShader);
    press(&mut a, Btn::Left);
    assert_eq!(read_slot_state(d.path()).shader_gb, Shader::Grid);
    assert_eq!(read_slot_state(d.path()).shader_gba, Shader::Off);
}

#[test]
fn colour_correction_leaves_the_settings_around_it_alone() {
    let (d, mut a, _) = on_carousel();
    open_at(&mut a, QuickRow::GbaColourCorrection);
    press(&mut a, Btn::Right);
    let s = read_slot_state(d.path());
    assert!(s.colour_correction_gba, "the row never took");
    assert_eq!(
        (s.ff_speed, s.ff_sound, s.rumble),
        (
            SlotState::default().ff_speed,
            SlotState::default().ff_sound,
            SlotState::default().rumble
        ),
        "the row reached a setting that is not its own"
    );
}

#[test]
fn the_arrows_change_nothing_on_a_row_that_opens() {
    let (d, mut a, _) = on_carousel();
    let before = std::fs::read(d.path().join("Config/slot.state")).expect("read slot.state");
    for row in [QuickRow::DateTime, QuickRow::About] {
        open_at(&mut a, row);
        press(&mut a, Btn::Left);
        press(&mut a, Btn::Right);
        assert_eq!(a.quick_menu(), Some(row), "an arrow left {row:?}");
        a.apply(Action::QuickMenu);
    }
    assert_eq!(
        std::fs::read(d.path().join("Config/slot.state")).expect("read slot.state"),
        before,
        "an arrow on a row that opens wrote the card"
    );
}

#[test]
fn a_on_about_opens_the_label_and_b_comes_back_to_the_menu() {
    let (_d, mut a, _) = on_carousel();
    open_at(&mut a, QuickRow::About);
    press(&mut a, Btn::A);
    assert!(matches!(a.phase(), Phase::About), "{:?}", a.phase());
    press(&mut a, Btn::B);
    assert_eq!(a.quick_menu(), Some(QuickRow::About));
}

#[test]
fn a_on_date_and_time_opens_the_clock_at_the_time_it_already_has() {
    let (_d, mut a, _) = on_carousel_with(SlotState {
        utc_offset_min: -300,
        ..SlotState::default()
    });
    open_at(&mut a, QuickRow::DateTime);
    press(&mut a, Btn::A);
    let picker = a.picker().expect("A on Date & Time did not open the clock");
    assert_eq!(picker.offset_min(), -300, "the offset started over");
    assert_eq!(
        picker.secs(),
        CLOCK_IS_SET - CLOCK_IS_SET % 60,
        "the picker does not show the minute the clock is in"
    );
}

#[test]
fn confirming_the_clock_from_the_menu_sets_it_and_comes_back_to_the_menu() {
    let (d, mut a, clock) = on_carousel_with(SlotState {
        utc_offset_min: -300,
        ..SlotState::default()
    });
    open_at(&mut a, QuickRow::DateTime);
    press(&mut a, Btn::A);
    press(&mut a, Btn::Up);
    for _ in 0..5 {
        press(&mut a, Btn::Right);
    }
    press(&mut a, Btn::Down);
    let changed = a.picker().expect("not on the clock").secs() - (CLOCK_IS_SET - CLOCK_IS_SET % 60);
    press(&mut a, Btn::A);
    assert_eq!(a.quick_menu(), Some(QuickRow::DateTime), "{:?}", a.phase());
    assert_eq!(
        clock.get(),
        CLOCK_IS_SET + changed,
        "the platform clock was not set"
    );
    let s = read_slot_state(d.path());
    assert_eq!(s.utc_offset_min, -330, "the offset was not saved");
    assert!(s.clock_set);
}

#[test]
fn b_on_the_clock_from_the_menu_comes_back_without_changing_anything() {
    let (d, mut a, clock) = on_carousel_with(SlotState {
        utc_offset_min: -300,
        ..SlotState::default()
    });
    open_at(&mut a, QuickRow::DateTime);
    press(&mut a, Btn::A);
    press(&mut a, Btn::Up);
    for _ in 0..5 {
        press(&mut a, Btn::Right);
    }
    press(&mut a, Btn::Down);
    press(&mut a, Btn::B);
    assert_eq!(a.quick_menu(), Some(QuickRow::DateTime), "{:?}", a.phase());
    assert_eq!(clock.get(), CLOCK_IS_SET, "B set the clock");
    assert_eq!(
        read_slot_state(d.path()).utc_offset_min,
        -300,
        "B saved the offset"
    );
}

#[test]
fn the_first_boot_clock_still_has_no_way_back() {
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    let (mut a, _) = app_booting_with_clock(d.path());
    press(&mut a, Btn::B);
    a.apply(Action::QuickMenu);
    assert!(
        matches!(a.phase(), Phase::SetClock { .. }),
        "left the first boot clock: {:?}",
        a.phase()
    );
}

#[test]
fn brightness_and_volume_still_answer_over_the_quick_menu() {
    let (d, mut a, _) = on_carousel();
    let icons: Vec<TexId> = (0..Icon::ALL.len())
        .map(|i| TexId::from_raw(700 + i))
        .collect();
    a.set_icon_faces(icons.clone());
    open_at(&mut a, QuickRow::FastForward);
    let before = read_slot_state(d.path());
    a.apply(Action::BrightnessUp);
    a.apply(Action::VolumeDown);
    a.flush_state();
    let after = read_slot_state(d.path());
    assert_eq!(
        (after.brightness, after.volume),
        (before.brightness + 1, before.volume - 5)
    );
    assert_eq!(
        a.quick_menu(),
        Some(QuickRow::FastForward),
        "a level moved the menu"
    );
    let out = frame(&a);
    assert!(
        out.iter()
            .any(|d| matches!(*d, Draw::Tex { tex, .. } if icons.contains(&tex))),
        "the level's bar is not drawn over the menu"
    );
}

fn fake_faces(a: &mut App) {
    let id = TexId::from_raw;
    a.set_quick_menu_faces(QuickMenuFaces {
        labels: (0..QuickRow::ALL.len())
            .map(|i| (id(100 + i), 200, 40))
            .collect(),
        values: (0..QuickValue::ALL.len())
            .map(|i| [(id(200 + i), 60, 40), (id(210 + i), 60, 40)])
            .collect(),
        carets: [(id(300), 10, 40), (id(301), 10, 40)],
        legend: [(id(400), 70), (id(401), 110), (id(402), 80)],
    });
    a.set_quick_clock_faces((id(500), 150, 40), (id(501), 150, 40));
}

fn value(v: QuickValue, lit: bool) -> usize {
    if lit {
        210 + v.index()
    } else {
        200 + v.index()
    }
}

fn frame(a: &App) -> Vec<Draw> {
    let mut out = Vec::new();
    a.draw(&mut out);
    out
}

fn placed(out: &[Draw], id: usize) -> Option<[f32; 4]> {
    out.iter().find_map(|d| match *d {
        Draw::Tex {
            x, y, w, h, tex, ..
        } if tex == TexId::from_raw(id) => Some([x, y, w, h]),
        _ => None,
    })
}

fn drawn(out: &[Draw], id: usize) -> bool {
    placed(out, id).is_some()
}

#[test]
fn the_legend_says_change_on_a_value_row_and_open_on_a_row_that_opens() {
    let (_d, mut a, _) = on_carousel();
    fake_faces(&mut a);
    a.apply(Action::QuickMenu);
    for row in QuickRow::MAIN {
        assert_eq!(a.quick_menu(), Some(row));
        let out = frame(&a);
        // Cart Art, RetroAchievements and Update are greyed out with no Wi-Fi, as here: only BACK.
        let usable = !matches!(row, QuickRow::CartArt | QuickRow::Achievements | QuickRow::Update);
        assert!(drawn(&out, 400), "no B BACK on {row:?}");
        assert_eq!(drawn(&out, 401), !row.opens(), "CHANGE on {row:?}");
        assert_eq!(drawn(&out, 402), row.opens() && usable, "OPEN on {row:?}");
        press(&mut a, Btn::Down);
    }
}

#[cfg(feature="brick")]
#[test]
fn cart_art_and_retroachievements_do_nothing_while_wifi_is_off() {
    for row in [QuickRow::CartArt, QuickRow::Achievements] {
        let (_d, mut a, _) = on_carousel();
        open_at(&mut a, row);
        press(&mut a, Btn::A);
        assert!(!a.overlay_open(), "{row:?} opened its overlay");
        assert_eq!(a.quick_menu(), Some(row));
    }
}

#[test]
fn the_arrows_stand_only_around_the_selected_rows_value() {
    let (_d, mut a, _) = on_carousel();
    fake_faces(&mut a);
    open_at(&mut a, QuickRow::FastForward);
    let out = frame(&a);
    assert!(
        drawn(&out, 300) && drawn(&out, 301),
        "no arrows on Fast Forward"
    );
    assert!(
        drawn(&out, value(QuickValue::Speed6, true)),
        "6× is not lit"
    );
    assert!(
        drawn(&out, value(QuickValue::Off, false)),
        "sound's OFF is not grey"
    );
    assert!(
        drawn(&out, value(QuickValue::On, false)),
        "rumble's ON is not grey"
    );
    open_at(&mut a, QuickRow::About);
    let out = frame(&a);
    assert!(drawn(&out, 500), "the date and time is not grey");

    open_at(&mut a, QuickRow::DateTime);
    let out = frame(&a);
    assert!(
        !drawn(&out, 300) && !drawn(&out, 301),
        "arrows on a row that opens"
    );
    assert!(drawn(&out, 501), "the date and time in hand is not lit");
}

#[test]
fn the_bar_runs_edge_to_edge_behind_the_selected_row() {
    let (_d, mut a, _) = on_carousel();
    fake_faces(&mut a);
    a.apply(Action::QuickMenu);
    for row in QuickRow::MAIN {
        let bars: Vec<_> = frame(&a)
            .into_iter()
            .filter_map(|d| match d {
                Draw::Rect { x, y, w, h, colour } if colour == edge() => Some([x, y, w, h]),
                _ => None,
            })
            .collect();
        let pitch = slot_ui::quick_pitch(QuickRow::MAIN.len());
        let top = QUICK_TOP + pitch * row.position() as f32;
        assert_eq!(
            bars,
            vec![[0.0, top + 4.0, OUT_W as f32, pitch - 8.0]],
            "{row:?}"
        );
        press(&mut a, Btn::Down);
    }
}

#[test]
fn labels_start_and_values_end_thirty_two_pixels_in() {
    let (_d, mut a, _) = on_carousel();
    fake_faces(&mut a);
    open_at(&mut a, QuickRow::FastForward);
    let out = frame(&a);
    let right = OUT_W as f32 - QUICK_EDGE;
    for row in QuickRow::GAME {
        let [x, ..] = placed(&out, 100 + row.index()).expect("a label was not drawn");
        assert_eq!(x + MENU_PAD as f32, QUICK_EDGE, "{row:?}'s label");
    }
    let [x, _, w, _] = placed(&out, value(QuickValue::On, false)).expect("rumble's value");
    assert_eq!(x + w - MENU_PAD as f32, right, "an unselected value");
    let [x, _, w, _] = placed(&out, 301).expect("the right arrow");
    assert_eq!(x + w, right, "the arrow around the selected value");
}

#[test]
fn only_the_clock_from_the_menu_offers_b_back() {
    let (_d, mut a, _) = on_carousel();
    fake_faces(&mut a);
    a.set_clock_faces(TexId::from_raw(600), TexId::from_raw(601));
    open_at(&mut a, QuickRow::DateTime);
    press(&mut a, Btn::A);
    let out = frame(&a);
    assert!(drawn(&out, 601), "the clock lost its own key");
    assert!(
        drawn(&out, 400),
        "the clock from the menu does not offer B BACK"
    );

    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    let (mut first, _) = app_booting_with_clock(d.path());
    fake_faces(&mut first);
    first.set_clock_faces(TexId::from_raw(600), TexId::from_raw(601));
    let out = frame(&first);
    assert!(drawn(&out, 601), "the first boot clock lost its key");
    assert!(
        !drawn(&out, 400),
        "the first boot clock offers a way back it does not have"
    );
}

#[test]
fn gb_palettes_flips_on_either_arrow_and_saves() {
    let (d, mut a, _) = on_carousel();
    open_at(&mut a, QuickRow::GbPalettes);
    assert_eq!(a.quick_value(QuickRow::GbPalettes), Some(QuickValue::Off));
    for (btn, want) in [(Btn::Right, true), (Btn::Left, false), (Btn::Left, true)] {
        press(&mut a, btn);
        assert_eq!(
            read_slot_state(d.path()).gb_palettes,
            want,
            "{btn:?} never reached the card"
        );
        assert_eq!(
            a.quick_value(QuickRow::GbPalettes),
            Some(QuickValue::flag(want))
        );
    }
}

#[cfg(feature="brick")]
#[test]
fn wifi_keeps_navigation_inside_overlay_and_b_returns_to_its_row() {
    let (_d,mut a,_)=on_carousel();open_at(&mut a,QuickRow::Wifi);press(&mut a,Btn::A);
    press(&mut a,Btn::Down);
    assert_eq!(a.quick_menu(),Some(QuickRow::Wifi));
    // Status may still be running. B first cancels it, then closes the overlay.
    for i in 0..100 {
        a.tick_ms(5000+i*10);press(&mut a,Btn::B);press(&mut a,Btn::Down);
        if a.quick_menu()==Some(QuickRow::CartArt){return}
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("Wi-Fi overlay did not return to settings");
}

#[cfg(feature="brick")]
#[test]
fn wifi_status_finishes_before_a_pending_sleep_request() {
    let (_d,mut a,_)=on_carousel();open_at(&mut a,QuickRow::Wifi);press(&mut a,Btn::A);
    a.apply(Action::PowerTap);
    for i in 0..100 {
        a.tick_ms(5000+i*10);
        if matches!(a.phase(),Phase::Doze{..}) {return}
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("Pending sleep was not completed after the Wi-Fi worker stopped");
}

#[test]
fn an_overlay_row_steps_through_off_and_the_overlays_there_are_and_saves_each() {
    let (d, mut a, _) = on_carousel();
    let dir = d.path().join("Overlays/GBA");
    std::fs::create_dir_all(&dir).unwrap();
    for f in ["SP.png", "Logo.png", "notes.txt"] {
        std::fs::write(dir.join(f), b"x").unwrap();
    }
    open_at(&mut a, QuickRow::GbaOverlay);
    assert_eq!(a.overlay_text(Platform::Gba), "Off");
    for (btn, want, shown) in [
        (Btn::Right, Some("Logo.png"), "Logo"),
        (Btn::Right, Some("SP.png"), "SP"),
        (Btn::Right, None, "Off"),
        (Btn::Left, Some("SP.png"), "SP"),
    ] {
        press(&mut a, btn);
        assert_eq!(read_slot_state(d.path()).overlay(Platform::Gba), want, "{btn:?}");
        assert_eq!(a.overlay_text(Platform::Gba), shown);
    }
    assert_eq!(read_slot_state(d.path()).overlay(Platform::Gb), None, "the Game Boy took the GBA's");
}
