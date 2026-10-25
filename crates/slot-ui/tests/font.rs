use slot_ui::text::{font_for, label_font, set_label_font};

/// A chosen font draws all text, and a character it lacks comes from Slot's own font.
#[test]
fn a_chosen_font_draws_the_text_and_the_built_in_one_fills_its_gaps() {
    let built_in = label_font().expect("Slot's own font") as *const _;
    let gohu = include_bytes!("../../../brick/card/Slot/System/Fonts/GohuFont.ttf");
    assert!(set_label_font(gohu));
    let chosen = label_font().expect("the chosen font");
    assert!(
        !std::ptr::eq(chosen, built_in),
        "the chosen font is not in use"
    );
    assert!(
        std::ptr::eq(font_for(chosen, 'É'), chosen),
        "GohuFont has É"
    );
    assert!(
        std::ptr::eq(font_for(chosen, '漢'), built_in),
        "a character GohuFont lacks"
    );
    assert!(std::ptr::eq(font_for(chosen, ' '), chosen));
    assert!(
        !set_label_font(b"not a font"),
        "a second font, or one that is not, is refused"
    );
}
