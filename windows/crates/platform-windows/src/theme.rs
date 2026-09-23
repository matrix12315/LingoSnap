//! Shared Selection Translate UI tokens.
//!
//! One cool-dark utility palette for the result popup and Manager.
//! COLORREF is `0x00BBGGRR`. Source of truth: `D:\Temp\selection-translate-redesign`.

#![allow(dead_code)]

#[cfg(windows)]
use windows::Win32::Foundation::COLORREF;

/// `#0B0E13` — app chrome, editor bodies, list/scroll wells.
#[cfg(windows)]
pub const VOID: COLORREF = COLORREF(0x0013_0E0B);
/// `#12171F` — window title bars, tab bars, popup chrome.
#[cfg(windows)]
pub const SURFACE: COLORREF = COLORREF(0x001F_1712);
/// `#1A2230` — cards, inputs, Selection/Result panels, menus.
#[cfg(windows)]
pub const RAISED: COLORREF = COLORREF(0x0030_221A);
/// `#2C3646` — 1px borders, dividers, scrollbar thumb.
#[cfg(windows)]
pub const LINE: COLORREF = COLORREF(0x0046_362C);
/// `#3D4A5E` — scrollbar thumb hover.
#[cfg(windows)]
pub const LINE_HOVER: COLORREF = COLORREF(0x005E_4A3D);
/// `#F0F4FA` — primary text.
#[cfg(windows)]
pub const INK: COLORREF = COLORREF(0x00FA_F4F0);
/// `#8B97A8` — labels, captions, context, hints.
#[cfg(windows)]
pub const MUTED: COLORREF = COLORREF(0x00A8_978B);
/// `#8BACFF` — active tab/pill, primary buttons, focus, ctx chip.
#[cfg(windows)]
pub const ACCENT: COLORREF = COLORREF(0x00FF_AC8B);
/// `#D4B56A` — IPA / code tokens / rare emphasis.
#[cfg(windows)]
pub const GOLD: COLORREF = COLORREF(0x006A_B5D4);
/// `#4FD1A5` — resident live, key present.
#[cfg(windows)]
pub const OK: COLORREF = COLORREF(0x00A5_D14F);
/// `#F07178` — delete key, delete selected, errors.
#[cfg(windows)]
pub const ERR: COLORREF = COLORREF(0x0078_71F0);
/// Text on filled accent buttons/pills (`#0B0E13`).
#[cfg(windows)]
pub const ON_ACCENT: COLORREF = VOID;

/// Primary UI face (Segoe UI Variable Text when present).
pub const UI_FONT: &str = "Segoe UI";
/// Mono face for target, IDs, code, IPA.
pub const MONO_FONT: &str = "Cascadia Code";
/// CJK face for context and translation body.
pub const CJK_FONT: &str = "Microsoft YaHei UI";

/// Caption style (SELECTION, RESULT, group headers).
pub const CAPTION_SIZE_PT: i32 = 10;
/// Body / label size.
pub const BODY_SIZE_PT: i32 = 13;
/// Button label size.
pub const BUTTON_SIZE_PT: i32 = 13;
/// Manager page title size.
pub const TITLE_SIZE_PT: i32 = 26;
/// Mono body size.
pub const MONO_SIZE_PT: i32 = 12;

pub const RADIUS_CARD: i32 = 10;
pub const RADIUS_WELL: i32 = 12;
pub const RADIUS_CONTROL: i32 = 8;
pub const RADIUS_POPUP: i32 = 12;

/// `#0B0E13` as RGB triple for non-COLORREF call sites.
pub const VOID_RGB: (u8, u8, u8) = (0x0B, 0x0E, 0x13);
pub const SURFACE_RGB: (u8, u8, u8) = (0x12, 0x17, 0x1F);
pub const RAISED_RGB: (u8, u8, u8) = (0x1A, 0x22, 0x30);
pub const LINE_RGB: (u8, u8, u8) = (0x2C, 0x36, 0x46);
pub const INK_RGB: (u8, u8, u8) = (0xF0, 0xF4, 0xFA);
pub const MUTED_RGB: (u8, u8, u8) = (0x8B, 0x97, 0xA8);
pub const ACCENT_RGB: (u8, u8, u8) = (0x8B, 0xAC, 0xFF);
pub const GOLD_RGB: (u8, u8, u8) = (0xD4, 0xB5, 0x6A);
pub const OK_RGB: (u8, u8, u8) = (0x4F, 0xD1, 0xA5);
pub const ERR_RGB: (u8, u8, u8) = (0xF0, 0x71, 0x78);

/// Build COLORREF from RGB (for tests and dynamic fills).
#[cfg(windows)]
pub const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF((b as u32) << 16 | (g as u32) << 8 | r as u32)
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    use super::*;

    #[cfg(windows)]
    #[test]
    fn tokens_match_design_table() {
        assert_eq!(VOID, rgb(0x0B, 0x0E, 0x13));
        assert_eq!(SURFACE, rgb(0x12, 0x17, 0x1F));
        assert_eq!(RAISED, rgb(0x1A, 0x22, 0x30));
        assert_eq!(LINE, rgb(0x2C, 0x36, 0x46));
        assert_eq!(INK, rgb(0xF0, 0xF4, 0xFA));
        assert_eq!(MUTED, rgb(0x8B, 0x97, 0xA8));
        assert_eq!(ACCENT, rgb(0x8B, 0xAC, 0xFF));
        assert_eq!(GOLD, rgb(0xD4, 0xB5, 0x6A));
        assert_eq!(OK, rgb(0x4F, 0xD1, 0xA5));
        assert_eq!(ERR, rgb(0xF0, 0x71, 0x78));
        assert_eq!(ON_ACCENT, VOID);
    }

    #[test]
    fn rgb_helpers_match_constants() {
        assert_eq!(VOID_RGB, (0x0B, 0x0E, 0x13));
        assert_eq!(ACCENT_RGB, (0x8B, 0xAC, 0xFF));
    }
}
