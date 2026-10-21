use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use slot_cheevos::{Cheevos, Console, Http, HttpRequest, Notice};

/// Answers like retroachievements.org would, by the request's `r=`.
#[derive(Default)]
struct Fake {
    seen: Mutex<Vec<String>>,
}

impl Http for Fake {
    fn send(&self, request: &HttpRequest) -> Result<(u16, Vec<u8>), String> {
        let post = request.post.clone().unwrap_or_default();
        self.seen.lock().unwrap().push(post.clone());
        let r = post.split('&').find_map(|kv| kv.strip_prefix("r=")).unwrap_or("");
        let body = match r {
            "login2" => r#"{"Success":true,"User":"Benni","Token":"t0k3n","Score":10,"SoftcoreScore":2,"Messages":0,"Permissions":1,"AccountType":"Registered"}"#,
            _ => r#"{"Success":false,"Error":"Unknown game","Code":"not_found","Status":404}"#,
        };
        Ok((if r == "login2" { 200 } else { 404 }, body.as_bytes().to_vec()))
    }
}

fn wait(c: &Cheevos, mut want: impl FnMut(&Notice) -> bool) -> Notice {
    let start = Instant::now();
    let mut seen = Vec::new();
    while start.elapsed() < Duration::from_secs(10) {
        for n in c.take() {
            if want(&n) {
                return n;
            }
            seen.push(n);
        }
        c.idle();
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("no such notice; saw {seen:?}");
}

#[test]
fn rcheevos_is_the_vendored_release() {
    assert_eq!(slot_cheevos::version(), "12.5");
}

#[test]
fn a_token_logs_in_and_a_rom_is_identified_by_its_hash() {
    let fake = Arc::new(Fake::default());
    let c = Cheevos::new(fake.clone()).unwrap();
    c.login("Benni", "t0k3n");
    assert_eq!(wait(&c, |n| matches!(n, Notice::LoggedIn(_))), Notice::LoggedIn(Ok(())));
    assert!(fake.seen.lock().unwrap()[0].contains("u=Benni") && fake.seen.lock().unwrap()[0].contains("t=t0k3n"));

    let d = tempfile::tempdir().unwrap();
    let rom = d.path().join("game.gba");
    std::fs::write(&rom, (0..=255u8).cycle().take(1024).collect::<Vec<_>>()).unwrap();
    c.load_game(&rom, Console::Gba);
    assert!(matches!(wait(&c, |n| matches!(n, Notice::GameLoaded(_))), Notice::GameLoaded(Err(_))));
    let seen = fake.seen.lock().unwrap();
    assert!(seen.iter().any(|p| p.contains("b2ea9f7fcea831a4a63b213f41a8855b")), "{seen:?}");
}

#[test]
fn a_nes_rom_is_hashed_without_its_ines_header() {
    let fake = Arc::new(Fake::default());
    let c = Cheevos::new(fake.clone()).unwrap();
    c.login("Benni", "t0k3n");
    wait(&c, |n| matches!(n, Notice::LoggedIn(_)));
    let d = tempfile::tempdir().unwrap();
    let rom = d.path().join("game.nes");
    let mut bytes = b"NES\x1a\x01\x01\0\0\0\0\0\0\0\0\0\0".to_vec();
    bytes.extend((0..=255u8).cycle().take(1024));
    std::fs::write(&rom, bytes).unwrap();
    c.load_game(&rom, Console::Nes);
    wait(&c, |n| matches!(n, Notice::GameLoaded(_)));
    let seen = fake.seen.lock().unwrap();
    // The same 1024 bytes as the GBA ROM above: the 16-byte header is not hashed.
    assert!(seen.iter().any(|p| p.contains("b2ea9f7fcea831a4a63b213f41a8855b")), "{seen:?}");
}

/// A PlayStation disc as a 2048-byte-sector ISO 9660 image: SYSTEM.CNF booting PSX.EXE.
fn ps1_iso(exe: &[u8]) -> Vec<u8> {
    const SECTOR: usize = 2048;
    let both32 = |v: u32| [v.to_le_bytes(), v.to_be_bytes()].concat();
    let record = |name: &[u8], sector: u32, size: u32, dir: bool| {
        let mut body = [both32(sector), both32(size)].concat();
        body.extend([0u8; 7]);
        body.extend([if dir { 2 } else { 0 }, 0, 0, 1, 0, 0, 1, name.len() as u8]);
        body.extend(name);
        if name.len() % 2 == 0 {
            body.push(0);
        }
        [vec![body.len() as u8 + 2, 0], body].concat()
    };
    let cnf = b"BOOT = cdrom:\\PSX.EXE;1\r\n";
    let (root, cnf_at, exe_at) = (20u32, 21u32, 22u32);
    let mut pvd = vec![0u8; SECTOR];
    pvd[0..7].copy_from_slice(b"\x01CD001\x01");
    pvd[80..88].copy_from_slice(&both32(exe_at + exe.len().div_ceil(SECTOR) as u32));
    pvd[128..132].copy_from_slice(&[0, 8, 8, 0]);
    pvd[156..190].copy_from_slice(&record(&[0], root, SECTOR as u32, true));
    let mut end = vec![0u8; SECTOR];
    end[0..7].copy_from_slice(b"\xffCD001\x01");
    let dir = [
        record(&[0], root, SECTOR as u32, true),
        record(&[1], root, SECTOR as u32, true),
        record(b"PSX.EXE;1", exe_at, exe.len() as u32, false),
        record(b"SYSTEM.CNF;1", cnf_at, cnf.len() as u32, false),
    ]
    .concat();
    // Sectors 16 on: the volume descriptor, the end of them, two empty path tables, the root
    // directory, then the files. Each starts on a sector of its own.
    let mut iso = vec![0u8; 16 * SECTOR];
    for part in [&pvd[..], &end, &[], &[], &dir, cnf, exe] {
        let start = iso.len();
        iso.extend(part);
        iso.resize(start + part.len().div_ceil(SECTOR).max(1) * SECTOR, 0);
    }
    iso
}

#[test]
fn a_playstation_disc_is_identified_by_its_boot_program() {
    let fake = Arc::new(Fake::default());
    let c = Cheevos::new(fake.clone()).unwrap();
    c.login("Benni", "t0k3n");
    wait(&c, |n| matches!(n, Notice::LoggedIn(_)));
    let mut exe = vec![0u8; 2048];
    exe[0..8].copy_from_slice(b"PS-X EXE");
    exe[0x1c..0x20].copy_from_slice(&2048u32.to_le_bytes());
    exe.extend((0..=255u8).cycle().take(2048));
    let d = tempfile::tempdir().unwrap();
    let disc = d.path().join("game.iso");
    std::fs::write(&disc, ps1_iso(&exe)).unwrap();
    c.load_game(&disc, Console::Ps1);
    wait(&c, |n| matches!(n, Notice::GameLoaded(_)));
    let seen = fake.seen.lock().unwrap();
    // md5("PSX.EXE" + the program): the boot program's name, then the program itself.
    assert!(seen.iter().any(|p| p.contains("d254942162e1a7e2ba7a02bb5cc8b1c2")), "{seen:?}");
}

#[test]
fn a_playstation_disc_kept_as_chd_is_identified_as_the_same_disc() {
    // tests/disc.chd: the disc ps1_iso builds, padded to 300 sectors, through MAME's
    // `chdman createcd` (0.251) as a MODE1 track.
    let fake = Arc::new(Fake::default());
    let c = Cheevos::new(fake.clone()).unwrap();
    c.login("Benni", "t0k3n");
    wait(&c, |n| matches!(n, Notice::LoggedIn(_)));
    c.load_game(std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/disc.chd")), Console::Ps1);
    wait(&c, |n| matches!(n, Notice::GameLoaded(_)));
    let seen = fake.seen.lock().unwrap();
    assert!(seen.iter().any(|p| p.contains("d254942162e1a7e2ba7a02bb5cc8b1c2")), "{seen:?}");
}

#[test]
fn core_memory_is_mapped_for_a_nes() {
    let mut c = Cheevos::new(Arc::new(Fake::default())).unwrap();
    let mut ram = vec![0u8; 0x800];
    let mapped = unsafe {
        c.set_memory(std::ptr::null(), 0, Console::Nes, &mut |id| match id {
            2 => Some((ram.as_mut_ptr(), ram.len())),
            _ => None,
        })
    };
    assert!(mapped >= 0x800, "{mapped}");
}

#[test]
fn a_refused_token_says_so() {
    struct No;
    impl Http for No {
        fn send(&self, _: &HttpRequest) -> Result<(u16, Vec<u8>), String> {
            Ok((401, br#"{"Success":false,"Error":"Invalid user/token combination.","Code":"invalid_credentials","Status":401}"#.to_vec()))
        }
    }
    let c = Cheevos::new(Arc::new(No)).unwrap();
    c.login("Benni", "old");
    match wait(&c, |n| matches!(n, Notice::LoggedIn(_))) {
        Notice::LoggedIn(Err(e)) => assert!(e.contains("Invalid"), "{e}"),
        n => panic!("{n:?}"),
    }
}

#[test]
fn core_memory_is_mapped_for_a_game_boy() {
    let c = Cheevos::new(Arc::new(Fake::default()));
    let mut c = c.unwrap();
    let mut wram = vec![0u8; 0x2000];
    let mut hram = vec![0u8; 0x80];
    let mapped = unsafe {
        c.set_memory(std::ptr::null(), 0, Console::Gb, &mut |id| match id {
            2 => Some((wram.as_mut_ptr(), wram.len())),
            _ => None,
        })
    };
    assert!(mapped >= 0x2000, "{mapped}");
    let _ = &mut hram;
}

#[test]
fn dropping_with_a_request_in_flight_does_not_crash() {
    struct Slow;
    impl Http for Slow {
        fn send(&self, _: &HttpRequest) -> Result<(u16, Vec<u8>), String> {
            std::thread::sleep(Duration::from_millis(300));
            Err("offline".into())
        }
    }
    let c = Cheevos::new(Arc::new(Slow)).unwrap();
    c.login("Benni", "t");
    drop(c);
    std::thread::sleep(Duration::from_millis(600));
}
