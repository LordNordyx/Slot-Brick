//! The simulator's buttons: whatever its drivers queue, with the Brick's own exit chord.
use crate::sim::Shared;
use slot_input::{Btn, InputSource, Millis, RawEvent};
use std::sync::Arc;

pub struct SimInput {
    shared: Arc<Shared>,
    /// A Brick Pro's left stick, through the same D-pad translation as on the device.
    stick: Option<super::brick::StickPad>,
    held: Vec<Btn>,
    exit_since: Option<Millis>,
    exit: bool,
}

impl SimInput {
    pub fn new(shared: Arc<Shared>) -> Self {
        Self {
            shared,
            stick: (slot_power::model() == slot_power::Model::BrickPro).then(super::brick::StickPad::default),
            held: Vec::new(),
            exit_since: None,
            exit: false,
        }
    }
    pub fn exit_requested(&self) -> bool {
        self.exit
    }
}

impl InputSource for SimInput {
    fn poll(&mut self, now: Millis) -> Vec<RawEvent> {
        let mut events = self.shared.take_events();
        for (axis, value) in self.shared.take_axes() {
            if let Some(stick) = &mut self.stick {
                let before = stick.bits;
                if let Some(bits) = stick.axis(axis, value) {
                    events.extend(super::brick::hat_events(before, bits));
                }
            }
        }
        for e in &events {
            match *e {
                RawEvent::Down(b) if !self.held.contains(&b) => self.held.push(b),
                RawEvent::Down(_) => {}
                RawEvent::Up(b) => self.held.retain(|h| *h != b),
            }
        }
        // As on the Brick: Menu + Start + Select held for a second saves and exits.
        if [Btn::Menu, Btn::Start, Btn::Select]
            .iter()
            .all(|b| self.held.contains(b))
        {
            let since = *self.exit_since.get_or_insert(now);
            self.exit = now.saturating_sub(since) >= 1000;
        } else {
            self.exit_since = None;
        }
        events
    }
}
