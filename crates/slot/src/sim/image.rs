//! Screenshots as PNG, and comparing one against its golden.
use std::path::Path;

pub fn encode(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, w, h);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let written = encoder
            .write_header()
            .and_then(|mut writer| writer.write_image_data(rgba));
        if let Err(e) = written {
            eprintln!("slot: sim: png: {e}");
        }
    }
    out
}

/// An RGBA PNG's size and pixels.
pub fn decode(path: &Path) -> Result<(u32, u32, Vec<u8>), String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut decoder = png::Decoder::new(file);
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return Err(format!("{}: not 8-bit RGBA", path.display()));
    }
    buf.truncate(info.buffer_size());
    Ok((info.width, info.height, buf))
}

/// How far a screenshot is from its golden.
pub struct Difference {
    /// Pixels whose colour moved more than `TOLERANCE` in any channel.
    pub pixels: usize,
    pub total: usize,
    /// The screenshot with every differing pixel painted magenta, to see where.
    pub marked: Vec<u8>,
}

/// Software rendering is exact from run to run, but not across Mesa versions: allow a little
/// rounding per channel.
pub const TOLERANCE: u8 = 8;

pub fn compare(got: &[u8], want: &[u8]) -> Difference {
    let mut marked = got.to_vec();
    let mut pixels = 0;
    for (i, (g, w)) in got.chunks_exact(4).zip(want.chunks_exact(4)).enumerate() {
        if g.iter().zip(w).any(|(a, b)| a.abs_diff(*b) > TOLERANCE) {
            pixels += 1;
            marked[i * 4..i * 4 + 4].copy_from_slice(&[255, 0, 255, 255]);
        }
    }
    Difference {
        pixels,
        total: got.len() / 4,
        marked,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_round_trip() {
        let rgba: Vec<u8> = (0..4 * 3 * 2).map(|v| v as u8).collect();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.png");
        std::fs::write(&path, encode(3, 2, &rgba)).unwrap();
        assert_eq!(decode(&path).unwrap(), (3, 2, rgba));
    }

    #[test]
    fn small_rounding_is_not_a_difference() {
        let a = [10, 10, 10, 255, 0, 0, 0, 255];
        let b = [12, 9, 10, 255, 200, 0, 0, 255];
        let d = compare(&a, &b);
        assert_eq!(d.pixels, 1);
        assert_eq!(&d.marked[4..8], &[255, 0, 255, 255]);
    }
}
