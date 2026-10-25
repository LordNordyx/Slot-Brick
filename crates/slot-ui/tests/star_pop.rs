use slot_ui::{pop_in, pop_out, Shelf};
use slot_store::{Cart, Platform};

fn cart(stem: &str) -> Cart {
    Cart {
        platform: Platform::Gba,
        stem: stem.into(),
        rom: format!("{stem}.gba").into(),
        label: None,
        famicom_label: None,
        title: String::new(),
        code: String::new(),
        shell: None,
    }
}

#[test]
fn a_star_pops_on_past_its_size_and_settles() {
    assert!(pop_in(0.0).abs() < 1e-6, "it starts from nothing");
    assert!((pop_in(1.0) - 1.0).abs() < 1e-6, "it ends at its own size");
    let top = (0..=100).map(|i| pop_in(i as f32 / 100.0)).fold(0.0, f32::max);
    assert!((1.1..1.3).contains(&top), "it overshoots to {top}: a pop, slight");
}

#[test]
fn a_star_going_swells_a_little_then_vanishes() {
    assert!((pop_out(0.0) - 1.0).abs() < 1e-6);
    assert!(pop_out(1.0).abs() < 1e-6, "it ends at nothing");
    let top = (0..=100).map(|i| pop_out(i as f32 / 100.0)).fold(0.0, f32::max);
    assert!((1.0..1.2).contains(&top), "it swells to {top}");
}

#[test]
fn the_shelf_plays_the_pop_through_and_then_holds_the_star_still() {
    let mut s = Shelf::new(vec![cart("Alpha"), cart("Beta")]);
    assert_eq!(s.star_scale(0), None);
    s.set_favorite(0, true);
    assert!(s.star_scale(0).is_some_and(|v| v < 0.1), "it starts small");
    s.update(0.15);
    assert!(s.star_scale(0).is_some_and(|v| v > 1.0), "half way, it is past its size");
    s.update(0.2);
    assert_eq!(s.star_scale(0), Some(1.0), "done: at rest");
    assert_eq!(s.star_scale(1), None, "the other cart has no star");
    s.set_favorite(0, false);
    assert!(s.star_scale(0).is_some(), "going, it is still drawn");
    s.update(0.2);
    assert_eq!(s.star_scale(0), None, "gone");
}

#[test]
fn the_star_is_three_golds_lit_from_the_top_left() {
    let (base, light, shade) = ([0xff, 0xc8, 0x3c], [0xff, 0xec, 0x9a], [0xd2, 0x86, 0x1c]);
    let f = slot_ui::star_face(80.0, base, light, shade);
    assert!(f.w > 0 && f.h > 0, "no font");
    let solid = |c: [u8; 3]| f.rgba.chunks_exact(4).filter(|p| p[3] == 255 && p[..3] == c).count();
    let (b, l, s) = (solid(base), solid(light), solid(shade));
    assert!(b > 0 && l > 0 && s > 0, "base {b}, light {l}, shade {s}");
    // Lit pixels sit up and to the left of shaded ones.
    let mean = |c: [u8; 3]| {
        let (mut x, mut y, mut n) = (0.0, 0.0, 0.0);
        for (i, p) in f.rgba.chunks_exact(4).enumerate() {
            if p[3] == 255 && p[..3] == c {
                x += (i as u32 % f.w) as f32;
                y += (i as u32 / f.w) as f32;
                n += 1.0;
            }
        }
        (x / n, y / n)
    };
    let (lit, dark) = (mean(light), mean(shade));
    assert!(lit.0 < dark.0 && lit.1 < dark.1, "lit at {lit:?}, shaded at {dark:?}");
}
