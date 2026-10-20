use slot_store::{parse_stamp, FF_SPEEDS};
use slot_ui::{
    date_time_text, quick_caret_face, quick_label_face, quick_value_face, ClockPicker, QuickRow,
    QuickValue, UndoFace, MENU_PAD,
};

fn inked(f: &UndoFace, x: u32, y: u32) -> bool {
    f.rgba[((y * f.w + x) * 4 + 3) as usize] > 128
}

fn ink_columns(f: &UndoFace) -> (u32, u32) {
    let cols: Vec<u32> = (0..f.w)
        .filter(|&x| (0..f.h).any(|y| inked(f, x, y)))
        .collect();
    (
        *cols.first().expect("an empty face"),
        *cols.last().expect("an empty face"),
    )
}

#[test]
fn the_type_sits_exactly_menu_pad_in_from_both_sides_of_its_face() {
    for f in [
        quick_label_face(QuickRow::FastForwardSound),
        quick_label_face(QuickRow::Rumble),
        quick_value_face("Off", false),
        quick_value_face(QuickValue::Speed6.text(), true),
        quick_value_face("SEP 15 16:35", true),
    ] {
        let (first, last) = ink_columns(&f);
        let right = f.w - 1 - MENU_PAD;
        assert!(
            (MENU_PAD..MENU_PAD + 4).contains(&first),
            "ink starts at {first} of {}",
            f.w
        );
        assert!(
            (right - 4..=right).contains(&last),
            "ink ends at {last} of {}",
            f.w
        );
    }
}

#[test]
fn a_long_label_is_set_as_large_as_a_short_one() {
    let tall = |f: &UndoFace| {
        (0..f.h)
            .filter(|&y| (0..f.w).any(|x| inked(f, x, y)))
            .count()
    };
    let long = tall(&quick_label_face(QuickRow::FastForwardSound));
    let short = tall(&quick_label_face(QuickRow::Rumble));
    assert!(long + 1 >= short, "{long} rows of ink against {short}");
}

#[test]
fn the_fast_forward_row_offers_the_four_ceilings_the_card_can_hold() {
    assert_eq!(FF_SPEEDS, [2, 3, 4, 6]);
    assert_eq!(
        FF_SPEEDS.map(QuickValue::speed),
        [
            Some(QuickValue::Speed2),
            Some(QuickValue::Speed3),
            Some(QuickValue::Speed4),
            Some(QuickValue::Speed6),
        ]
    );
    for other in [1, 5, 7, 9, 16, 28, 255] {
        assert_eq!(
            QuickValue::speed(other),
            None,
            "{other}x is not a value the row has"
        );
    }
}

#[test]
fn the_rows_run_in_the_order_the_user_chose() {
    #[cfg(not(feature="brick"))]
    assert_eq!(QuickRow::MAIN.map(QuickRow::label), ["Screen", "Gameplay", "Date & Time", "About"]);
    #[cfg(feature="brick")]
    assert_eq!(QuickRow::MAIN.map(QuickRow::label), ["Screen", "Gameplay", "LEDs", "Wi-Fi", "Cart Art", "RetroAchievements", "Update", "Date & Time", "About"]);
    assert_eq!(QuickRow::SCREEN.map(QuickRow::label), ["Game Boy / GBC", "Game Boy Advance", "NES", "SNES"]);
    assert_eq!(
        QuickRow::SCREEN_GB.map(QuickRow::label),
        ["Shader", "Color Correction", "GB Palettes", "GB Overlay", "GBC Overlay"]
    );
    assert_eq!(QuickRow::SCREEN_GBA.map(QuickRow::label), ["Shader", "Color Correction", "Overlay"]);
    assert_eq!(QuickRow::SCREEN_NES.map(QuickRow::label), ["Shader", "Overlay"]);
    assert_eq!(QuickRow::SCREEN_SNES.map(QuickRow::label), ["Shader", "Overlay"]);
    // Each Overlay row picks the overlay for its own system.
    use slot_store::Platform;
    let systems: Vec<_> = QuickRow::ALL.into_iter().filter_map(QuickRow::overlay_of).collect();
    assert_eq!(systems, [Platform::Gb, Platform::Gbc, Platform::Gba, Platform::Nes, Platform::Snes]);
    assert_eq!(
        QuickRow::GAME.map(QuickRow::label),
        [
            "Fast Forward",
            "Fast Forward Sound",
            "Rewind",
            "Turbo Buttons",
            "Rumble",
            "Auto Save on Eject",
            "Auto Save",
            "Low Battery Shutdown"
        ]
    );
    let opens: Vec<QuickRow> = QuickRow::ALL.into_iter().filter(|r| r.opens()).collect();
    assert_eq!(
        opens,
        [
            QuickRow::Screen,
            QuickRow::DateTime,
            QuickRow::About,
            QuickRow::Game,
            QuickRow::Lights,
            QuickRow::Wifi,
            QuickRow::CartArt,
            QuickRow::Achievements,
            QuickRow::ScreenGb,
            QuickRow::ScreenGba,
            QuickRow::ScreenNes,
            QuickRow::ScreenSnes,
            QuickRow::Update
        ]
    );
    for page in [
        &QuickRow::MAIN[..],
        &QuickRow::SCREEN[..],
        &QuickRow::SCREEN_GB[..],
        &QuickRow::SCREEN_GBA[..],
        &QuickRow::SCREEN_NES[..],
        &QuickRow::SCREEN_SNES[..],
        &QuickRow::GAME[..],
    ] {
        for &row in page {
            assert_eq!(row.page(), page, "{row:?}");
        }
    }
}

