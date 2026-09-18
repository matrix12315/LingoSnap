//! Shared dark-utility UI tokens from DESIGN.md (Raycast/Linear-class + DeepL result).
//! COLORREF is `0x00BBGGRR` (RGB hex as `#RRGGBB` in comments).

use windows::Win32::Foundation::COLORREF;

/// App chrome / backdrop — `--void` `#0B0E13`
pub const CANVAS: COLORREF = COLORREF(0x0013_0E0B);
/// Navigation / shell — same void family, slightly deeper than panels
pub const NAV: COLORREF = COLORREF(0x000E_0B08);
/// Panels / manager shell — `--surface` `#12171F`
pub const CARD: COLORREF = COLORREF(0x001F_1712);
/// Header band — surface, not a second brand color
pub const HEADER: COLORREF = COLORREF(0x001F_1712);
/// Cards, inputs, menus — `--raised` `#1A2230`
pub const FIELD: COLORREF = COLORREF(0x0030_221A);
/// Result “manuscript” body — `--lex` `#1E2634`
pub const LEX: COLORREF = COLORREF(0x0034_261E);
/// Hover fill
pub const HOVER: COLORREF = COLORREF(0x003A_2A22);
/// Hairline borders — `--line` `#2C3646`
pub const BORDER: COLORREF = COLORREF(0x0046_362C);
/// Field stroke (same hairline system)
pub const FIELD_BORDER: COLORREF = COLORREF(0x0046_362C);
/// Primary text — `--ink` `#F0F4FA`
pub const TEXT: COLORREF = COLORREF(0x00FA_F4F0);
/// Secondary — `--muted` `#8B97A8`
pub const MUTED: COLORREF = COLORREF(0x00A8_978B);
/// Interactive primary / active profile — `--accent` `#8BACFF`
pub const ACCENT: COLORREF = COLORREF(0x00FF_AC8B);
/// Accent pressed
pub const ACCENT_DIM: COLORREF = COLORREF(0x00E0_9A78);
/// Ink on accent fill — `--void`
pub const ACCENT_INK: COLORREF = COLORREF(0x0013_0E0B);
/// Linguistic emphasis (IPA, Expert, section marks) — `--lex-gold` `#D4B56A`
pub const GOLD: COLORREF = COLORREF(0x006A_B5D4);
/// Success
pub const OK: COLORREF = COLORREF(0x00A5_D14F);
/// Warning
pub const WARN: COLORREF = COLORREF(0x005E_A3F0);
/// Destructive / error — `--err` `#F07178`
pub const DANGER: COLORREF = COLORREF(0x0078_71F0);
/// Default button fill — raised
pub const BUTTON: COLORREF = FIELD;
/// Ghost button — transparent over canvas (use canvas as fill)
pub const BUTTON_GHOST: COLORREF = COLORREF(0x001F_1712);
/// Disabled button
pub const BUTTON_DISABLED: COLORREF = COLORREF(0x001A_1612);
/// Button stroke
pub const BUTTON_STROKE: COLORREF = BORDER;
/// Radius scale (logical px @ 96 DPI)
pub const CARD_RADIUS: i32 = 12;
pub const FIELD_RADIUS: i32 = 8;
pub const BUTTON_RADIUS: i32 = 8;
/// Result lexicon left rail width (logical px)
pub const LEX_RAIL: i32 = 3;
/// Default popup geometry (DESIGN.md mockup ≈420–440 utility card)
pub const POPUP_WIDTH: i32 = 420;
pub const POPUP_HEIGHT: i32 = 480;
pub const POPUP_MIN_WIDTH: i32 = 340;
pub const POPUP_MIN_HEIGHT: i32 = 360;
