//! The simulator's buttons: whatever its drivers queue, with the Brick's own exit chord.
use crate::sim::Shared;
use slot_input::{Btn, InputSource, Millis, RawEvent};
use std::sync::Arc;

pub struct SimInput {
    shared: Arc<Shared>,
    held: Vec<Btn>,
    exit_since: Option<Millis>,
    exit: bool,
}

impl SimInput {
    pub fn new(shared: Arc<Shared>) -> Self {
        Self {
            shared,
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
        let events = self.shared.take_events();
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
