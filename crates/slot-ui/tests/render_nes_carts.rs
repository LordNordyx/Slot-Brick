use slot_store::scan;
use slot_ui::{cart_face, rest_y, CartFace, CART_W, MOUTH_H, OUT_H, PLATE_H};
use tempfile::TempDir;

const COLS: u32 = 3;
const SHEET_W: u32 = COLS * CART_W;

const GROUND: [u8; 3] = [0x14, 0x15, 0x1a];
const PLATE: [u8; 3] = [0x25, 0x27, 0x2e];
const FLOOR: [u8; 3] = [0x3a, 0x3d, 0x46];

fn write_nes_rom(d: &TempDir, name: &str) {
    let games = d.path().join("Games/NES");
    std::fs::create_dir_all(&games).expect("create games dir");
    std::fs::write(games.join(name), vec![0u8; 0x8000]).expect("write rom");
}

fn paste(frame: &mut [u8], face: &CartFace, left: u32) {
    for y in 0..face.h {
        for x in 0..face.w {
            let s = ((y * face.w + x) * 4) as usize;
            let a = face.rgba[s + 3] as u32;
            if a == 0 {
                continue;
            }
            let top = rest_y(face.h as f32) as u32;
            let d = (((y + top) * SHEET_W + x + left) * 3) as usize;
            for c in 0..3 {
                frame[d + c] =
                    ((face.rgba[s + c] as u32 * a + frame[d + c] as u32 * (255 - a) + 127) / 255)
                        as u8;
            }
        }
    }
}

/// The three NES shells side by side: the plain grey cart, a Famicom-board one, and one of
/// Tengen's own unlicensed titles — through the real scan → shape → render pipeline, not a
/// hand-built mockup.
#[test]
fn render_nes_carts() {
    let Ok(out) = std::env::var("SCRATCH_PNG") else {
        return;
    };
    let d = tempfile::tempdir().expect("tempdir");
    write_nes_rom(&d, "Super Mario Bros. (USA).nes");
    write_nes_rom(&d, "Downtown - Nekketsu Monogatari (Japan).nes");
    write_nes_rom(&d, "Indiana Jones and the Temple of Doom (USA).nes");
    let carts = scan(d.path()).expect("scan");

    let mut frame = Vec::with_capacity((SHEET_W * OUT_H * 3) as usize);
    for y in 0..OUT_H {
        for _ in 0..SHEET_W {
            let c = if (y as f32) < PLATE_H { PLATE } else { GROUND };
            frame.extend_from_slice(&c);
        }
    }
    for x in 0..SHEET_W {
        let d = (((OUT_H / 2) * SHEET_W + x) * 3) as usize;
        frame[d..d + 3].copy_from_slice(&FLOOR);
    }
    for x in 0..SHEET_W {
        let d = (((OUT_H - MOUTH_H as u32) * SHEET_W + x) * 3) as usize;
        frame[d..d + 3].copy_from_slice(&FLOOR);
    }

    for (i, cart) in carts.iter().enumerate() {
        let face = cart_face(cart);
        paste(&mut frame, &face, i as u32 * CART_W);
        println!(
            "{} at column {}, standing {}..{}",
            cart.stem,
            i as u32 * CART_W,
            rest_y(face.h as f32),
            rest_y(face.h as f32) + face.h as f32
        );
    }

    let f = std::fs::File::create(&out).expect("create png");
    let mut e = png::Encoder::new(std::io::BufWriter::new(f), SHEET_W, OUT_H);
    e.set_color(png::ColorType::Rgb);
    e.set_depth(png::BitDepth::Eight);
    e.write_header()
        .expect("png header")
        .write_image_data(&frame)
        .expect("png data");
    println!("wrote {out}");
}
