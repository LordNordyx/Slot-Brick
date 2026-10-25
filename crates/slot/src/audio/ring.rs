use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::drc::drc_target;

pub fn ring_capacity(sample_rate: u32) -> usize {
    sample_rate as usize * 8 / 60
}

const MAX_WAIT: Duration = Duration::from_millis(50);

struct Inner {
    buf: Vec<i16>,
    head: usize,
    len: usize,
    pending: Vec<i16>,
    placed: usize,
    pending_at: u64,
    drained: u64,
    /// The clip last mixed in with `mix_over`, its kind, where it started, and how much of it
    /// is in `buf`: a new clip of the same kind takes its place rather than adding to it.
    voice: Vec<i16>,
    voice_kind: Option<u8>,
    voice_at: u64,
    voice_laid: usize,
}

pub struct Ring {
    inner: Mutex<Inner>,
    room: Condvar,
    queued: AtomicUsize,
    capacity: AtomicUsize,
    rate: AtomicU32,
    muted: AtomicBool,
    idle: AtomicBool,
    overruns: AtomicU64,
    underruns: AtomicU64,
    consuming: AtomicBool,
    priming: AtomicBool,
}

impl Ring {
    pub fn new(capacity_frames: usize) -> Self {
        Ring {
            inner: Mutex::new(Inner {
                buf: vec![0; capacity_frames * 2],
                head: 0,
                len: 0,
                pending: Vec::new(),
                placed: 0,
                pending_at: 0,
                drained: 0,
                voice: Vec::new(),
                voice_kind: None,
                voice_at: 0,
                voice_laid: 0,
            }),
            room: Condvar::new(),
            queued: AtomicUsize::new(0),
            idle: AtomicBool::new(false),
            capacity: AtomicUsize::new(capacity_frames),
            rate: AtomicU32::new(0),
            muted: AtomicBool::new(false),
            overruns: AtomicU64::new(0),
            underruns: AtomicU64::new(0),
            consuming: AtomicBool::new(true),
            priming: AtomicBool::new(false),
        }
    }

    pub fn reopen(&self, sample_rate: u32) {
        let frames = ring_capacity(sample_rate);
        let mut i = self.lock();
        i.buf = vec![0; frames * 2];
        i.head = 0;
        i.pending = Vec::new();
        i.placed = 0;
        i.pending_at = 0;
        i.drained = 0;
        i.voice.clear();
        i.voice_kind = None;
        i.voice_laid = 0;
        i.len = drc_target(frames) * 2;
        self.queued.store(i.len, Ordering::Relaxed);
        self.capacity.store(frames, Ordering::Relaxed);
        self.rate.store(sample_rate, Ordering::Relaxed);
        self.overruns.store(0, Ordering::Relaxed);
        self.underruns.store(0, Ordering::Relaxed);
        self.consuming.store(true, Ordering::Relaxed);
        self.priming.store(false, Ordering::Relaxed);
    }

    pub fn push(&self, samples: &[i16]) {
        let mut i = self.lock();
        let lost = write_into(&mut i, samples).len();
        self.queued.store(i.len, Ordering::Relaxed);
        drop(i);
        if lost > 0 && !self.muted() && self.capacity_frames() > 0 {
            self.overruns.fetch_add(lost as u64, Ordering::Relaxed);
        }
    }

