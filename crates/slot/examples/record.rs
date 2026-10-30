// Records Slot's screen as it plays a script. It draws with HeadlessSurface, which is the
// Mac's only; elsewhere the Brick simulator (brick/sim.sh) takes screenshots instead.
#[cfg(target_os = "macos")]
#[path = "support/record_mac.rs"]
mod mac;

#[cfg(target_os = "macos")]
fn main() {
    mac::main();
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("record needs a Mac");
    std::process::exit(2);
}
