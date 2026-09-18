//! Shared dark-glass UI tokens for resident popup and manager chrome.
//! COLORREF is `0x00BBGGRR`.

use windows::Win32::Foundation::COLORREF;

/// Window / page canvas.
pub const CANVAS: COLORREF = COLORREF(0x0024_170F); // #0F1724
/// Navigation rail.
pub const NAV: COLORREF = COLORREF(0x001E_120C); // #0C121E
/// Card / surface fill.
pub const CARD: COLORREF = COLORREF(0x0036_2317); // #172336
/// Header band / elevated card.
pub const HEADER: COLORREF = COLORREF(0x0030_1E14); // #141E30
/// Input well / field background.
pub const FIELD: COLORREF = COLORREF(0x0040_2A1C); // #1C2A40
/// Hover fill on cards and buttons.
pub const HOVER: COLORREF = COLORREF(0x0048_3020); // #203048
/// Default border.
pub const BORDER: COLORREF = COLORREF(0x0055_3B2A); // #2A3B55
/// Field stroke (slightly brighter).
pub const FIELD_BORDER: COLORREF = COLORREF(0x0066_4835); // #354866
/// Primary text.
pub const TEXT: COLORREF = COLORREF(0x00F7_EEE8); // #E8EEF7
/// Secondary text.
pub const MUTED: COLORREF = COLORREF(0x00C0_A493); // #93A4C0
/// Accent (buttons, focus, logo chip).
pub const ACCENT: COLORREF = COLORREF(0x00FF_9C4C); // #4C9CFF
/// Accent pressed / dim.
pub const ACCENT_DIM: COLORREF = COLORREF(0x00D6_7F3B); // #3B7FD6
/// Ink on accent fill.
pub const ACCENT_INK: COLORREF = COLORREF(0x0024_170F); // #0F1724
/// Destructive / error text.
pub const DANGER: COLORREF = COLORREF(0x0071_71F8); // #F87171
/// Default button fill.
pub const BUTTON: COLORREF = FIELD;
/// Disabled button fill.
pub const BUTTON_DISABLED: COLORREF = COLORREF(0x0028_1C14); // #141C28
/// Button stroke.
pub const BUTTON_STROKE: COLORREF = BORDER;
/// Radius scale helpers (logical px at 96 DPI).
pub const CARD_RADIUS: i32 = 12;
pub const FIELD_RADIUS: i32 = 8;
pub const BUTTON_RADIUS: i32 = 8;
