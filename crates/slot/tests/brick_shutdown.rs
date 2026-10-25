#![cfg(feature = "brick")]
use slot::app::App;
use slot_input::{Action, Btn};
use slot_ui::PowerChoice;
#[test]
fn holding_power_opens_menu_and_release_does_not_confirm() {
    let mut app = App::new(Vec::new());
    app.apply(Action::PowerHold);
    assert_eq!(app.power_menu(), Some(0));
    app.apply(Action::PowerOff); // emitted when the held power button is released
    assert_eq!(app.power_menu(), Some(0));
    assert!(!app.shutting_down());
    app.apply(Action::GbaDown(Btn::B));
    assert!(app.power_menu().is_none());
    assert!(!app.shutting_down());
}
#[test]
fn menu_routes_exit_and_shutdown_separately() {
    assert_eq!(PowerChoice::Restart.text(), "EXIT TO STOCK OS");
    assert_eq!(PowerChoice::PowerOff.text(), "SHUT DOWN DEVICE");
    let mut app = App::new(Vec::new());
    app.apply(Action::PowerHold);
    app.apply(Action::GbaDown(Btn::A));
    assert!(app.powering_off());
    assert!(!app.restarting());
    let mut app = App::new(Vec::new());
    app.apply(Action::PowerHold);
    app.apply(Action::GbaDown(Btn::Down));
    app.apply(Action::GbaDown(Btn::A));
    assert!(app.restarting());
    assert!(!app.powering_off());
    assert_eq!(PowerChoice::PowerOff.index(), 0);
    assert_eq!(PowerChoice::Restart.index(), 1);
}
#[test]
fn on_slot_os_holding_power_shuts_down_without_a_menu() {
    let mut app = App::new(Vec::new());
    app.set_power_off_only(true);
    app.apply(Action::PowerHold);
    assert!(app.power_menu().is_none());
    assert!(app.powering_off());
    assert!(!app.restarting());
}
