mod common;

use common::tmp_root_with_carts;
use slot::app::{INSERT_S, SEATED_AT};
use slot::audio::{ring_capacity, Ring, Sfx, GBA_HZ};
use slot::session::Session;

#[test]
fn the_contacts_in_the_clip_are_where_the_lead_says() {
    for s in [Sfx::Insert, Sfx::Eject] {
        let v = s.render(48_000);
        let at = v
            .chunks_exact(2)
            .enumerate()
            .max_by_key(|(_, c)| c[0].unsigned_abs())
            .map(|(i, _)| i as f32 / 48_000.0)
            .unwrap();
        assert!(
            (at - s.lead()).abs() < 0.008,
            "{s:?}: the contacts are {:.0} ms into the clip and lead says {:.0} ms",
            at * 1000.0,
            s.lead() * 1000.0
        );
    }
}

#[test]
fn the_insert_starts_inside_the_travel() {
    assert!(
        Sfx::Insert.lead() < SEATED_AT,
        "the clip is longer than the travel it belongs to"
    );
}

#[test]
fn the_clips_are_the_recording_and_nothing_else() {
    for s in [Sfx::Insert, Sfx::Eject] {
        let a = s.render(48_000);
        let b = s.render(48_000);
        assert_eq!(a, b, "{s:?} is not the same twice");
        assert!(
            a.chunks_exact(2).all(|c| c[0] == c[1]),
            "{s:?} is not the mono recording"
        );
    }
}

#[test]
fn the_two_directions_are_different_clips() {
    let ins = Sfx::Insert.render(48_000);
    let ej = Sfx::Eject.render(48_000);
    assert_ne!(ins.len(), ej.len());
    assert!(
        Sfx::Eject.lead() * 3.0 < Sfx::Insert.lead(),
        "the eject leads with {} ms and the insert with {} ms: one is the wrong take",
        Sfx::Eject.lead() * 1000.0,
        Sfx::Insert.lead() * 1000.0
    );
}

#[test]
fn neither_clip_starts_or_ends_on_a_step() {
    for (name, s) in [("insert", Sfx::Insert), ("eject", Sfx::Eject)] {
        let v = s.render(48_000);
        assert!(
            v[..8].iter().all(|x| x.abs() < 400),
            "{name} starts on a step"
        );
        assert!(
            v[v.len() - 200..].iter().all(|x| x.abs() < 400),
            "{name} ends on a step"
        );
    }
}

#[test]
fn a_clip_is_mixed_into_queued_game_audio_rather_than_played_after_it() {
    let r = Ring::new(ring_capacity(48_000));
    let c = Sfx::Insert.render(48_000);
    r.push(&vec![1_000i16; c.len() * 2]);
    let queued = r.queued_frames();
    r.mix(&c);
    assert_eq!(
        r.queued_frames(),
        queued,
        "the clip was appended, so it plays after the game caught up"
    );
    let mut out = vec![0i16; c.len()];
    r.fill(&mut out);
    assert_eq!(out[0], 1_000i16.saturating_add(c[0]));
}

#[test]
fn the_whole_of_a_cart_sound_reaches_the_device() {
    for rate in [48_000, GBA_HZ] {
        for s in [Sfx::Insert, Sfx::Eject] {
            let clip = s.render(rate);
            assert!(
                clip.len() > ring_capacity(rate) * 2,
                "{s:?} now fits the ring at {rate} Hz, so this no longer exercises anything"
            );
            let r = Ring::new(ring_capacity(rate));
            r.reopen(rate);
            r.mix(&clip);
            let mut got: Vec<i16> = Vec::new();
            while got.len() < clip.len() {
                let mut out = vec![0i16; 512 * 2];
                r.fill(&mut out);
                got.extend_from_slice(&out);
            }
            let first = got
                .iter()
                .zip(&clip)
                .position(|(a, b)| a != b)
                .unwrap_or(clip.len());
            assert_eq!(
                first,
                clip.len(),
                "{s:?} at {rate} Hz came apart {:.0} ms in, {:.0} ms short of the {:.0} ms it runs",
                1000.0 * first as f32 / 2.0 / rate as f32,
                1000.0 * (clip.len() - first) as f32 / 2.0 / rate as f32,
                1000.0 * clip.len() as f32 / 2.0 / rate as f32,
            );
        }
    }
}

