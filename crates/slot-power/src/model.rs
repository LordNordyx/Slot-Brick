//! Which TrimUI Brick this is. Both run the same firmware platform (tg5040); the firmware's
//! MainUI names the model it was built for, which is how NextUI tells them apart too.
use std::sync::OnceLock;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Model {
    /// The Brick, and the Brick Hammer (the same inside).
    Brick,
    /// Bigger screen and battery, two analog sticks with lights around them, a rear light zone.
    BrickPro,
}

impl Model {
    pub fn name(self) -> &'static str {
        match self {
            Model::Brick => "Brick",
            Model::BrickPro => "Brick Pro",
        }
    }
}

const MAIN_UI: &str = "/usr/trimui/bin/MainUI";

/// The model this is running on, read once. Anything but a Brick Pro's MainUI (or none at
/// all) is taken for a Brick.
pub fn model() -> Model {
    static MODEL: OnceLock<Model> = OnceLock::new();
    *MODEL.get_or_init(|| match std::fs::read(crate::hw(MAIN_UI)) {
        Ok(bytes) => model_of(&bytes),
        Err(_) => Model::Brick,
    })
}

/// MainUI holds its model's name as a string of its own: "Trimui Brick Pro", or "Trimui Brick".
fn model_of(main_ui: &[u8]) -> Model {
    let pro = b"Trimui Brick Pro\0";
    match main_ui.windows(pro.len()).any(|w| w == pro) {
        true => Model::BrickPro,
        false => Model::Brick,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_brick_pro_is_known_by_its_name_in_main_ui() {
        assert_eq!(model_of(b"\x7fELF...\0Trimui Brick Pro\0more"), Model::BrickPro);
        assert_eq!(model_of(b"\x7fELF...\0Trimui Brick\0more"), Model::Brick);
        assert_eq!(model_of(b"\0Trimui Brick Pros and cons\0"), Model::Brick);
        assert_eq!(model_of(b""), Model::Brick);
    }
}
