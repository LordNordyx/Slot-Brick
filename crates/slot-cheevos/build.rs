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
    let mut files = vec![Path::new("shim/slot_cheevos.c").to_path_buf()];
    sources(Path::new("rcheevos/src"), &mut files);
    files.sort();
    let mut build = cc::Build::new();
    build
        .files(&files)
        .include("rcheevos/include")
        .include("rcheevos/src")
        .include("libretro")
        .define("RC_STATIC", None)
        .define("RC_CLIENT_SUPPORTS_HASH", None)
        // GB, GBC, GBA, NES and SNES hash the ROM file itself: no discs, archives or encryption.
        .define("RC_HASH_NO_DISC", None)
        .define("RC_HASH_NO_ENCRYPTED", None)
        .define("RC_HASH_NO_ZIP", None)
        .flag_if_supported("-std=gnu99")
        .warnings(false);
    build.compile("slot_cheevos");
    println!("cargo:rerun-if-changed=shim");
    println!("cargo:rerun-if-changed=rcheevos");
    println!("cargo:rerun-if-changed=libretro");
}