#[test]
fn the_rest_of_a_clip_is_mixed_into_the_game_it_lands_over() {
    let rate = GBA_HZ;
    let r = Ring::new(ring_capacity(rate));
    r.reopen(rate);
    let clip = Sfx::Insert.render(rate);
    r.mix(&clip);
    let mut got: Vec<i16> = Vec::new();
    while got.len() < clip.len() {
        r.push(&vec![100i16; 512 * 2]);
        let mut out = vec![0i16; 512 * 2];
        r.fill(&mut out);
        got.extend_from_slice(&out);
    }
    let late = ring_capacity(rate) * 2 + 1000;
    assert!(
        got[late..clip.len()]
            .iter()
            .zip(&clip[late..])
            .all(|(a, b)| *a == b.saturating_add(100)),
        "the clip and the game did not add up"
    );
}

#[test]
fn a_clip_plays_with_no_core_running() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut s = Session::boot(d.path().to_path_buf());
    assert!(!s.has_core());
    s.play_sfx(Sfx::Insert);
    assert!(s.audio_queued() > 0, "the clip went nowhere");
}

#[test]
fn each_movement_makes_one_sound() {
    for (action, want) in [
        (slot_input::Action::Insert, Sfx::Insert),
        (slot_input::Action::Eject, Sfx::Eject),
    ] {
        let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
        let mut a = common::boot(d.path());
        if action == slot_input::Action::Eject {
            a.apply(slot_input::Action::Insert);
            for _ in 0..80 {
                a.update(1.0 / 60.0);
                a.take_sfx();
            }
        }
        a.apply(action);
        let mut heard: Vec<Sfx> = a.take_sfx().into_iter().collect();
        for _ in 0..90 {
            a.update(1.0 / 60.0);
            if let Some(s) = a.take_sfx() {
                heard.push(s);
            }
        }
        assert_eq!(heard, vec![want], "{action:?} sounded like {heard:?}");
    }
}

#[test]
fn a_paused_core_does_not_silence_the_insert() {
    let r = Ring::new(ring_capacity(48_000));
    r.reopen(48_000);
    r.set_muted(false);
    r.mix(&Sfx::Insert.render(48_000));
    let mut out = vec![0i16; 4_000];
    r.fill(&mut out);
    assert!(
        out.iter().any(|v| v.abs() > 100),
        "the insert came out silent behind a paused core"
    );
}

#[test]
fn fast_forward_is_still_gated() {
    let r = Ring::new(ring_capacity(48_000));
    r.reopen(48_000);
    r.set_muted(true);
    r.mix(&Sfx::Insert.render(48_000));
    let mut out = vec![0i16; 4_000];
    r.fill(&mut out);
    assert!(out.iter().all(|v| *v == 0), "muting no longer silences");
}

#[test]
fn the_game_waits_for_the_cart_to_finish_landing() {
    let hold = INSERT_S - SEATED_AT;
    let tail = Sfx::Insert.tail();
    assert!(
        hold > tail,
        "the picture arrives {:.0} ms after the cart lands and the sound runs {:.0} ms",
        hold * 1000.0,
        tail * 1000.0
    );
    let beat = hold - tail;
    assert!(
        (0.05..0.30).contains(&beat),
        "{:.0} ms of air between the sound and the game",
        beat * 1000.0
    );
}

fn sfx_peak(setup: impl Fn(&mut Session)) -> u16 {
    let d = tmp_root_with_carts(&["Emerald"]);
    slot_store::write_slot_state(
        d.path(),
        &slot_store::SlotState {
            clock_set: true,
            ..slot_store::SlotState::default()
        },
    )
    .unwrap();
    let mut s = Session::boot(d.path().to_path_buf());
    setup(&mut s);
    s.play_sfx(Sfx::Insert);
    let ring = s.audio_ring();
    let mut out = vec![0i16; ring.queued_frames() * 2];
    ring.fill(&mut out);
    out.iter().map(|v| v.unsigned_abs()).max().unwrap_or(0)
}

#[test]
fn a_cart_sound_is_played_at_the_volume_that_is_set() {
    let loud = sfx_peak(|_| {});
    let quiet = sfx_peak(|s| {
        for _ in 0..8 {
            s.app_mut().apply(slot_input::Action::VolumeDown);
        }
    });
    assert!(loud > 0, "the clip was silent at the volume it booted with");
    assert!(
        quiet * 2 < loud,
        "turning the volume down did not quieten the cart: {quiet} against {loud}"
    );
}

#[test]
fn a_cart_sound_is_silent_when_muted() {
    let muted = sfx_peak(|s| s.app_mut().apply(slot_input::Action::MuteToggle));
    assert_eq!(muted, 0, "the cart was heard through a mute");
}

