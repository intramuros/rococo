//! The colours of Éclat: candlelit gold and rose against deep navy.

use macroquad::prelude::Color;

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0)
}

pub const fn with_alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, a)
}

pub const INK: Color = rgb(10, 9, 18);
pub const NAVY: Color = rgb(22, 24, 48);

pub const GOLD: Color = rgb(212, 175, 106);
pub const GOLD_LIGHT: Color = rgb(250, 228, 170);
pub const GOLD_DARK: Color = rgb(146, 108, 54);
pub const GOLD_DEEP: Color = rgb(78, 56, 30);

pub const ROSE: Color = rgb(222, 146, 168);
pub const ROSE_DARK: Color = rgb(164, 84, 112);
pub const CRIMSON: Color = rgb(172, 34, 62);
pub const CRIMSON_LIGHT: Color = rgb(232, 92, 112);
pub const CREAM: Color = rgb(244, 232, 216);
pub const PARCHMENT: Color = rgb(233, 219, 192);
pub const LETTER_INK: Color = rgb(44, 30, 34);

pub const SKIN: Color = rgb(242, 212, 196);
pub const HAIR: Color = rgb(234, 228, 238);
pub const HAIR_SHADE: Color = rgb(184, 176, 196);
pub const STOCKING: Color = rgb(238, 234, 240);
pub const STOCKING_SHADE: Color = rgb(170, 160, 182);
pub const SILVER: Color = rgb(226, 230, 238);
pub const IRON: Color = rgb(38, 34, 42);

pub const MARBLE: Color = rgb(70, 56, 72);
pub const MARBLE_DARK: Color = rgb(56, 44, 60);
pub const MARBLE_DEEP: Color = rgb(40, 31, 44);
pub const MARBLE_TOP: Color = rgb(206, 182, 182);
pub const MARBLE_TOP_DARK: Color = rgb(150, 124, 136);

pub const WOOD: Color = rgb(74, 48, 38);
pub const WOOD_LIGHT: Color = rgb(122, 82, 58);
pub const WOOD_DARK: Color = rgb(38, 26, 26);
pub const WOOD_DEEP: Color = rgb(24, 17, 18);
pub const BOOKS: [Color; 6] =
    [rgb(96, 34, 44), rgb(38, 46, 86), rgb(64, 72, 50), rgb(92, 64, 40), rgb(70, 42, 78), rgb(120, 96, 64)];

pub const MANA: Color = rgb(96, 128, 222);
pub const MANA_LIGHT: Color = rgb(170, 196, 255);
pub const CANDLE: Color = rgb(255, 190, 100);
pub const FLAME_CORE: Color = rgb(255, 244, 210);
pub const CRYSTAL: Color = rgb(214, 228, 255);

pub const ARMOR: Color = rgb(196, 152, 70);
pub const ARMOR_DARK: Color = rgb(120, 88, 40);
pub const GOWN: Color = rgb(218, 210, 230);
pub const GOWN_SHADE: Color = rgb(160, 150, 182);
pub const MASK: Color = rgb(246, 242, 248);
pub const MOTH: Color = rgb(206, 190, 226);
