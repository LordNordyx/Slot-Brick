//! Builds the vendored rcheevos (see rcheevos/VERSION) and slot's shim over it.
use std::path::Path;

fn sources(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("rcheevos sources").flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "c") {
            out.push(path);
        }
    }
}

fn main() {
    let mut files = vec![Path::new("shim/slot_cheevos.c").to_path_buf(), Path::new("shim/chd_reader.c").to_path_buf()];
    sources(Path::new("rcheevos/src"), &mut files);
    // libchdr and the decompressors it needs (see libchdr/README.slot), for .chd discs.
    sources(Path::new("libchdr/src"), &mut files);
    files.extend(
        ["libchdr/deps/lzma-26.02/src/LzmaDec.c", "libchdr/deps/zstd-1.5.7/zstddeclib.c", "libchdr/deps/miniz-3.1.2/miniz.c"]
            .map(|f| Path::new(f).to_path_buf()),
    );
    files.sort();
    let mut build = cc::Build::new();
    build
        .files(&files)
        .include("rcheevos/include")
        .include("rcheevos/src")
        .include("libretro")
        .include("libchdr/include")
        .include("libchdr/deps/lzma-26.02/include")
        .include("libchdr/deps/zstd-1.5.7")
        .include("libchdr/deps/miniz-3.1.2")
        // libchdr's zlib codec, on miniz.
        .define("USE_MINIZ", None)
        .define("RC_STATIC", None)
        .define("RC_CLIENT_SUPPORTS_HASH", None)
        // The cartridges hash the ROM file itself; a PlayStation disc is read for its program
        // (rhash/cdreader.c, hash_disc.c). No archives or encryption.
        .define("RC_HASH_NO_ENCRYPTED", None)
        .define("RC_HASH_NO_ZIP", None)
        .flag_if_supported("-std=gnu99")
        .warnings(false);
    build.compile("slot_cheevos");
    println!("cargo:rerun-if-changed=shim");
    println!("cargo:rerun-if-changed=rcheevos");
    println!("cargo:rerun-if-changed=libretro");
    println!("cargo:rerun-if-changed=libchdr");
}
