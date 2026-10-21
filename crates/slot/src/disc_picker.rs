//! A PlayStation game of several discs: A tilts its disc and fans the others out from behind
//! it. Left and right bring a disc to the front, A puts that one in, B puts them back.
use std::path::PathBuf;

use slot_gfx::{Draw, TexId, OUT_W};
use slot_ui::{ease, rest_y, Millis, DISC_PX};

/// Which disc of each game was put in last, by the game's stem, from 1.
pub const DISCS_FILE: &str = "Config/discs.ini";

pub const OPEN_MS: Millis = 280;
pub const CLOSE_MS: Millis = 220;
/// How far apart the fanned discs stand, how much higher each one behind stands, how much
/// smaller it is, and how far each one leans.
const FAN_X: f32 = 110.0;
const FAN_RISE: f32 = 16.0;
const FAN_SHRINK: f32 = 0.14;
const FAN_LEAN: f32 = 0.10;
/// The front disc's own tilt, as it is lifted to show the ones behind.
const TILT: f32 = -0.06;
/// How quickly a disc chosen with left or right comes to the front.
const BRING: f32 = 14.0;

pub struct DiscPicker {
    /// The game: its stem on the shelf.
    pub stem: String,
    pub discs: Vec<PathBuf>,
    pub clean: bool,
    selected: usize,
    /// Where the front of the fan stands now, in discs: easing toward `selected`.
    shown: f32,
    opened: Millis,
    close: Option<(Millis, f32)>,
}

impl DiscPicker {
    /// The fan opens on disc `at` (from 0): the one put in last.
    pub fn open(stem: String, discs: Vec<PathBuf>, at: usize, clean: bool, now: Millis) -> Self {
        let selected = at.min(discs.len().saturating_sub(1));
        DiscPicker { stem, discs, clean, selected, shown: selected as f32, opened: now, close: None }
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    /// One disc to the left or right; false at either end of the fan.
    pub fn step(&mut self, by: i32) -> bool {
        let to = self.selected as i32 + by;
        if self.close.is_some() || to < 0 || to >= self.discs.len() as i32 {
            return false;
        }
        self.selected = to as usize;
        true
    }

    pub fn update(&mut self, dt: f32) {
        let to = self.selected as f32;
        self.shown += (to - self.shown) * (dt * BRING).min(1.0);
        if (to - self.shown).abs() < 0.001 {
            self.shown = to;
        }
    }

    pub fn close(&mut self, now: Millis) {
        if self.close.is_none() {
            self.close = Some((now, self.openness(now)));
        }
    }

    pub fn closing(&self) -> bool {
        self.close.is_some()
    }

    pub fn finished(&self, now: Millis) -> bool {
        self.close.is_some() && self.openness(now) <= 0.0
    }

    pub fn openness(&self, now: Millis) -> f32 {
        match self.close {
            Some((at, from)) => (from - now.saturating_sub(at) as f32 / CLOSE_MS as f32).max(0.0),
            None => (now.saturating_sub(self.opened) as f32 / OPEN_MS as f32).min(1.0),
        }
    }

    /// Still moving: the fan opening or closing, or a disc on its way to the front.
    pub fn moving(&self, now: Millis) -> bool {
        let open = self.openness(now);
        (open > 0.0 && open < 1.0) || self.shown != self.selected as f32
    }

    /// The fan, farthest disc first, so the one in front is drawn last. `faces` holds each
    /// disc's face; `fallback` stands in for one not built yet.
    pub fn draw(&self, faces: &[TexId], fallback: Option<TexId>, now: Millis, out: &mut Vec<Draw>) {
        let open = ease(self.openness(now));
        let size = DISC_PX as f32;
        let (cx, cy) = (OUT_W as f32 / 2.0, rest_y(size) + size / 2.0);
        let mut order: Vec<usize> = (0..self.discs.len()).collect();
        order.sort_by(|a, b| {
            let away = |i: &usize| (*i as f32 - self.shown).abs();
            away(b).total_cmp(&away(a))
        });
        for i in order {
            let Some(tex) = faces.get(i).copied().or(fallback) else {
                continue;
            };
            let rel = i as f32 - self.shown;
            let behind = rel.abs().min(1.5);
            let s = size * (1.0 - FAN_SHRINK * behind * open);
            let x = cx + rel * FAN_X * open - s / 2.0;
            let y = cy - behind * FAN_RISE * open - s / 2.0;
            out.push(Draw::Turned {
                x,
                y,
                w: s,
                h: s,
                tex,
                alpha: 1.0,
                turn: (rel * FAN_LEAN + TILT) * open,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picker() -> DiscPicker {
        DiscPicker::open("FF7".into(), vec!["1".into(), "2".into(), "3".into()], 0, false, 0)
    }

    #[test]
    fn left_and_right_stop_at_the_first_and_last_disc() {
        let mut p = picker();
        assert!(!p.step(-1));
        assert!(p.step(1) && p.step(1));
        assert!(!p.step(1));
        assert_eq!(p.selected(), 2);
    }

    #[test]
    fn the_chosen_disc_comes_to_the_front() {
        let mut p = picker();
        p.step(1);
        for _ in 0..60 {
            p.update(1.0 / 60.0);
        }
        assert!(!p.moving(OPEN_MS), "still moving after a second");
        let mut out = Vec::new();
        let faces = [TexId::from_raw(1), TexId::from_raw(2), TexId::from_raw(3)];
        p.draw(&faces, None, OPEN_MS, &mut out);
        assert!(matches!(out.last(), Some(Draw::Turned { tex, .. }) if *tex == faces[1]), "disc 2 is not in front");
    }
}