#[test]
fn the_values_read_as_the_menu_prints_them() {
    assert_eq!(
        QuickValue::ALL.map(QuickValue::text),
        [
            "2×",
            "3×",
            "4×",
            "6×",
            "On",
            "Off",
            "LCD3x",
            "Grid",
            "Dot",
            "Simpletex", "Stock", "Low", "Medium", "High", "White", "Blue", "Green", "Amber", "Red", "Purple", "Steady", "Breathe", "Unavailable",
            "1 min", "5 min", "10 min", "3%", "5%", "10%"
        ]
    );
    assert_eq!(
        slot_store::AUTOSAVE_MINS.map(QuickValue::minutes),
        [Some(QuickValue::Off), Some(QuickValue::Min1), Some(QuickValue::Min5), Some(QuickValue::Min10)]
    );
    assert_eq!(
        slot_store::BATTERY_SHUTDOWNS.map(QuickValue::percent),
        [Some(QuickValue::Off), Some(QuickValue::Percent3), Some(QuickValue::Percent5), Some(QuickValue::Percent10)]
    );
    assert_eq!(QuickValue::flag(true), QuickValue::On);
    assert_eq!(QuickValue::flag(false), QuickValue::Off);
}

#[test]
fn the_date_and_time_read_as_a_month_a_day_and_the_carousels_24_hour_clock() {
    let at = |stamp: &str| date_time_text(parse_stamp(stamp).expect("a stamp"));
    assert_eq!(at("2026-09-15_16-35-00"), "SEP 15 16:35");
    assert_eq!(at("2027-01-05_04-07-59"), "JAN 5 04:07");
}

#[test]
fn a_picker_for_a_set_clock_starts_at_the_local_time_and_its_offset() {
    let utc = parse_stamp("2026-09-15_21-35-42").expect("a stamp");
    let p = ClockPicker::local(utc, -300);
    assert_eq!(p.offset_min(), -300);
    assert_eq!(
        p.secs(),
        utc - 42,
        "the picker does not show the minute it was opened in"
    );
    assert!(p.text().starts_with("2026-09-15 16:35"), "{}", p.text());
}

#[test]
fn a_value_is_grey_until_its_row_is_in_hand() {
    let inkiest = |lit| {
        let f = quick_value_face("4×", lit);
        f.rgba
            .chunks(4)
            .max_by_key(|p| p[3])
            .map(|p| [p[0], p[1], p[2]])
            .expect("an empty face")
    };
    assert_eq!(inkiest(true), [0xf6, 0xf4, 0xef]);
    assert_eq!(inkiest(false), [0x9a, 0x9a, 0xa4]);
}

#[test]
fn the_arrows_are_faces_the_height_of_a_value() {
    for right in [false, true] {
        let caret = quick_caret_face(right);
        assert!(
            caret.w > 0 && caret.rgba.chunks(4).any(|p| p[3] > 0),
            "an empty arrow"
        );
        assert_eq!(caret.h, quick_value_face("On", true).h);
    }
}

#[test]
fn a_value_gets_the_room_beside_its_label_and_less_of_it_beside_the_carets() {
    use slot_ui::{quick_caret_face, quick_label_face, quick_value_room};
    let label = quick_label_face(QuickRow::GbaOverlay).w;
    let carets = (quick_caret_face(false).w, quick_caret_face(true).w);
    let (dim, lit) = (quick_value_room(label, None), quick_value_room(label, Some(carets)));
    assert!(lit > 200 && dim > lit, "dim {dim}, lit {lit}");
    // A longer label leaves less room.
    assert!(quick_value_room(quick_label_face(QuickRow::GbcOverlay).w, None) < dim);
}