#[test]
fn a_star_pops_on_with_one_of_five_pops_and_a_lower_softer_one_takes_it_off() {
    let peak = |s: Sfx| s.render(48_000).iter().map(|v| v.unsigned_abs()).max().unwrap_or(0);
    let mut takes = Vec::new();
    for n in 0..slot::audio::POP_TAKES {
        for s in [Sfx::Pop(n), Sfx::Unpop(n)] {
            let v = s.render(48_000);
            assert!(v.chunks_exact(2).all(|c| c[0] == c[1]), "{s:?} is not the same on both sides");
            assert!(v[..8].iter().all(|x| x.abs() < 400), "{s:?} starts on a step");
            assert!(v[v.len() - 200..].iter().all(|x| x.abs() < 400), "{s:?} ends on a step");
            let ms = v.len() as f32 / 2.0 / 48_000.0 * 1000.0;
            assert!((60.0..200.0).contains(&ms), "{s:?} runs {ms:.0} ms: a pop is short");
            assert!((s.tail() * 1000.0 - ms).abs() < 1.0);
        }
        assert!(peak(Sfx::Unpop(n)) < peak(Sfx::Pop(n)), "taking a star off is the quieter pop");
        assert!(Sfx::Unpop(n).render(48_000).len() > Sfx::Pop(n).render(48_000).len(), "and the lower, slower one");
        assert!((5_000..7_000).contains(&peak(Sfx::Pop(n))), "pop {n} is {} loud: the takes are evened out, and soft", peak(Sfx::Pop(n)));
        takes.push(Sfx::Pop(n).render(48_000));
    }
    takes.sort();
    takes.dedup();
    assert_eq!(takes.len(), slot::audio::POP_TAKES, "two takes are the same");
    assert!(!Sfx::Pop(0).render(GBA_HZ).is_empty());
}

#[test]
fn y_on_the_shelf_pops_the_star_on_and_off_never_with_the_same_pop_twice_running() {
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    let mut a = common::boot(d.path());
    let mut last = None;
    for i in 0..20 {
        a.apply(slot_input::Action::GbaDown(slot_input::Btn::Y));
        let n = match (i % 2, a.take_sfx()) {
            (0, Some(Sfx::Pop(n))) | (1, Some(Sfx::Unpop(n))) => n,
            (_, other) => panic!("press {i} sounded like {other:?}"),
        };
        assert!(n < slot::audio::POP_TAKES);
        assert_ne!(Some(n), last, "the same pop twice running");
        last = Some(n);
    }
}

#[test]
fn the_menu_has_four_short_soft_pops_of_a_kind() {
    use slot::audio::MenuSound;
    let all = [MenuSound::Move, MenuSound::Change, MenuSound::Open, MenuSound::Back].map(Sfx::Menu);
    let mut seen = Vec::new();
    for s in all {
        let v = s.render(48_000);
        assert!(v[..8].iter().all(|x| x.abs() < 400), "{s:?} starts on a step");
        assert!(v[v.len() - 100..].iter().all(|x| x.abs() < 400), "{s:?} ends on a step");
        let ms = v.len() as f32 / 2.0 / 48_000.0 * 1000.0;
        assert!(ms < 80.0, "{s:?} runs {ms:.0} ms: menu sounds are short");
        let peak = v.iter().map(|x| x.unsigned_abs()).max().unwrap_or(0);
        assert!((2_500..5_000).contains(&peak), "{s:?} peaks at {peak}: subtle, well under the star's pop");
        seen.push(v);
    }
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), 4, "two of the menu's sounds are the same");
}

#[test]
fn the_cart_pop_and_the_label_land_where_their_lead_says() {
    let mut labels = Vec::new();
    for s in [Sfx::CartOpen, Sfx::CartClose, Sfx::Label(0), Sfx::Label(1), Sfx::Label(2)] {
        let v = s.render(48_000);
        assert!(v[..8].iter().all(|x| x.abs() < 400), "{s:?} starts on a step");
        assert!(v[v.len() - 100..].iter().all(|x| x.abs() < 400), "{s:?} ends on a step");
        let (at, peak) = v
            .chunks_exact(2)
            .enumerate()
            .max_by_key(|(_, c)| c[0].unsigned_abs())
            .map(|(i, c)| (i as f32 / 48_000.0, c[0].unsigned_abs()))
            .unwrap();
        assert!((at - s.lead()).abs() < 0.01, "{s:?} peaks {:.0} ms in, lead says {:.0}", at * 1000.0, s.lead() * 1000.0);
        let most = if matches!(s, Sfx::CartOpen | Sfx::CartClose) { 6_000 } else { 10_000 };
        assert!(peak < most, "{s:?} is too loud: {peak}");
        if let Sfx::Label(_) = s {
            labels.push(v);
        }
    }
    labels.sort();
    labels.dedup();
    assert_eq!(labels.len(), slot::audio::LABEL_TAKES, "two label takes are the same");
    assert!(Sfx::CartClose.render(48_000).len() > Sfx::CartOpen.render(48_000).len(), "closing is the lower, slower pop");
}