    pub fn push_blocking(&self, samples: &[i16]) {
        if self.muted() || self.capacity_frames() == 0 || !self.consuming.load(Ordering::Relaxed) {
            self.push(samples);
            return;
        }
        let deadline = Instant::now() + MAX_WAIT;
        let mut rest = samples;
        let mut i = self.lock();
        loop {
            rest = write_into(&mut i, rest);
            self.queued.store(i.len, Ordering::Relaxed);
            if rest.is_empty() || self.muted() {
                return;
            }
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                self.consuming.store(false, Ordering::Relaxed);
                self.overruns
                    .fetch_add(rest.len() as u64, Ordering::Relaxed);
                return;
            };
            i = self
                .room
                .wait_timeout(i, left)
                .map(|(guard, _)| guard)
                .unwrap_or_else(|e| e.into_inner().0);
        }
    }

    /// Adds a clip (Slot's own sounds) into whatever is queued, the game's sound included.
    pub fn mix(&self, samples: &[i16]) {
        self.mix_voice(samples, None);
    }

    /// As `mix`, but a clip of the same `kind` still playing is taken out first, fading over
    /// VOICE_FADE samples: menu pops in quick succession replace one another rather than
    /// adding up to louder and louder. One of another kind (the cart going in) plays on.
    pub fn mix_over(&self, samples: &[i16], kind: u8) {
        self.mix_voice(samples, Some(kind));
    }

    fn mix_voice(&self, samples: &[i16], kind: Option<u8>) {
        self.priming.store(false, Ordering::Relaxed);
        // Copied before the lock: the audio thread waits on it.
        let voice = kind.map(|_| samples.to_vec()).unwrap_or_default();
        let mut i = self.lock();
        let at = i.placed.min(i.pending.len());
        let (tail, tail_at) = (i.pending.split_off(at), i.pending_at);
        i.pending.clear();
        i.placed = 0;
        if i.buf.is_empty() {
            return;
        }
        let start = i.drained;
        let mut merged = samples.to_vec();
        if kind.is_some() && i.voice_kind == kind {
            // The same kind: it gives way, and what of it had not reached `buf` goes too.
            silence_voice(&mut i);
        } else if let Some(shift) = tail_at.checked_sub(start) {
            // Another kind plays on: the rest of it, not yet in `buf`, goes in with this one.
            let shift = shift as usize;
            if merged.len() < shift + tail.len() {
                merged.resize(shift + tail.len(), 0);
            }
            for (k, v) in tail.into_iter().enumerate() {
                merged[shift + k] = merged[shift + k].saturating_add(v);
            }
        }
        i.pending_at = start;
        i.voice = voice;
        i.voice_kind = kind;
        i.voice_at = start;
        i.voice_laid = 0;
        i.pending = merged;
        lay_pending(&mut i);
        self.queued.store(i.len, Ordering::Relaxed);
    }

    pub fn fill(&self, out: &mut [i16]) {
        let mut i = self.lock();
        lay_pending(&mut i);
        let cap = i.buf.len();
        let holding = self.priming.load(Ordering::Relaxed) && cap > 0 && i.len < cap / 2;
        self.priming.store(holding, Ordering::Relaxed);
        let n = if holding { 0 } else { out.len().min(i.len) };
        if cap > 0 && !holding {
            let first = n.min(cap - i.head);
            out[..first].copy_from_slice(&i.buf[i.head..i.head + first]);
            out[first..n].copy_from_slice(&i.buf[..n - first]);
            i.head = (i.head + n) % cap;
            i.len -= n;
            i.drained += n as u64;
        }
        out[n..].fill(0);
        self.queued.store(i.len, Ordering::Relaxed);
        drop(i);
        self.consuming.store(true, Ordering::Relaxed);
        self.room.notify_all();
        let short = out.len() - n;
        if short > 0 && !self.muted() && !self.idle.load(Ordering::Relaxed) && cap > 0 {
            self.underruns.fetch_add(short as u64, Ordering::Relaxed);
            self.priming.store(true, Ordering::Relaxed);
        }
        if self.muted() {
            out.fill(0);
        }
    }

    pub fn queued_frames(&self) -> usize {
        self.queued.load(Ordering::Relaxed) / 2
    }

    pub fn capacity_frames(&self) -> usize {
        self.capacity.load(Ordering::Relaxed)
    }

    pub fn overruns(&self) -> u64 {
        self.overruns.load(Ordering::Relaxed)
    }

    pub fn underruns(&self) -> u64 {
        self.underruns.load(Ordering::Relaxed)
    }

    pub fn clear_faults(&self) {
        self.overruns.store(0, Ordering::Relaxed);
        self.underruns.store(0, Ordering::Relaxed);
    }

    pub fn sample_rate(&self) -> u32 {
        self.rate.load(Ordering::Relaxed)
    }

    pub fn set_idle(&self, idle: bool) {
        self.idle.store(idle, Ordering::Relaxed);
    }

    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Relaxed);
        self.room.notify_all();
    }

    pub fn muted(&self) -> bool {
        self.muted.load(Ordering::Relaxed)
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// How long a clip cut short by the next takes to fade out (samples, both channels): 2 ms at
/// 48 kHz, enough not to click.
const VOICE_FADE: usize = 192;

/// Takes the part of the last clip not yet played out of `buf`, fading it out from where
/// playback is.
fn silence_voice(i: &mut Inner) {
    let cap = i.buf.len();
    // From where playback is: what has been played is gone already.
    let played = i.drained.saturating_sub(i.voice_at) as usize;
    for k in played..i.voice_laid.min(i.voice.len()) {
        let off = (i.voice_at + k as u64 - i.drained) as usize;
        if off >= i.len || cap == 0 {
            break;
        }
        let keep = 1.0 - (off as f32 / VOICE_FADE as f32).min(1.0);
        let at = (i.head + off) % cap;
        let take = (i.voice[k] as f32 * (1.0 - keep)) as i16;
        i.buf[at] = i.buf[at].saturating_sub(take);
    }
    i.voice.clear();
    i.voice_laid = 0;
}

fn lay_pending(i: &mut Inner) {
    let cap = i.buf.len();
    if cap == 0 || i.placed >= i.pending.len() {
        return;
    }
    let Some(off) = i.pending_at.checked_sub(i.drained) else {
        i.pending.clear();
        i.placed = 0;
        return;
    };
    let off = off as usize;
    if off >= cap {
        return;
    }
    let n = (i.pending.len() - i.placed).min(cap - off);
    let len = i.len;
    for k in 0..n {
        let s = i.pending[i.placed + k];
        let at = (i.head + off + k) % cap;
        i.buf[at] = if off + k < len {
            i.buf[at].saturating_add(s)
        } else {
            s
        };
    }
    i.len = i.len.max(off + n);
    i.placed += n;
    i.voice_laid += n;
    i.pending_at += n as u64;
    if i.placed >= i.pending.len() {
        i.pending = Vec::new();
        i.placed = 0;
    }
}

fn write_into<'a>(i: &mut Inner, samples: &'a [i16]) -> &'a [i16] {
    let cap = i.buf.len();
    if cap == 0 {
        return samples;
    }
    let n = samples.len().min(cap - i.len);
    let start = (i.head + i.len) % cap;
    let first = n.min(cap - start);
    i.buf[start..start + first].copy_from_slice(&samples[..first]);
    i.buf[..n - first].copy_from_slice(&samples[first..n]);
    i.len += n;
    &samples[n..]
}
