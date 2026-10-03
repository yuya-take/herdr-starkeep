//! Pixel sprites as rows of palette keys. `.` is transparent.

use crate::render::{Pal, Rgb};

pub const KNIGHT: [&str; 16] = [
    "....hhhh....",
    "...hHHHHh...",
    "..hHHHHHHh..",
    ".hHHsSSsHHh.",
    ".hHSESSESHh.", // E = eyes; closed (s) while meditating
    ".hHSSSSSSHh.",
    ".hHHSssSHHh.",
    "..hHHHHHHh..",
    ".hHHTTTTHHh.",
    "hHHHTCCTHHHh", // C = crystal
    "hHHHTTTTHHHh",
    "hHHHTTTTHHHh",
    "hHHHBBBBHHHh",
    "hHHHTTTTHHHh",
    "hHHHTTTTHHHh",
    "hhHHTTTTHHhh",
];

pub const KNIGHT_MINI: [&str; 9] = [
    "..hhh..",
    ".hHHHh.",
    "hHSSSHh",
    "hHESEHh",
    ".hSsSh.",
    "hHTCTHh",
    "hHTTTHh",
    "hHBBBHh",
    "hHTTTHh",
];

pub const APPRENTICE: [&str; 12] = [
    "...hhh...",
    "..hhhhh..",
    ".hBBCBBh.",
    ".hSSSSSh.",
    ".hSESESh.",
    "..SSsSS..",
    "..TTTTT..",
    ".TTTTTTT.",
    ".TTTTTTT.",
    ".TTbbbTT.",
    "..TT.TT..",
    "..KK.KK..",
];

/// Second walking frame for the last two rows of `APPRENTICE`.
pub const APPRENTICE_STRIDE: [&str; 2] = [".TT...TT.", ".KK...KK."];

pub const APPRENTICE_SEATED: [&str; 9] = [
    "...hhh...",
    "..hhhhh..",
    ".hBBCBBh.",
    ".hSSSSSh.",
    ".hSESESh.",
    "..SSsSS..",
    "..TTTTT..",
    ".TTTTTTT.",
    "TTTTTTTTT",
];

pub const APPRENTICE_MINI: [&str; 6] = [".hh.", "hCCh", "SEES", ".SS.", "TTTT", "T..T"];

pub const CRYSTALS: [Rgb; 15] = [
    Rgb::hex(0x5fd7ff),
    Rgb::hex(0x7dff9a),
    Rgb::hex(0xc39bff),
    Rgb::hex(0xff8fb8),
    Rgb::hex(0x7fb2ff),
    Rgb::hex(0x5ff0d0),
    Rgb::hex(0xe9f27a),
    Rgb::hex(0xffb38a),
    Rgb::hex(0xe0e6ff),
    Rgb::hex(0xff9de2),
    Rgb::hex(0x9be7ff),
    Rgb::hex(0xb6ff7a),
    Rgb::hex(0xd6a8ff),
    Rgb::hex(0xffd29a),
    Rgb::hex(0x8af0b8),
];
pub const SKINS: [Rgb; 4] = [
    Rgb::hex(0xe8b892),
    Rgb::hex(0xc98e62),
    Rgb::hex(0x8d5a3b),
    Rgb::hex(0xf2cfae),
];
pub const HAIRS: [Rgb; 5] = [
    Rgb::hex(0x3a2a22),
    Rgb::hex(0x1f1a1a),
    Rgb::hex(0xb8743c),
    Rgb::hex(0xd9c27a),
    Rgb::hex(0x6b3f2a),
];

pub const EYE: Rgb = Rgb::hex(0x141826);
pub const WHITE: Rgb = Rgb::hex(0xffffff);
pub const BLACK: Rgb = Rgb::hex(0x000000);

/// Knight palette. Shared by the large and mini sprites so colours match.
pub fn knight_pal(crystal: Rgb, skin: Rgb, flash_on: bool) -> Pal {
    Pal::of(&[
        ('h', Rgb::hex(0x2a2d5a)),
        ('H', Rgb::hex(0x4a4f8c)),
        ('T', Rgb::hex(0xc9cbd8)),
        ('B', Rgb::hex(0x5b4a3a)),
        ('C', if flash_on { WHITE } else { crystal }),
        ('S', skin),
        ('s', skin.shade()),
        ('E', EYE),
    ])
}

pub fn apprentice_pal(crystal: Rgb, skin: Rgb, hair: Rgb) -> Pal {
    Pal::of(&[
        ('h', hair),
        ('B', Rgb::hex(0xd9dbe6)),
        ('C', crystal),
        ('S', skin),
        ('s', skin.shade()),
        ('E', EYE),
        ('T', Rgb::hex(0x4d8a7e)),
        ('b', Rgb::hex(0x3a2f28)),
        ('K', Rgb::hex(0x2a2230)),
    ])
}
