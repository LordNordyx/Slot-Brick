use slot_ui::{sticker_lines, StickerFields};

fn fields() -> StickerFields<'static> {
    StickerFields {
        battery: Some(87),
        serial: "0473885",
        dirty_digit: '0',
    }
}

#[test]
fn the_headline_rows_name_the_brick_port() {
    let all = sticker_lines(&fields()).join("\n");
    assert!(all.contains("TRIMUI BRICK"), "{all}");
    assert!(all.contains(slot_ui::PORT_VERSION), "{all}");
    assert!(all.contains("87"), "the gauge reading is missing: {all}");
    assert!(all.contains("0473885"), "the serial went missing: {all}");
    assert!(!all.contains("AGS-102"), "the RG SP model plate is still there: {all}");
}

#[test]
fn the_about_screen_says_it_is_a_port_of_the_original() {
    let all = sticker_lines(&fields()).join(" ");
    for owed in ["UNOFFICIAL PORT OF SLOT", "BRANDON T. KOWALSKI", "ANBERNIC RG SP", "TRIMUI BRICK PORT", "PORT BY LORDNORDYX"] {
        assert!(all.contains(owed), "the About text does not mention {owed}: {all}");
    }
}

#[test]
fn a_missing_gauge_is_not_drawn_as_empty() {
    let mut f = fields();
    f.battery = None;
    let all = sticker_lines(&f).join("\n");
    assert!(
        !all.contains("0%"),
        "no gauge was drawn as a flat battery: {all}"
    );
    assert!(
        all.contains("BATTERY"),
        "the row should still be there: {all}"
    );
}

#[test]
fn the_compliance_block_is_the_credits() {
    let all = sticker_lines(&fields()).join("\n").to_uppercase();
    for owed in [
        "MGBA",
        "GPSP",
        "OPEN SANS",
        "NERD",
        "LCD3X",
        "CLAUDE",
    ] {
        assert!(all.contains(owed), "the credits do not mention {owed}");
    }
}

#[test]
fn the_serial_row_matches_the_encoded_hash() {
    let all = sticker_lines(&fields()).join("\n");
    assert!(all.contains("0473885"), "{all}");
}

#[test]
fn every_line_is_already_upper_case() {
    for line in sticker_lines(&fields()) {
        assert_eq!(line, line.to_uppercase(), "{line}");
    }
}

#[test]
fn every_credit_line_fits_its_column() {
    // The left column ends where the barcode panel starts.
    let left_column = 660.0 * 62.133 / 205.762 - 2.0 * 11.0;
    let font = slot_ui::text::label_font().expect("label font");
    for line in slot_ui::CREDITS.iter().chain(slot_ui::head_rows(&fields()).iter().map(|s| s.as_str()).collect::<Vec<_>>().iter()) {
        let px = if slot_ui::CREDITS.contains(line) { 8.5 } else { 11.0 };
        let layout = slot_ui::text::fit(font, line, f32::MAX, 1, px, px);
        let w = slot_ui::text::line_width(font, &layout.lines[0], px, layout.tracking);
        assert!(w <= left_column, "{line:?} is {w}px wide, column is {left_column}px");
    }
}
