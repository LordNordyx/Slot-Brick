use std::path::Path;

use slot_store::gb::Class;
use slot_store::{Cart, Platform, ShellFinish};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Finish {
    Solid,
    Translucent,
    Glitter,
    /// Polished metal, as the gold NES carts were moulded. Not among FINISHES: no cart can be
    /// set to it, it only comes with the plastics that really had it.
    Sheen,
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
/// The gold the two Zelda carts were moulded in: a polished metal, not a yellow plastic.
pub const NES_GOLD_SHELL: Shell = Shell { colour: [0xd0, 0xa4, 0x2c], finish: Finish::Sheen };

/// The SNES cart's lighter, cooler grey.
pub const SNES_SHELL: Shell = shell([0x95, 0x94, 0x9a], Finish::Solid);

/// The PAL and Super Famicom cart's lighter, warmer grey.
pub const SNES_PAL_SHELL: Shell = shell([0xab, 0xab, 0xa7], Finish::Solid);

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
    match cart.platform {
        // NES and SNES carts came in their system's grey: the plastic follows the shape. Famicom
        // carts came in many colours, so theirs can be chosen.
        Platform::Nes if crate::cart::nes_famicom_of(cart) => match cart.shell {
            Some(choice) => shell(choice.colour, Finish::Solid),
            None => famicom_shell(&cart.stem),
        },
        Platform::Nes => nes_plain_shell(&cart.stem),
        Platform::Snes => match crate::cart::snes_shell_of(cart) {
            Some(crate::SnesShell::Pal) => SNES_PAL_SHELL,
            _ => SNES_SHELL,
        },
        Platform::Gba | Platform::Gb | Platform::Gbc => match cart.shell {
            Some(choice) => {
                let finish = match choice.finish {
                    ShellFinish::Solid => Finish::Solid,
                    ShellFinish::Clear => Finish::Translucent,
                    ShellFinish::Glitter => Finish::Glitter,
                };
                shell(choice.colour, finish)
            }
            None if cart.platform == Platform::Gba => gba_shell_for(&cart.code),
            None => gb_shell_for(&cart.rom),
        },
    }
}

/// A NES cart's own plastic: grey, but gold for the two that really were. The NES library's
/// only gold carts are the Zeldas, so the name alone tells them apart.
pub fn nes_plain_shell(stem: &str) -> Shell {
    match stem.to_lowercase().contains("zelda") {
        true => NES_GOLD_SHELL,
        false => NES_SHELL,
    }
}

/// A Famicom plastic for a ROM named as a Japanese release, NES grey for the rest.
pub fn nes_shell_for(stem: &str) -> Shell {
    if !japanese(stem) {
        return NES_SHELL;
    }
    famicom_shell(stem)
}

/// One of the Famicom plastics, picked steadily from the ROM's name.
pub fn famicom_shell(stem: &str) -> Shell {
    // FNV-1a: the same name always gets the same plastic.
    let hash = stem
        .bytes()
        .fold(0x811c_9dc5u32, |h, b| (h ^ b as u32).wrapping_mul(0x0100_0193));
    FAMICOM_SHELLS[hash as usize % FAMICOM_SHELLS.len()]
}

/// A ROM named as a European or Japanese release and not a US one: it came on the rounder
/// PAL / Super Famicom cart. No-Intro "(Europe)", "(Japan)", GoodTools "(E)", "(J)".
pub fn snes_pal(stem: &str) -> bool {
    const PAL: &[&str] = &[
        "Japan", "Europe", "Germany", "France", "Spain", "Italy", "UK", "Australia",
        "Netherlands", "Sweden", "Scandinavia", "J", "E", "G", "F", "S", "I", "A",
    ];
    let mut pal = false;
    for region in region_tags(stem) {
        if PAL.contains(&region) {
            pal = true;
        } else if matches!(region, "USA" | "Canada" | "Brazil" | "World")
            // GoodTools codes with a US release: "(U)", "(JU)", "(UE)".
            || region.len() <= 3 && region.contains('U') && region.bytes().all(|b| b.is_ascii_uppercase())
        {
            return false;
        }
    }
    pal
}

fn region_tags(stem: &str) -> impl Iterator<Item = &str> {
    stem.split(['(', '['])
        .skip(1)
        .filter_map(|t| t.split([')', ']']).next())
        .flat_map(|tag| tag.split(',').map(str::trim))
}

/// A ROM named as a Japanese release: No-Intro "(Japan)" or "(Japan, USA)", GoodTools "(J)".
pub fn japanese(stem: &str) -> bool {
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
        ("Super Famicom grey", SNES_PAL_SHELL),
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
