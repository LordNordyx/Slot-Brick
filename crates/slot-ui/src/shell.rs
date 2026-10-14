use std::path::Path;

use slot_store::gb::Class;
use slot_store::{Cart, Platform, ShellFinish};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Finish {
    Solid,
    Translucent,
    Glitter,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Shell {
    pub colour: [u8; 3],
    pub finish: Finish,
}

pub const DEFAULT_SHELL: Shell = Shell {
    colour: [0x35, 0x35, 0x3a],
    finish: Finish::Solid,
};

const fn shell(colour: [u8; 3], finish: Finish) -> Shell {
    Shell { colour, finish }
}

const EXACT: &[(&str, Shell)] = &[
    ("AXV", shell([0xc2, 0x33, 0x2e], Finish::Translucent)),
    ("AXP", shell([0x2f, 0x5c, 0xc0], Finish::Translucent)),
    ("BPE", shell([0x24, 0x9c, 0x60], Finish::Translucent)),
    ("BPR", shell([0xd8, 0x52, 0x24], Finish::Solid)),
    ("BPG", shell([0x63, 0xb0, 0x44], Finish::Solid)),
    ("U3I", CLEAR),
    ("U32", CLEAR),
    ("U33", CLEAR),
    ("V49", shell([0xa9, 0x51, 0x3b], Finish::Solid)),
    ("KYGE", YOSHI),
    ("KYGP", YOSHI),
    ("RZW", shell([0x5f, 0x62, 0x64], Finish::Translucent)),
    ("RZWJ", shell([0xec, 0xee, 0xe8], Finish::Solid)),
];

const CLEAR: Shell = shell([0xd9, 0xdb, 0xd8], Finish::Translucent);
const YOSHI: Shell = shell([0x2f, 0x8f, 0x4e], Finish::Solid);

const FAMILY: &[(u8, Shell)] = &[(b'M', shell([0xc6, 0xc6, 0xc9], Finish::Solid))];

pub const DMG_SHELL: Shell = shell([0x9a, 0x97, 0x8f], Finish::Solid);

pub const DUAL_MODE_SHELL: Shell = shell([0x33, 0x30, 0x31], Finish::Solid);

pub const GB_CLEAR_SHELL: Shell = shell([0x7c, 0x7a, 0x8a], Finish::Translucent);

/// Every NES cart's grey.
pub const NES_SHELL: Shell = shell([0x8e, 0x8d, 0x8a], Finish::Solid);

/// The SNES cart's lighter, cooler grey.
pub const SNES_SHELL: Shell = shell([0x95, 0x94, 0x9a], Finish::Solid);

/// Famicom carts came in many plastics. A Japanese release wears one of them, picked
/// steadily from its name: a colour in the spirit of the Famicom, not its real one.
pub const FAMICOM_SHELLS: [Shell; 8] = [
    shell([0xe3, 0xb9, 0x2d], Finish::Solid),
    shell([0xbf, 0x32, 0x2d], Finish::Solid),
    shell([0x2f, 0x5a, 0xab], Finish::Solid),
    shell([0x3b, 0x8c, 0x4c], Finish::Solid),
    shell([0x2b, 0x2a, 0x2e], Finish::Solid),
    shell([0xe6, 0xe2, 0xd6], Finish::Solid),
    shell([0xdc, 0x78, 0x2b], Finish::Solid),
    shell([0xe2, 0x8c, 0xa9], Finish::Solid),
];

const GB_CODES: &[(&str, Shell)] = &[
    ("AAU", shell([0xb3, 0x8b, 0x3a], Finish::Solid)),
    ("AAUJ", DUAL_MODE_SHELL),
    ("AAX", shell([0xa9, 0xaa, 0xa7], Finish::Solid)),
    ("AAXJ", DUAL_MODE_SHELL),
    ("BYT", CRYSTAL),
    ("BXT", CRYSTAL),
    ("KTN", KIRBY),
    ("KKK", KIRBY),
    ("KCE", shell([0x1f, 0x9f, 0xb6], Finish::Translucent)),
    ("VCA", shell([0xf0, 0xa9, 0x5e], Finish::Translucent)),
    ("VPHJ", shell([0xe2, 0xb4, 0x13], Finish::Solid)),
    ("BMG", DUAL_MODE_SHELL),
];

const GB_OVERSEAS_TITLES: &[(&str, Shell)] = &[
    ("POKEMON RED", shell([0xc0, 0x28, 0x2c], Finish::Solid)),
    ("POKEMON BLU", shell([0x2b, 0x3a, 0x88], Finish::Solid)),
    ("POKEMON YEL", shell([0xe9, 0xa8, 0x26], Finish::Solid)),
];

const CRYSTAL: Shell = shell([0x86, 0xb9, 0xbf], Finish::Glitter);
const KIRBY: Shell = shell([0xec, 0x94, 0xb4], Finish::Translucent);

pub fn shell_for(cart: &Cart) -> Shell {
    if let Some(choice) = cart.shell {
        let finish = match choice.finish {
            ShellFinish::Solid => Finish::Solid,
            ShellFinish::Clear => Finish::Translucent,
            ShellFinish::Glitter => Finish::Glitter,
        };
        return shell(choice.colour, finish);
    }
    match cart.platform {
        Platform::Gba => gba_shell_for(&cart.code),
        Platform::Gb | Platform::Gbc => gb_shell_for(&cart.rom),
        Platform::Nes => nes_shell_for(&cart.stem),
        Platform::Snes => SNES_SHELL,
    }
}

/// A Famicom plastic for a ROM named as a Japanese release, NES grey for the rest.
pub fn nes_shell_for(stem: &str) -> Shell {
    if !japanese(stem) {
        return NES_SHELL;
    }
    // FNV-1a: the same name always gets the same plastic.
    let hash = stem
        .bytes()
        .fold(0x811c_9dc5u32, |h, b| (h ^ b as u32).wrapping_mul(0x0100_0193));
    FAMICOM_SHELLS[hash as usize % FAMICOM_SHELLS.len()]
}

/// No-Intro "(Japan)" or "(Japan, USA)", GoodTools "(J)".
fn japanese(stem: &str) -> bool {
    stem.split(['(', '['])
        .skip(1)
        .filter_map(|t| t.split([')', ']']).next())
        .any(|tag| tag == "J" || tag.split(',').any(|r| r.trim() == "Japan"))
}

pub fn gba_shell_for(code: &str) -> Shell {
    lookup(code, EXACT, FAMILY)
}

fn gb_shell_for(rom: &Path) -> Shell {
    let Some(h) = slot_store::gb::header(rom) else {
        return flag_shell(slot_store::gb::class(rom));
    };
    let by_title = || {
        GB_OVERSEAS_TITLES
            .iter()
            .find(|(t, _)| !h.japan && *t == h.title)
            .map(|(_, s)| *s)
    };
    by_code(&h.code, GB_CODES)
        .or_else(by_title)
        .unwrap_or_else(|| flag_shell(h.class()))
}

fn flag_shell(class: Class) -> Shell {
    match class {
        Class::Original => DMG_SHELL,
        Class::DualMode => DUAL_MODE_SHELL,
        Class::ColourOnly => GB_CLEAR_SHELL,
    }
}

pub fn table_keys() -> Vec<&'static str> {
    EXACT.iter().map(|(k, _)| *k).collect()
}