/// Ten clips of one kind mixed a few milliseconds apart into `r`, over `game` (a level held
/// under them, 0 for none), and what came out.
fn ten_presses(r: &Ring, clip: &[i16], game: i16) -> Vec<i16> {
    let mut out = Vec::new();
    for _ in 0..10 {
        r.push(&vec![game; 2 * 240]);
        r.mix_over(clip, 1);
        let mut chunk = vec![0i16; 2 * 240];
        r.fill(&mut chunk);
        out.extend_from_slice(&chunk);
    }
    r.push(&vec![game; clip.len()]);
    let mut rest = vec![0i16; clip.len()];
    r.fill(&mut rest);
    out.extend_from_slice(&rest);
    out
}

#[test]
fn a_sound_played_over_one_still_playing_takes_its_place_rather_than_adding_to_it() {
    let pop = Sfx::Pop(0).render(48_000);
    let alone = pop.iter().map(|v| v.unsigned_abs()).max().unwrap_or(0);
    let r = Ring::new(ring_capacity(48_000));
    r.reopen(48_000);
    let peak = ten_presses(&r, &pop, 0).iter().map(|v| v.unsigned_abs()).max().unwrap_or(0);
    assert!(peak <= alone + alone / 10, "overlapping pops reached {peak}, one alone is {alone}");
    assert!(peak >= alone * 9 / 10, "the pops went quiet: {peak}, one alone is {alone}");
}

#[test]
fn replacing_a_clip_leaves_the_game_under_it_alone_even_once_the_ring_has_wrapped() {
    let pop = Sfx::Pop(0).render(48_000);
    let r = Ring::new(ring_capacity(48_000));
    r.reopen(48_000);
    // Round the ring a few times first, so its head is anywhere but the start.
    for _ in 0..7 {
        r.push(&vec![0i16; ring_capacity(48_000)]);
        let mut out = vec![0i16; ring_capacity(48_000)];
        r.fill(&mut out);
    }
    ten_presses(&r, &pop, 1_000);
    // Once the last pop is over, only the game's level is left: nothing of any pop stayed behind.
    r.push(&vec![1_000i16; 4_000]);
    let mut tail = vec![0i16; 4_000];
    r.fill(&mut tail);
    assert!(tail.iter().all(|v| (*v - 1_000).abs() <= 2), "the game came back as {:?}", &tail[..8]);
}

#[test]
fn a_sound_of_another_kind_plays_on_under_the_next() {
    let insert = Sfx::Insert.render(48_000);
    let pop = Sfx::Menu(slot::audio::MenuSound::Move).render(48_000);
    let r = Ring::new(ring_capacity(48_000));
    r.reopen(48_000);
    r.mix_over(&insert, 2);
    r.mix_over(&pop, 1);
    // A chunk at a time, as the device takes it: the clip is longer than the ring.
    let mut out: Vec<i16> = Vec::new();
    while out.len() < insert.len() {
        let mut chunk = vec![0i16; 512 * 2];
        r.fill(&mut chunk);
        out.extend_from_slice(&chunk);
    }

    // Well after the pop is over, the cart's clunk is still there.
    let late = pop.len() + 4_000;
    let heard = out.len().min(insert.len());
    assert!(heard > late + 4_000, "too little came out to tell");
    let wrong = (late..heard).find(|k| out[*k] != insert[*k]);
    assert_eq!(wrong, None, "the menu pop cut the cart off ({} of {heard}, pop {})", wrong.map_or(0, |k| out[k]), pop.len());
}

#[test]
fn the_sounds_volume_turns_every_sound_down_on_top_of_the_volume() {
    let peak = |percent: u8| {
        let d = tmp_root_with_carts(&["Emerald"]);
        let state = slot_store::SlotState { clock_set: true, sound_volume: percent, ..slot_store::SlotState::default() };
        slot_store::write_slot_state(d.path(), &state).unwrap();
        let mut s = Session::boot(d.path().to_path_buf());
        s.play_sfx(Sfx::Insert);
        let ring = s.audio_ring();
        let mut out = vec![0i16; ring.queued_frames() * 2];
        ring.fill(&mut out);
        out.iter().map(|v| v.unsigned_abs()).max().unwrap_or(0)
    };
    let (full, half, none) = (peak(100), peak(50), peak(0));
    assert!(full > 0 && half * 2 < full, "50% is {half} against {full}");
    assert_eq!(none, 0, "0% still made a sound");
}
