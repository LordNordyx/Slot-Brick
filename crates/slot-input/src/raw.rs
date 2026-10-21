#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Btn {
    Up,
    Down,
    Left,
    Right,
    A,
    B,
    X,
    Y,
    L1,
    R1,
    L2,
    R2,
    Start,
    Select,
    Menu,
    VolUp,
    VolDown,
    Power,
    Lid,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum RawEvent {
    Down(Btn),
    Up(Btn),
}

pub type Millis = u64;

pub trait InputSource {
    fn poll(&mut self, now: Millis) -> Vec<RawEvent>;
    /// Where an analog stick stands, -32768..32767 across then down, on a pad that has one.
    fn stick(&self) -> Option<(i16, i16)> {
        None
    }
    /// The stick to the game as a stick (a PlayStation game's DualShock), not as the D-pad.
    fn set_stick_to_game(&mut self, _on: bool) {}
}
