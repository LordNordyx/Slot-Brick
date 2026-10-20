#![cfg(feature = "brick")]
use slot::app::App;
use slot_input::{Action, Btn};
use slot_ui::{QuickRow, QuickValue};
#[test]
fn led_page_uses_existing_navigation_and_handles_missing_hardware() {
    let mut app=App::new(Vec::new());
    app.apply(Action::QuickMenu);
    app.apply(Action::GbaDown(Btn::Up));
    assert_eq!(app.quick_menu(),Some(QuickRow::System));
    app.apply(Action::GbaDown(Btn::A));
    assert_eq!(app.quick_menu(),Some(QuickRow::Lights));
    app.apply(Action::GbaDown(Btn::A));
    assert_eq!(app.quick_menu(),Some(QuickRow::LedMode));
    assert_eq!(app.quick_value(QuickRow::LedMode),Some(QuickValue::Unavailable));
    app.apply(Action::GbaDown(Btn::Right)); // should not panic or write hardware
    app.apply(Action::GbaDown(Btn::Up));
    assert_eq!(app.quick_menu(),Some(QuickRow::LedEffect));
    app.apply(Action::GbaDown(Btn::B));
    assert_eq!(app.quick_menu(),Some(QuickRow::Lights));
    app.apply(Action::GbaDown(Btn::B));
    assert_eq!(app.quick_menu(),Some(QuickRow::System));
    app.apply(Action::GbaDown(Btn::B));
    assert_eq!(app.quick_menu(),None);
}