pub fn gb_table_shells() -> Vec<Shell> {
    GB_CODES
        .iter()
        .chain(GB_OVERSEAS_TITLES)
        .map(|(_, s)| *s)
        .collect()
}

pub fn lookup_order_is_exact_then_family_then_default() -> bool {
    const A: Shell = shell([1, 1, 1], Finish::Solid);
    const B: Shell = shell([2, 2, 2], Finish::Solid);
    let exact = [("MSK", A)];
    let family = [(b'M', B)];
    lookup("MSKE", &exact, &family) == A
        && lookup("MPOE", &exact, &family) == B
        && lookup("ZZZZ", &exact, &family) == DEFAULT_SHELL
}

fn lookup(code: &str, exact: &[(&str, Shell)], family: &[(u8, Shell)]) -> Shell {
    if let Some(s) = by_code(code, exact) {
        return s;
    }
    if let Some(first) = code.as_bytes().first() {
        if let Some((_, s)) = family.iter().find(|(k, _)| k == first) {
            return *s;
        }
    }
    DEFAULT_SHELL
}

fn by_code(code: &str, table: &[(&str, Shell)]) -> Option<Shell> {
    let whole: String = code.chars().take(4).collect();
    let prefix: String = code.chars().take(3).collect();
    [whole, prefix]
        .iter()
        .find_map(|key| table.iter().find(|(k, _)| k == key).map(|(_, s)| *s))
}

pub fn shell_presets() -> Vec<(&'static str, Shell)> {
    const GBA: &[(&str, &str)] = &[
        ("Pokémon Ruby", "AXV"),
        ("Pokémon Sapphire", "AXP"),
        ("Pokémon Emerald", "BPE"),
        ("Pokémon FireRed", "BPR"),
        ("Pokémon LeafGreen", "BPG"),
        ("Boktai clear", "U3I"),
        ("Drill Dozer red", "V49"),
        ("Yoshi green", "KYGE"),
        ("WarioWare smoke", "RZW"),
        ("Made in Wario white", "RZWJ"),
    ];
    const GB: &[(&str, &str)] = &[
        ("Pokémon Gold", "AAU"),
        ("Pokémon Silver", "AAX"),
        ("Pokémon Crystal", "BYT"),
        ("Kirby pink", "KTN"),
        ("Command Master turquoise", "KCE"),
        ("Chee-Chai Alien orange", "VCA"),
        ("Pinball yellow", "VPHJ"),
    ];
    let mut out = vec![
        ("Game Boy grey", DMG_SHELL),
        ("Game Boy black", DUAL_MODE_SHELL),
        ("Game Boy Color clear", GB_CLEAR_SHELL),
        ("Advance charcoal", DEFAULT_SHELL),
        ("NES grey", NES_SHELL),
        ("SNES grey", SNES_SHELL),
        ("Advance Video grey", FAMILY[0].1),
    ];
    out.extend(
        GBA.iter()
            .filter_map(|(n, k)| Some((*n, by_code(k, EXACT)?))),
    );
    out.extend(
        GB.iter()
            .filter_map(|(n, k)| Some((*n, by_code(k, GB_CODES)?))),
    );
    out.extend(
        [
            ("Pokémon Red", "POKEMON RED"),
            ("Pokémon Blue", "POKEMON BLU"),
            ("Pokémon Yellow", "POKEMON YEL"),
        ]
        .iter()
        .filter_map(|(n, t)| {
            let s = GB_OVERSEAS_TITLES.iter().find(|(k, _)| k == t)?.1;
            Some((*n, s))
        }),
    );
    out
}
