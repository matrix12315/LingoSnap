//! Small native, non-activating result popup.

use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub const fn width(self) -> i32 {
        self.right - self.left
    }

    pub const fn height(self) -> i32 {
        self.bottom - self.top
    }
}

/// Clamp a popup rectangle to the monitor work area without changing its size
/// unless the popup is larger than the available work area.
pub fn clamped_origin(anchor: Point, size: (i32, i32), work_area: Rect) -> Point {
    let left = work_area.left.min(work_area.right);
    let right = work_area
        .left
        .max(work_area.right)
        .max(left.saturating_add(1));
    let top = work_area.top.min(work_area.bottom);
    let bottom = work_area
        .top
        .max(work_area.bottom)
        .max(top.saturating_add(1));
    let width = size.0.max(1).min((right - left).max(1));
    let height = size.1.max(1).min((bottom - top).max(1));
    Point {
        x: anchor.x.clamp(left, right - width),
        y: anchor.y.clamp(top, bottom - height),
    }
}

/// Place a cascaded popup beside its parent, preferring the right side and
/// falling back to the left before applying the common monitor clamp.
pub fn cascade_origin(parent: Rect, child_size: (i32, i32), gap: i32, work_area: Rect) -> Point {
    let right = Point {
        x: parent.right.saturating_add(gap),
        y: parent.top,
    };
    let desired = if right.x.saturating_add(child_size.0) <= work_area.right {
        right
    } else {
        Point {
            x: parent.left.saturating_sub(gap).saturating_sub(child_size.0),
            y: parent.top,
        }
    };
    clamped_origin(desired, child_size, work_area)
}

/// Clamp a user/config opacity into `[0.0, 1.0]`; non-finite becomes opaque.
pub fn normalize_opacity(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        1.0
    }
}

/// Opacity used when a new popup/optioner window is created.
static POPUP_OPACITY: AtomicU32 = AtomicU32::new(f32::to_bits(1.0));

/// Record the process-wide popup/optioner opacity (0.0–1.0).
pub fn set_popup_opacity(opacity: f32) {
    POPUP_OPACITY.store(normalize_opacity(opacity).to_bits(), Ordering::Release);
}

/// Read the process-wide popup/optioner opacity (0.0–1.0).
pub fn current_popup_opacity() -> f32 {
    f32::from_bits(POPUP_OPACITY.load(Ordering::Acquire))
}

#[cfg(windows)]
mod windows_impl {
    use super::super::runtime_trace;
    use super::{
        cascade_origin, clamped_origin, current_popup_opacity, normalize_opacity, Point, Rect,
    };
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{
        FreeLibrary, GlobalFree, COLORREF, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT,
        WPARAM,
    };
    use windows::Win32::Graphics::Gdi::{
        BeginPaint, CreateFontW, CreateRoundRectRgn, CreateSolidBrush, DeleteObject, DrawFocusRect,
        DrawTextW, EndPaint, FillRect, FillRgn, FrameRect, FrameRgn, GetMonitorInfoW,
        InvalidateRect, MonitorFromPoint, MonitorFromWindow, SelectObject, SetBkColor, SetBkMode,
        SetTextColor, BACKGROUND_MODE, DRAW_TEXT_FORMAT, FONT_CHARSET, FONT_CLIP_PRECISION,
        FONT_OUTPUT_PRECISION, FONT_QUALITY, HBRUSH, HFONT, HGDIOBJ, HRGN, MONITORINFO,
        MONITOR_DEFAULTTONEAREST, TRANSPARENT,
    };
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::LibraryLoader::LoadLibraryW;
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::UI::Controls::RichEdit::{
        CFE_BOLD, CFE_EFFECTS, CFE_ITALIC, CFE_STRIKEOUT, CFM_BOLD, CFM_CHARSET, CFM_COLOR,
        CFM_FACE, CFM_ITALIC, CFM_SIZE, CFM_STRIKEOUT, CHARFORMATW,
    };
    use windows::Win32::UI::Controls::{SetWindowTheme, DRAWITEMSTRUCT, ODT_BUTTON};
    use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetFocus, VK_ESCAPE};
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
        GetAncestor, GetClassNameW, GetClientRect, GetParent, GetWindow, GetWindowLongPtrW,
        GetWindowRect, GetWindowTextW, IsWindow, IsWindowVisible, MoveWindow, PostMessageW,
        RegisterClassW, SendMessageW, SetForegroundWindow, SetLayeredWindowAttributes,
        SetWindowLongPtrW, SetWindowPos, SetWindowTextW, ShowWindow, TrackPopupMenu,
        WindowFromPoint, BS_PUSHBUTTON, CS_DROPSHADOW, CS_HREDRAW, CS_VREDRAW, ES_AUTOVSCROLL,
        ES_MULTILINE, ES_NOHIDESEL, ES_READONLY, GA_ROOT, GWLP_USERDATA, GWL_EXSTYLE, GW_OWNER,
        HMENU, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCLIENT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT,
        HTTOPRIGHT, HWND_TOPMOST, LWA_ALPHA, MA_NOACTIVATE, MF_STRING, MINMAXINFO, SWP_NOACTIVATE,
        SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNOACTIVATE, TPM_LEFTALIGN,
        TPM_RETURNCMD, TPM_TOPALIGN, WINDOW_STYLE, WM_APP, WM_CLOSE, WM_COMMAND, WM_CREATE,
        WM_DESTROY, WM_DPICHANGED, WM_DRAWITEM, WM_ENTERSIZEMOVE, WM_ERASEBKGND, WM_EXITSIZEMOVE,
        WM_GETMINMAXINFO, WM_KEYDOWN, WM_LBUTTONDOWN, WM_MOUSEACTIVATE, WM_NCCREATE, WM_NCDESTROY,
        WM_NCHITTEST, WM_NCLBUTTONDOWN, WM_PAINT, WM_SETREDRAW, WM_SIZE, WM_TIMER, WNDCLASSW,
        WS_CHILD, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
        WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
    };

    const CLASS_NAME: PCWSTR = w!("SelectionTranslatePopup");
    // Geometry from the egui mockup, scaled to 1x (egui values are ~2x).
    const WIDTH: i32 = 660;
    const HEIGHT: i32 = 470;
    const MIN_WIDTH: i32 = 480;
    const MIN_HEIGHT: i32 = 360;
    const MARGIN: i32 = 15;
    const TARGET_HEIGHT: i32 = 28;
    const CONTEXT_HEIGHT: i32 = 56;
    const LABEL_HEIGHT: i32 = 14;
    const SECTION_GAP: i32 = 8;
    const CONTENT_GAP: i32 = 10;
    const BUTTON_HEIGHT: i32 = 35;
    const BUTTON_GAP: i32 = 9;
    const BUTTON_BOTTOM_GAP: i32 = 12;
    const CARD_PAD: i32 = 13;
    const HEADER_HEIGHT: i32 = 56;
    const DRAG_BAND_HEIGHT: i32 = 24;
    const CHOOSER_HEIGHT: i32 = 38;
    const CHOOSER_MARGIN: i32 = 4;
    const CHOOSER_BUTTON_GAP: i32 = 4;
    const CHOOSER_POINTER_GAP: i32 = 8;
    const CHOOSER_MIN_BUTTON_WIDTH: i32 = 48;
    const CHOOSER_MAX_BUTTON_WIDTH: i32 = 140;
    /// Invisible edge used for custom resize. Frameless so the OS does not
    /// paint a light non-client border over the dark popup.
    const RESIZE_BORDER: i32 = 6;
    const DEFAULT_DPI: u32 = 96;
    const EM_SETSEL: u32 = 0x00b1;
    const EM_LINESCROLL: u32 = 0x00b6;
    const EM_GETFIRSTVISIBLELINE: u32 = 0x00ce;
    const EM_EXLIMITTEXT: u32 = 0x0435;
    const EM_SETBKGNDCOLOR: u32 = 0x0443;
    const EM_SETMARGINS: u32 = 0x00d3;
    const EM_SETCHARFORMAT: u32 = 0x0444;
    const SCF_SELECTION: usize = 0x0001;
    const BASE_FONT_HEIGHT_TWIPS: i32 = 200;
    pub(super) const RICH_EDIT_CLASS: PCWSTR = w!("RICHEDIT50W");
    const REQUIRED_POPUP_EX_STYLE: u32 = WS_EX_TOPMOST.0 | WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0;

    // One palette is shared by the parent, text controls, and owner-drawn
    // buttons so the popup reads as a single surface during every state.
    // COLORREF is 0x00BBGGRR.
    // Exact RGB tokens from the egui mockup (COLORREF is 0x00BBGGRR).
    // Outer: rgb(17,29,49) · border rgb(48,72,106)
    pub(super) const POPUP_BG: COLORREF = COLORREF(0x00311d11);
    const POPUP_BORDER: COLORREF = COLORREF(0x006a4830);
    // Input card: rgb(25,40,65) · border rgb(37,57,85)
    const POPUP_SECTION_BG: COLORREF = COLORREF(0x00412819);
    const POPUP_CARD_BORDER: COLORREF = COLORREF(0x00553925);
    // Result card: rgb(24,39,63) · border rgb(38,59,88)
    const POPUP_HEADER_BG: COLORREF = COLORREF(0x003f2718);
    const POPUP_RESULT_BORDER: COLORREF = COLORREF(0x00583b26);
    // Field well: rgb(26,43,69) · stroke rgb(65,91,128)
    const POPUP_FIELD_BG: COLORREF = COLORREF(0x00452b1a);
    const POPUP_FIELD_BORDER: COLORREF = COLORREF(0x00805b41);
    // Text
    pub(super) const POPUP_TEXT: COLORREF = COLORREF(0x00fcf7f5); // rgb(245,247,252)
    const POPUP_TITLE: COLORREF = COLORREF(0x00fff5f0); // rgb(240,245,255)
    const POPUP_MUTED: COLORREF = COLORREF(0x00cda991); // rgb(145,169,205)
    const POPUP_LABEL: COLORREF = COLORREF(0x00e5bb9e); // rgb(158,187,229)
    #[allow(dead_code)]
    const POPUP_ICON: COLORREF = COLORREF(0x00ffe1cd); // rgb(205,225,255)
    const POPUP_RESULT_ICON: COLORREF = COLORREF(0x00ffa969); // rgb(105,169,255)
    #[allow(dead_code)]
    const POPUP_RESULT_TEXT: COLORREF = COLORREF(0x00fdf9f8); // rgb(248,249,253)
    #[allow(dead_code)]
    const POPUP_CLOSE: COLORREF = COLORREF(0x00f0d2be); // rgb(190,210,240)
                                                        // Logo chip
    pub(super) const POPUP_ACCENT: COLORREF = COLORREF(0x00f4a65b); // rgb(91,166,244)
    const POPUP_LOGO_INK: COLORREF = COLORREF(0x003c2314); // rgb(20,35,60)
                                                           // Buttons
    pub(super) const POPUP_ACCENT_DIM: COLORREF = COLORREF(0x00f4914b); // rgb(75,145,244)
    pub(super) const POPUP_BUTTON_BG: COLORREF = COLORREF(0x00452b1b); // rgb(27,43,69)
    const POPUP_BUTTON_STROKE: COLORREF = COLORREF(0x0077523a); // rgb(58,82,119)
    const POPUP_BUTTON_HOVER: COLORREF = COLORREF(0x005a3c24); // hover fill
    const POPUP_BUTTON_DISABLED: COLORREF = COLORREF(0x003a2819);
    const POPUP_BUTTON_TEXT: COLORREF = COLORREF(0x00fff2eb); // rgb(235,242,255)
    pub(super) const OWNER_DRAW_BUTTON_STYLE: u32 = BS_PUSHBUTTON as u32 | 0x0000000b;

    pub const MAX_OUTPUT_CHARS: usize = 64 * 1024;

    /// Apply whole-window opacity to a native popup/optioner surface.
    /// `1.0` leaves the window non-layered (fully opaque).
    pub fn apply_window_opacity(hwnd: HWND, opacity: f32) {
        let opacity = normalize_opacity(opacity);
        unsafe {
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            if opacity >= 1.0 {
                if style & WS_EX_LAYERED.0 != 0 {
                    let _ =
                        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (style & !WS_EX_LAYERED.0) as isize);
                }
                return;
            }
            let alpha = (opacity * 255.0).round() as u8;
            let layered = style | WS_EX_LAYERED.0;
            if style != layered {
                let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, layered as isize);
            }
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);
        }
    }

    const MAX_INPUT_CHARS: usize = 4 * 1024;
    const MAX_OUTPUT_UTF16_UNITS: usize = MAX_OUTPUT_CHARS * 2;
    const TRUNCATION_MARKER: &str = "\n\n[Output truncated]";
    const OUTPUT_ID: usize = 1;
    const COPY_ID: usize = 2;
    const RETRY_ID: usize = 3;
    const PROMPT_ID: usize = 4;
    const PIN_ID: usize = 5;
    const CLOSE_ID: usize = 6;
    const INPUT_ID: usize = 7;
    const PROFILE_CHOICE_ID_START: usize = 1000;
    const PROFILE_MORE_ID: usize = 900;
    const RENDER_TIMER_ID: usize = 1;
    const RENDER_TIMER_MS: u32 = 40;
    const INLINE_PROFILE_LIMIT: usize = 3;
    pub const POPUP_DISMISSED: u32 = WM_APP + 8;
    pub const POPUP_RETRY: u32 = WM_APP + 9;
    pub const POPUP_PROMPT: u32 = WM_APP + 10;
    pub const POPUP_PROFILE_SELECTED: u32 = WM_APP + 11;
    pub type PopupId = usize;

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub(super) enum PopupState {
        Loading,
        Streaming(OutputBuffer),
        Completed(OutputBuffer),
        LocalError(String),
    }

    impl PopupState {
        pub(super) fn append(&mut self, delta: &str) {
            match self {
                Self::Loading => {
                    let mut output = OutputBuffer::new("");
                    output.append(delta);
                    *self = Self::Streaming(output);
                }
                Self::Streaming(output) => output.append(delta),
                Self::Completed(_) | Self::LocalError(_) => {}
            }
        }

        pub(super) fn finish(&mut self) {
            if let Self::Streaming(output) = self {
                *self = Self::Completed(output.clone());
            }
        }
    }

    pub(super) fn popup_allows_hover_text(state: &PopupState) -> bool {
        matches!(state, PopupState::Completed(_))
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub(super) struct OutputBuffer {
        pub(super) text: String,
        pub(super) truncated: bool,
        char_count: usize,
    }

    impl OutputBuffer {
        pub(super) fn new(text: &str) -> Self {
            let mut output = Self {
                text: String::new(),
                truncated: false,
                char_count: 0,
            };
            output.append(text);
            output
        }

        pub(super) fn append(&mut self, delta: &str) {
            if self.truncated {
                return;
            }
            let marker_len = TRUNCATION_MARKER.chars().count();
            let available =
                MAX_OUTPUT_CHARS.saturating_sub(self.char_count.saturating_add(marker_len));
            let mut chars = delta.chars();
            let mut accepted: usize = 0;
            self.text.extend(
                chars
                    .by_ref()
                    .take(available)
                    .inspect(|_| accepted = accepted.saturating_add(1)),
            );
            self.char_count = self.char_count.saturating_add(accepted);
            if chars.next().is_some() {
                self.text.push_str(TRUNCATION_MARKER);
                self.truncated = true;
            }
        }
    }

    #[derive(Debug)]
    struct PopupData {
        id: PopupId,
        state: PopupState,
        pinned: bool,
        /// Message-only resident window that receives popup commands. It is
        /// deliberately not the native owner: a window created with a
        /// message-only parent/owner becomes message-only and cannot display.
        callback_target: HWND,
        anchor: Point,
        dpi: u32,
        input: HWND,
        context_input: HWND,
        output: HWND,
        rich_edit_module: windows::Win32::Foundation::HMODULE,
        buttons: [HWND; 5],
        profile_buttons: Vec<HWND>,
        profile_button_widths: Vec<i32>,
        profile_labels: Vec<String>,
        choosing_profile: bool,
        /// Only a user close should notify the resident. Replacement and
        /// cancellation destroy the window silently.
        notify_owner: bool,
        /// True while Windows owns the native move loop. Markdown projection
        /// is deliberately deferred during this interval because RichEdit
        /// formatting/repaint work competes with pointer motion.
        in_native_move: bool,
        /// A state update arrived during the move loop and needs one render
        /// after WM_EXITSIZEMOVE.
        render_pending: bool,
        render_timer_armed: bool,
        /// Last physical window size chosen by the user (or the initial
        /// default). Present/reanchor paths reuse this instead of snapping
        /// back to the default geometry.
        window_size: Option<(i32, i32)>,
        fonts: [HFONT; 2],
    }

    pub struct Popup {
        hwnd: HWND,
    }

    impl Popup {
        pub fn show(owner: HWND, id: PopupId, anchor: Point) -> windows::core::Result<Self> {
            Self::create(owner, id, anchor, true)
        }

        pub fn stage(owner: HWND, id: PopupId, anchor: Point) -> windows::core::Result<Self> {
            Self::create(owner, id, anchor, false)
        }

        fn create(
            owner: HWND,
            id: PopupId,
            anchor: Point,
            present: bool,
        ) -> windows::core::Result<Self> {
            runtime_trace::record("popup_show_create_attempt");
            if let Err(error) = register_class() {
                runtime_trace::record("popup_show_create_failure");
                return Err(error);
            }
            let data = Box::new(PopupData {
                id,
                state: PopupState::Loading,
                pinned: false,
                callback_target: owner,
                anchor,
                dpi: DEFAULT_DPI,
                input: HWND::default(),
                context_input: HWND::default(),
                output: HWND::default(),
                rich_edit_module: windows::Win32::Foundation::HMODULE::default(),
                buttons: [HWND::default(); 5],
                profile_buttons: Vec::new(),
                profile_button_widths: Vec::new(),
                profile_labels: Vec::new(),
                choosing_profile: false,
                notify_owner: true,
                in_native_move: false,
                render_pending: false,
                render_timer_armed: false,
                window_size: None,
                fonts: [HFONT::default(); 2],
            });
            let data_ptr = Box::into_raw(data);
            let result = unsafe {
                let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)?;
                // The initial rectangle is only a creation rectangle. The
                // final position/size is selected after the window DPI is
                // known, using the anchor monitor's work area.
                CreateWindowExW(
                    WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                    CLASS_NAME,
                    w!("Selection Translate"),
                    WS_POPUP,
                    anchor.x,
                    anchor.y,
                    WIDTH,
                    HEIGHT,
                    None,
                    None,
                    Some(HINSTANCE(instance.0)),
                    Some(data_ptr.cast()),
                )
            };
            let hwnd = match result {
                Ok(hwnd) => hwnd,
                Err(error) => {
                    // WM_NCDESTROY cannot reclaim a pointer when window
                    // creation itself fails.
                    unsafe { drop(Box::from_raw(data_ptr)) };
                    runtime_trace::record("popup_show_create_failure");
                    return Err(error);
                }
            };
            let surface_ready = data_mut(hwnd).is_some_and(|data| {
                !data.input.0.is_null()
                    && !data.context_input.0.is_null()
                    && !data.output.0.is_null()
            });
            if !surface_ready {
                runtime_trace::record("popup_surface_child_failure");
                let mut popup = Self { hwnd };
                popup.dismiss();
                return Err(windows::core::Error::new(
                    windows::core::HRESULT(0x8000_4005_u32 as i32),
                    "result surface unavailable",
                ));
            }
            runtime_trace::record("popup_surface_child_ready");
            let dpi = dpi_for_window(hwnd);
            if let Some(data) = data_mut(hwnd) {
                data.dpi = dpi;
            }
            apply_layout(hwnd, anchor, dpi);
            apply_window_opacity(hwnd, current_popup_opacity());
            let size = resolve_window_size(hwnd, dpi);
            let origin = origin_for(anchor, size).unwrap_or(anchor);
            if present && !present_popup(hwnd, origin, size) {
                runtime_trace::record("popup_show_presentation_failure");
                let mut popup = Self { hwnd };
                popup.dismiss();
                return Err(windows::core::Error::new(
                    windows::core::HRESULT(0x8000_4005_u32 as i32),
                    "result surface presentation unavailable",
                ));
            }
            if present {
                record_topology(hwnd);
                runtime_trace::record("popup_show_visible");
            } else {
                runtime_trace::record("popup_staged_hidden");
            }
            Ok(Self { hwnd })
        }

        pub fn present_staged(&mut self) -> bool {
            let Some(data) = data_mut(self.hwnd) else {
                return false;
            };
            let size = data
                .window_size
                .unwrap_or_else(|| default_popup_size(data.dpi));
            let origin = origin_for(data.anchor, size).unwrap_or(data.anchor);
            if !present_popup(self.hwnd, origin, size) {
                return false;
            }
            record_topology(self.hwnd);
            runtime_trace::record("popup_staged_visible");
            true
        }

        pub fn hide_temporarily(&mut self) -> bool {
            if !self.is_result_surface_available() {
                return false;
            }
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
                IsWindow(Some(self.hwnd)).as_bool() && !IsWindowVisible(self.hwnd).as_bool()
            }
        }

        /// Move an existing popup to the anchor of an accepted replacement.
        /// Returns `false` when the native window has already been destroyed,
        /// allowing the resident to discard the stale wrapper and create a
        /// fresh popup.
        pub fn reanchor(&mut self, anchor: Point) -> bool {
            let Some(data) = data_mut(self.hwnd) else {
                runtime_trace::record("popup_reanchor_failure");
                return false;
            };
            if data.output.0.is_null() {
                runtime_trace::record("popup_reanchor_failure");
                return false;
            }
            let dpi = dpi_for_window(self.hwnd);
            data.anchor = anchor;
            data.dpi = dpi;
            let size = resolve_window_size(self.hwnd, dpi);
            apply_layout(self.hwnd, anchor, dpi);
            let origin = origin_for(anchor, size).unwrap_or(anchor);
            if !present_popup(self.hwnd, origin, size) {
                runtime_trace::record("popup_reanchor_presentation_failure");
                return false;
            }
            record_topology(self.hwnd);
            runtime_trace::record("popup_reanchor_success");
            true
        }

        /// Apply whole-window opacity (0.0–1.0) to this popup/optioner surface.
        pub fn set_opacity(&mut self, opacity: f32) -> bool {
            if !unsafe { IsWindow(Some(self.hwnd)) }.as_bool() {
                return false;
            }
            apply_window_opacity(self.hwnd, opacity);
            true
        }

        pub fn show_loading(&mut self) {
            let layout = if let Some(data) = data_mut(self.hwnd) {
                leave_profile_chooser(data);
                data.state = PopupState::Loading;
                let render_now = request_render(self.hwnd, data);
                Some((data.anchor, data.dpi, render_now))
            } else {
                None
            };
            if let Some((anchor, dpi, render_now)) = layout {
                if render_now {
                    sync_controls(self.hwnd);
                }
                apply_layout(self.hwnd, anchor, dpi);
            }
        }

        pub fn update(&mut self, delta: &str) {
            runtime_trace::record("popup_delta_received");
            let render_now = if let Some(data) = data_mut(self.hwnd) {
                data.state.append(delta);
                request_render(self.hwnd, data)
            } else {
                false
            };
            if render_now {
                sync_controls(self.hwnd);
            }
        }

        pub fn finish(&mut self) {
            runtime_trace::record("popup_finish_received");
            let render_now = if let Some(data) = data_mut(self.hwnd) {
                data.state.finish();
                request_render(self.hwnd, data)
            } else {
                false
            };
            if render_now {
                sync_controls(self.hwnd);
            }
        }

        pub fn show_local_error(&mut self, message: &str) {
            let layout = if let Some(data) = data_mut(self.hwnd) {
                leave_profile_chooser(data);
                data.state = PopupState::LocalError(bounded_string(message));
                let render_now = request_render(self.hwnd, data);
                Some((data.anchor, data.dpi, render_now))
            } else {
                None
            };
            if let Some((anchor, dpi, render_now)) = layout {
                if render_now {
                    sync_controls(self.hwnd);
                }
                apply_layout(self.hwnd, anchor, dpi);
            }
        }

        pub fn set_text(&mut self, text: &str) {
            let layout = if let Some(data) = data_mut(self.hwnd) {
                leave_profile_chooser(data);
                data.state = PopupState::Completed(OutputBuffer::new(text));
                let render_now = request_render(self.hwnd, data);
                Some((data.anchor, data.dpi, render_now))
            } else {
                None
            };
            if let Some((anchor, dpi, render_now)) = layout {
                if render_now {
                    sync_controls(self.hwnd);
                }
                apply_layout(self.hwnd, anchor, dpi);
            }
        }

        /// Replace the result surface with a names-only profile chooser.
        /// No selected text or prompt content is placed in these controls.
        pub fn show_profile_choices(&mut self, names: &[String]) -> bool {
            let Some(data) = data_mut(self.hwnd) else {
                return false;
            };
            if names.is_empty() {
                return false;
            }
            clear_profile_buttons(data);
            data.profile_labels = names
                .iter()
                .map(|name| compact_profile_label(name))
                .collect();
            set_standard_controls_visible(data, false);
            let Ok(instance) =
                (unsafe { windows::Win32::System::LibraryLoader::GetModuleHandleW(None) })
            else {
                set_standard_controls_visible(data, true);
                return false;
            };
            let inline_count = data.profile_labels.len().min(INLINE_PROFILE_LIMIT);
            let mut visible_labels = data.profile_labels[..inline_count].to_vec();
            if data.profile_labels.len() > INLINE_PROFILE_LIMIT {
                visible_labels.push("More…".to_owned());
            }
            data.profile_button_widths = visible_labels
                .iter()
                .map(|label| chooser_button_width(label))
                .collect();
            for (index, name) in visible_labels.iter().enumerate() {
                let command_id = if index == INLINE_PROFILE_LIMIT {
                    PROFILE_MORE_ID
                } else {
                    PROFILE_CHOICE_ID_START + index
                };
                let mut label: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
                let button = unsafe {
                    CreateWindowExW(
                        Default::default(),
                        w!("BUTTON"),
                        PCWSTR(label.as_mut_ptr()),
                        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(OWNER_DRAW_BUTTON_STYLE),
                        0,
                        0,
                        1,
                        1,
                        Some(self.hwnd),
                        Some(child_menu(command_id)),
                        Some(HINSTANCE(instance.0)),
                        None,
                    )
                }
                .unwrap_or_default();
                if button.0.is_null() {
                    clear_profile_buttons(data);
                    set_standard_controls_visible(data, true);
                    return false;
                }
                data.profile_buttons.push(button);
            }
            data.choosing_profile = true;
            let anchor = data.anchor;
            let dpi = data.dpi;
            let size = chooser_size(anchor, &data.profile_button_widths, dpi);
            let origin = chooser_origin(anchor, size, dpi).unwrap_or(anchor);
            let presented = present_popup(self.hwnd, origin, size);
            if presented {
                // Do NOT write chooser geometry into window_size: that field
                // is the durable result-popup size and must stay usable after
                // the user picks a profile.
                if let Some(data) = data_mut(self.hwnd) {
                    for button in data.profile_buttons.clone() {
                        if !button.0.is_null() && !data.fonts[1].0.is_null() {
                            unsafe {
                                let _ = SendMessageW(
                                    button,
                                    0x0030,
                                    Some(WPARAM(data.fonts[1].0 as usize)),
                                    Some(LPARAM(1)),
                                );
                            }
                        }
                    }
                }
                layout_children(self.hwnd, dpi);
                apply_round_region(self.hwnd, size, dpi);
            }
            presented
        }

        /// Target and context stay in separate panes so the user can see
        /// exactly which text is being translated versus which local sentence
        /// is only disambiguating it.
        pub fn set_input(&mut self, target: &str, context: Option<&str>) {
            if let Some(data) = data_mut(self.hwnd) {
                set_control_text(data.input, &bounded_input(target));
                let context_text = match context {
                    Some(value) if !value.is_empty() && value != target => bounded_input(value),
                    _ => String::new(),
                };
                set_control_text(data.context_input, &context_text);
            }
        }

        pub fn set_pinned(&mut self, pinned: bool) {
            let button = if let Some(data) = data_mut(self.hwnd) {
                data.pinned = pinned;
                Some((data.buttons[3], if data.pinned { "Unpin" } else { "Pin" }))
            } else {
                None
            };
            if let Some((button, label)) = button {
                set_control_text(button, label);
            }
        }

        pub fn is_pinned(&self) -> bool {
            data_mut(self.hwnd).is_some_and(|data| data.pinned)
        }

        pub fn is_completed(&self) -> bool {
            data_mut(self.hwnd).is_some_and(|data| matches!(data.state, PopupState::Completed(_)))
        }

        pub fn id(&self) -> Option<PopupId> {
            data_mut(self.hwnd).map(|data| data.id)
        }

        pub fn owns_window(&self, candidate: HWND) -> bool {
            if self.hwnd.0.is_null() || candidate.0.is_null() {
                return false;
            }
            let root = unsafe { GetAncestor(candidate, GA_ROOT) };
            let root = if root.0.is_null() { candidate } else { root };
            root == self.hwnd
        }

        pub fn contains_window_point(&self, point: Point) -> bool {
            if self.hwnd.0.is_null() {
                return false;
            }
            let candidate = unsafe {
                WindowFromPoint(POINT {
                    x: point.x,
                    y: point.y,
                })
            };
            self.owns_window(candidate)
        }

        pub fn contains_completed_output_point(&self, point: Point) -> bool {
            if self.hwnd.0.is_null() {
                return false;
            }
            let candidate = unsafe {
                WindowFromPoint(POINT {
                    x: point.x,
                    y: point.y,
                })
            };
            data_mut(self.hwnd).is_some_and(|data| {
                candidate == data.output && popup_allows_hover_text(&data.state)
            })
        }

        /// Preferred origin for a cascaded child. Keep the child beside its
        /// source popup, using the right side when it fits and falling back to
        /// the left, then clamp the final rectangle to the monitor work area.
        pub fn cascade_anchor(&self) -> Option<Point> {
            if self.hwnd.0.is_null() {
                return None;
            }
            let mut parent = RECT::default();
            unsafe { GetWindowRect(self.hwnd, &mut parent) }.ok()?;
            let dpi = dpi_for_window(self.hwnd);
            let child_size = default_popup_size(dpi);
            let gap = scale(8, dpi);
            let monitor_point = Point {
                x: parent.left,
                y: parent.top,
            };
            let work_area = work_area_for(monitor_point).ok()?;
            Some(cascade_origin(
                Rect {
                    left: parent.left,
                    top: parent.top,
                    right: parent.right,
                    bottom: parent.bottom,
                },
                child_size,
                gap,
                work_area,
            ))
        }

        pub fn is_result_surface_available(&self) -> bool {
            data_mut(self.hwnd).is_some_and(|data| !data.output.0.is_null())
        }

        pub fn dismiss(&mut self) {
            if self.hwnd.0.is_null() {
                return;
            }
            runtime_trace::record("popup_programmatic_dismiss");
            // This is used by replacement/cancellation and must not be
            // mistaken for the user's Close/Escape action.
            if let Some(data) = data_mut(self.hwnd) {
                data.notify_owner = false;
            }
            unsafe {
                let _ = DestroyWindow(self.hwnd);
            }
            self.hwnd = HWND::default();
        }
    }

    /// Record only fixed topology labels so a failed external popup probe can
    /// distinguish a missing window from a wrong owner/class relationship.
    /// Handles, titles, control text, and OS error strings are intentionally
    /// excluded from the trace.
    fn record_topology(hwnd: HWND) {
        unsafe {
            if IsWindow(Some(hwnd)).as_bool() {
                runtime_trace::record("popup_topology_is_window_true");
            } else {
                runtime_trace::record("popup_topology_is_window_false");
            }
            if IsWindowVisible(hwnd).as_bool() {
                runtime_trace::record("popup_topology_is_window_visible_true");
            } else {
                runtime_trace::record("popup_topology_is_window_visible_false");
            }
            if GetParent(hwnd).unwrap_or_default().0.is_null() {
                runtime_trace::record("popup_topology_parent_null");
            } else {
                runtime_trace::record("popup_topology_parent_non_null");
            }
            if GetWindow(hwnd, GW_OWNER).unwrap_or_default().0.is_null() {
                runtime_trace::record("popup_topology_owner_null");
            } else {
                runtime_trace::record("popup_topology_owner_non_null");
            }
            let mut class_name = [0u16; 128];
            let length = GetClassNameW(hwnd, &mut class_name);
            let exact = length > 0
                && String::from_utf16_lossy(&class_name[..length as usize])
                    == "SelectionTranslatePopup";
            if exact {
                runtime_trace::record("popup_topology_class_exact");
            } else {
                runtime_trace::record("popup_topology_class_mismatch");
            }
        }
    }

    /// Position and expose the popup without activating it, then verify the
    /// native presentation invariants. A successful SetWindowPos call alone
    /// is insufficient: another window manager or a stale HWND can leave the
    /// surface hidden behind the foreground application.
    fn present_popup(hwnd: HWND, origin: Point, size: (i32, i32)) -> bool {
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            if SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                origin.x,
                origin.y,
                size.0,
                size.1,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
            .is_err()
            {
                return false;
            }
            if !IsWindow(Some(hwnd)).as_bool() || !IsWindowVisible(hwnd).as_bool() {
                return false;
            }
            let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            if !popup_ex_style_is_presentable(ex_style) {
                return false;
            }
            let mut window_rect = RECT::default();
            if GetWindowRect(hwnd, &mut window_rect).is_err() {
                return false;
            }
            let actual = Rect {
                left: window_rect.left,
                top: window_rect.top,
                right: window_rect.right,
                bottom: window_rect.bottom,
            };
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            if monitor.is_invalid() {
                return false;
            }
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if !GetMonitorInfoW(monitor, &mut info).as_bool() {
                return false;
            }
            let work_area = Rect {
                left: info.rcWork.left,
                top: info.rcWork.top,
                right: info.rcWork.right,
                bottom: info.rcWork.bottom,
            };
            if !popup_rect_is_presentable(actual, work_area) {
                return false;
            }
            apply_window_opacity(hwnd, current_popup_opacity());
            true
        }
    }

    pub(super) fn popup_ex_style_is_presentable(style: u32) -> bool {
        style & REQUIRED_POPUP_EX_STYLE == REQUIRED_POPUP_EX_STYLE
    }

    pub(super) fn popup_rect_is_presentable(rect: Rect, monitor_work_area: Rect) -> bool {
        rect.width() > 0
            && rect.height() > 0
            && rect.left < monitor_work_area.right
            && rect.right > monitor_work_area.left
            && rect.top < monitor_work_area.bottom
            && rect.bottom > monitor_work_area.top
    }

    impl Drop for Popup {
        fn drop(&mut self) {
            self.dismiss();
        }
    }

    fn register_class() -> windows::core::Result<()> {
        static ONCE: std::sync::OnceLock<windows::core::Result<()>> = std::sync::OnceLock::new();
        let registered = ONCE.get_or_init(|| {
            let instance =
                unsafe { windows::Win32::System::LibraryLoader::GetModuleHandleW(None)? };
            let class = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW | CS_DROPSHADOW,
                lpfnWndProc: Some(popup_wnd_proc),
                hInstance: HINSTANCE(instance.0),
                // The class and every popup instance share one process-lifetime
                // brush. CTLCOLOR messages must return a brush which remains
                // valid after the callback; allocating one per paint leaks GDI
                // handles and deleting it immediately is invalid.
                hbrBackground: popup_background_brush(),
                lpszClassName: CLASS_NAME,
                ..Default::default()
            };
            let atom = unsafe { RegisterClassW(&class) };
            if atom == 0 {
                Err(windows::core::Error::from_win32())
            } else {
                Ok(())
            }
        });
        registered.clone()
    }

    fn data_mut(hwnd: HWND) -> Option<&'static mut PopupData> {
        let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut PopupData;
        (!pointer.is_null()).then(|| unsafe { &mut *pointer })
    }

    fn dpi_for_window(hwnd: HWND) -> u32 {
        let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd) };
        if dpi == 0 {
            DEFAULT_DPI
        } else {
            dpi
        }
    }

    fn scale(value: i32, dpi: u32) -> i32 {
        ((value as i64 * dpi as i64 + 48) / 96) as i32
    }

    pub(super) fn scaled_size(size: (i32, i32), dpi: u32) -> (i32, i32) {
        (scale(size.0, dpi).max(1), scale(size.1, dpi).max(1))
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) enum ButtonVisualState {
        Normal,
        Pressed,
        Disabled,
        Focused,
    }

    pub(super) fn button_fill(state: ButtonVisualState) -> COLORREF {
        match state {
            ButtonVisualState::Normal => POPUP_BUTTON_BG,
            ButtonVisualState::Pressed => POPUP_ACCENT_DIM,
            ButtonVisualState::Disabled => POPUP_BUTTON_DISABLED,
            ButtonVisualState::Focused => POPUP_BUTTON_HOVER,
        }
    }

    pub(super) fn popup_corner_radius(dpi: u32) -> i32 {
        scale(12, dpi).max(8)
    }

    fn work_area_for(anchor: Point) -> windows::core::Result<Rect> {
        let monitor = unsafe {
            MonitorFromPoint(
                POINT {
                    x: anchor.x,
                    y: anchor.y,
                },
                MONITOR_DEFAULTTONEAREST,
            )
        };
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !unsafe { GetMonitorInfoW(monitor, &mut info).as_bool() } {
            return Err(windows::core::Error::from_win32());
        }
        Ok(Rect {
            left: info.rcWork.left,
            top: info.rcWork.top,
            right: info.rcWork.right,
            bottom: info.rcWork.bottom,
        })
    }

    fn origin_for(anchor: Point, size: (i32, i32)) -> windows::core::Result<Point> {
        Ok(clamped_origin(anchor, size, work_area_for(anchor)?))
    }

    pub(super) fn chooser_size(anchor: Point, button_widths: &[i32], dpi: u32) -> (i32, i32) {
        let count = button_widths.len().max(1) as i32;
        let desired_width = scale(
            CHOOSER_MARGIN * 2
                + button_widths.iter().copied().sum::<i32>()
                + CHOOSER_BUTTON_GAP * (count - 1),
            dpi,
        );
        let available_width = work_area_for(anchor)
            .map(|area| area.width().max(1))
            .unwrap_or(desired_width);
        (
            desired_width.min(available_width).max(1),
            scale(CHOOSER_HEIGHT, dpi).max(1),
        )
    }

    pub(super) fn chooser_button_width(label: &str) -> i32 {
        ((label.chars().count() as i32).saturating_mul(8) + 20)
            .clamp(CHOOSER_MIN_BUTTON_WIDTH, CHOOSER_MAX_BUTTON_WIDTH)
    }

    pub(super) fn compact_profile_label(name: &str) -> String {
        let word = name
            .trim()
            .split(|character: char| {
                character.is_whitespace() || matches!(character, '-' | '_' | '/' | '\\')
            })
            .find(|part| !part.is_empty())
            .unwrap_or("Profile");
        let mut characters = word.chars();
        let mut compact: String = characters.by_ref().take(11).collect();
        if characters.next().is_some() {
            compact.push('…');
        }
        compact
    }

    fn chooser_origin(anchor: Point, size: (i32, i32), dpi: u32) -> windows::core::Result<Point> {
        let desired = Point {
            x: anchor.x.saturating_sub(size.0 / 2),
            y: anchor
                .y
                .saturating_sub(size.1)
                .saturating_sub(scale(CHOOSER_POINTER_GAP, dpi)),
        };
        Ok(clamped_origin(desired, size, work_area_for(anchor)?))
    }

    /// Result-popup size is valid only when it can host target/context/result.
    /// Chooser strip dimensions must never win this check.
    pub(super) fn valid_result_size(
        stored: Option<(i32, i32)>,
        min: (i32, i32),
        fallback: (i32, i32),
    ) -> (i32, i32) {
        match stored {
            Some((width, height)) if width >= min.0 && height >= min.1 => (width, height),
            _ => fallback,
        }
    }

    fn resolve_window_size(hwnd: HWND, dpi: u32) -> (i32, i32) {
        let min = min_popup_size(dpi);
        let fallback = default_popup_size(dpi);
        if let Some(data) = data_mut(hwnd) {
            let size = valid_result_size(data.window_size, min, fallback);
            data.window_size = Some(size);
            return size;
        }
        fallback
    }

    fn default_popup_size(dpi: u32) -> (i32, i32) {
        scaled_size((WIDTH, HEIGHT), dpi)
    }

    fn min_popup_size(dpi: u32) -> (i32, i32) {
        scaled_size((MIN_WIDTH, MIN_HEIGHT), dpi)
    }

    fn apply_layout(hwnd: HWND, anchor: Point, dpi: u32) {
        let size = resolve_window_size(hwnd, dpi);
        let origin = origin_for(anchor, size).unwrap_or(anchor);
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                None,
                origin.x,
                origin.y,
                size.0,
                size.1,
                SWP_NOACTIVATE | SWP_NOZORDER,
            );
        }
        layout_children(hwnd, dpi);
        apply_round_region(hwnd, size, dpi);
    }

    fn apply_round_region(hwnd: HWND, size: (i32, i32), dpi: u32) {
        let radius = popup_corner_radius(dpi);
        let region: HRGN =
            unsafe { CreateRoundRectRgn(0, 0, size.0 + 1, size.1 + 1, radius, radius) };
        if !region.0.is_null() {
            unsafe {
                // Windows owns the region only when SetWindowRgn succeeds.
                if windows::Win32::Graphics::Gdi::SetWindowRgn(hwnd, Some(region), true) == 0 {
                    let _ = DeleteObject(region.into());
                }
            }
        }
    }

    /// Edge hit-test for frameless resize. Interior returns HTCLIENT so the
    /// existing drag-band and child-control paths keep working.
    pub(super) fn resize_hit_test(
        point: Point,
        width: i32,
        height: i32,
        border: i32,
        choosing_profile: bool,
    ) -> LRESULT {
        let border = border.max(1);
        let left = point.x >= 0 && point.x < border;
        let right = point.x >= width - border && point.x < width;
        let top = point.y >= 0 && point.y < border;
        let bottom = point.y >= height - border && point.y < height;
        let code = match (left, right, top, bottom) {
            (true, _, true, _) => HTTOPLEFT,
            (_, true, true, _) => HTTOPRIGHT,
            (true, _, _, true) => HTBOTTOMLEFT,
            (_, true, _, true) => HTBOTTOMRIGHT,
            (true, _, _, _) => HTLEFT,
            (_, true, _, _) => HTRIGHT,
            (_, _, true, _) if !choosing_profile => HTTOP,
            (_, _, _, true) => HTBOTTOM,
            _ => HTCLIENT,
        };
        LRESULT(code as isize)
    }

    fn move_child(hwnd: HWND, x: i32, y: i32, width: i32, height: i32) {
        if !hwnd.0.is_null() {
            unsafe {
                let _ = MoveWindow(hwnd, x, y, width, height, true);
            }
        }
    }

    /// Shared geometry for child controls and parent paint. Every metric is
    /// derived from the current client size so a resize updates the whole
    /// popup, not only the result pane.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) struct PopupLayout {
        pub(super) content_width: i32,
        pub(super) margin: i32,
        pub(super) drag_band: i32,
        pub(super) label_height: i32,
        pub(super) target_label_top: i32,
        pub(super) target_top: i32,
        pub(super) target_height: i32,
        pub(super) context_label_top: i32,
        pub(super) context_top: i32,
        pub(super) context_height: i32,
        pub(super) output_label_top: i32,
        pub(super) output_top: i32,
        pub(super) output_height: i32,
        pub(super) button_top: i32,
        pub(super) button_height: i32,
        pub(super) button_width: i32,
        pub(super) button_gap: i32,
        pub(super) header_height: i32,
        pub(super) input_card_top: i32,
        pub(super) input_card_bottom: i32,
        pub(super) output_card_top: i32,
        pub(super) output_card_bottom: i32,
    }

    pub(super) fn compute_layout(
        client_width: i32,
        client_height: i32,
        dpi: u32,
        button_count: i32,
    ) -> PopupLayout {
        let margin = scale(MARGIN, dpi);
        let header = scale(HEADER_HEIGHT, dpi);
        let label_height = scale(LABEL_HEIGHT, dpi);
        let section_gap = scale(SECTION_GAP, dpi);
        let content_gap = scale(CONTENT_GAP, dpi);
        let card_pad = scale(CARD_PAD, dpi);
        let button_height = scale(BUTTON_HEIGHT, dpi);
        let button_gap = scale(BUTTON_GAP, dpi);
        let button_bottom_gap = scale(BUTTON_BOTTOM_GAP, dpi);
        let preferred_target = scale(TARGET_HEIGHT, dpi).max(1);
        let preferred_context = scale(CONTEXT_HEIGHT, dpi).max(1);
        let content_width = (client_width - margin * 2).max(1);
        let button_count = button_count.max(1);
        let button_width =
            ((content_width - button_gap * (button_count - 1)) / button_count).max(1);

        let button_top = (client_height - button_height - button_bottom_gap).max(header + margin);
        let output_card_bottom = (button_top - content_gap).max(header + margin);

        // Input card holds Target + Context; result card holds Result.
        let target_label_top = header + scale(8, dpi);
        let target_top = target_label_top + label_height + scale(4, dpi);
        let context_label_top = target_top + preferred_target + section_gap;
        let context_top = context_label_top + label_height + scale(4, dpi);
        let input_card_bottom = context_top + preferred_context + card_pad;
        let input_card_top = header + scale(4, dpi);

        let output_card_top = input_card_bottom + content_gap;
        let output_label_top = output_card_top + card_pad;
        let output_top = output_label_top + label_height + scale(4, dpi);
        let output_height = (output_card_bottom - output_top - card_pad).max(scale(48, dpi));

        PopupLayout {
            content_width,
            margin,
            drag_band: header,
            label_height,
            target_label_top,
            target_top,
            target_height: preferred_target,
            context_label_top,
            context_top,
            context_height: preferred_context,
            output_label_top,
            output_top,
            output_height,
            button_top,
            button_height,
            button_width,
            button_gap,
            header_height: header,
            input_card_top,
            input_card_bottom,
            output_card_top,
            output_card_bottom,
        }
    }

    /// Compact chooser strip: fixed short height, vertically centered, with
    /// per-button x positions and widths. Never stretches to full client height.
    pub(super) fn chooser_strip_layout(
        client_width: i32,
        client_height: i32,
        natural_widths: &[i32],
        dpi: u32,
    ) -> (i32, i32, Vec<(i32, i32)>) {
        let margin = scale(CHOOSER_MARGIN, dpi);
        let gap = scale(CHOOSER_BUTTON_GAP, dpi);
        let row_height = scale(CHOOSER_HEIGHT, dpi).max(1);
        let row_top = ((client_height - row_height) / 2).max(margin);
        let count = natural_widths.len().max(1);
        let available = (client_width - margin * 2 - gap * (count as i32 - 1)).max(1);
        let natural_total = natural_widths.iter().copied().sum::<i32>().max(1);
        let mut rects = Vec::with_capacity(count);
        let mut x = margin;
        for index in 0..count {
            let width = if index + 1 == count {
                (client_width - margin - x).max(1)
            } else if natural_total > available {
                (natural_widths.get(index).copied().unwrap_or(1) * available / natural_total).max(1)
            } else {
                natural_widths.get(index).copied().unwrap_or(1)
            };
            rects.push((x, width));
            x += width + gap;
        }
        (row_top, row_height, rects)
    }

    fn layout_children(hwnd: HWND, dpi: u32) {
        let Some(data) = data_mut(hwnd) else { return };
        let mut client = RECT::default();
        if unsafe { GetClientRect(hwnd, &mut client) }.is_err() {
            return;
        }
        if data.choosing_profile {
            let natural: Vec<i32> = data
                .profile_button_widths
                .iter()
                .map(|width| scale(*width, dpi).max(1))
                .collect();
            let (row_top, row_height, rects) =
                chooser_strip_layout(client.right, client.bottom, &natural, dpi);
            for (index, button) in data.profile_buttons.iter().enumerate() {
                let (x, width) = rects.get(index).copied().unwrap_or((0, 1));
                move_child(*button, x, row_top, width, row_height);
            }
            return;
        }
        let layout = compute_layout(client.right, client.bottom, dpi, data.buttons.len() as i32);
        move_child(
            data.input,
            layout.margin,
            layout.target_top,
            layout.content_width,
            layout.target_height,
        );
        move_child(
            data.context_input,
            layout.margin,
            layout.context_top,
            layout.content_width,
            layout.context_height,
        );
        move_child(
            data.output,
            layout.margin,
            layout.output_top,
            layout.content_width,
            layout.output_height,
        );
        for (index, button) in data.buttons.iter().enumerate() {
            let x = layout.margin + index as i32 * (layout.button_width + layout.button_gap);
            move_child(
                *button,
                x,
                layout.button_top,
                layout.button_width,
                layout.button_height,
            );
        }
    }

    fn bounded_string(text: &str) -> String {
        OutputBuffer::new(text).text
    }

    pub(super) fn bounded_input(text: &str) -> String {
        text.chars().take(MAX_INPUT_CHARS).collect()
    }

    pub(super) fn state_text(state: &PopupState) -> &str {
        match state {
            PopupState::Loading => "Translating…",
            PopupState::Streaming(output) | PopupState::Completed(output) => &output.text,
            PopupState::LocalError(message) => message,
        }
    }

    fn set_control_text(hwnd: HWND, text: &str) {
        let mut value: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            let _ = SetWindowTextW(hwnd, PCWSTR(value.as_mut_ptr()));
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) enum MarkdownStyle {
        Heading(u32),
        Bold,
        Italic,
        Strike,
        Code,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) struct FormatSpan {
        pub(super) start: usize,
        pub(super) end: usize,
        pub(super) style: MarkdownStyle,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub(super) struct RenderedMarkdown {
        pub(super) text: String,
        pub(super) spans: Vec<FormatSpan>,
    }

    fn append_inline(source: &str, output: &mut String, spans: &mut Vec<FormatSpan>) {
        let mut index = 0;
        while index < source.len() {
            let rest = &source[index..];
            if let Some(close_label) = rest
                .strip_prefix('[')
                .and_then(|value| value.find("](").map(|end| (value, end)))
            {
                let (value, label_end) = close_label;
                if let Some(close_url) = value[label_end + 2..].find(')') {
                    output.push_str(&value[..label_end]);
                    output.push_str(" (");
                    output.push_str(&value[label_end + 2..label_end + 2 + close_url]);
                    output.push(')');
                    index += label_end + close_url + 4;
                    continue;
                }
            }
            let marker = if rest.starts_with("**") || rest.starts_with("__") {
                Some((&rest[..2], MarkdownStyle::Bold))
            } else if rest.starts_with("~~") {
                Some((&rest[..2], MarkdownStyle::Strike))
            } else if rest.starts_with('`') {
                Some((&rest[..1], MarkdownStyle::Code))
            } else if rest.starts_with('*') || rest.starts_with('_') {
                Some((&rest[..1], MarkdownStyle::Italic))
            } else {
                None
            };
            let Some((delimiter, style)) = marker else {
                let ch = rest.chars().next().unwrap();
                output.push(ch);
                index += ch.len_utf8();
                continue;
            };
            let body_start = index + delimiter.len();
            let Some(close_rel) = source[body_start..].find(delimiter) else {
                output.push_str(delimiter);
                index = body_start;
                continue;
            };
            let body_end = body_start + close_rel;
            if body_end == body_start {
                output.push_str(delimiter);
                index = body_start;
                continue;
            }
            let start = output.encode_utf16().count();
            append_inline(&source[body_start..body_end], output, spans);
            let end = output.encode_utf16().count();
            spans.push(FormatSpan { start, end, style });
            index = body_end + delimiter.len();
        }
    }

    pub(super) fn render_markdown(source: &str) -> RenderedMarkdown {
        let mut text = String::new();
        let mut spans = Vec::new();
        let mut fenced = false;
        let mut emitted_line = false;
        for line in source.lines() {
            if line.trim_start().starts_with("```") {
                fenced = !fenced;
                continue;
            }
            if emitted_line {
                text.push('\n');
            }
            emitted_line = true;
            let trimmed = line.trim_start();
            let (content, style, prefix) = if fenced {
                (line, Some(MarkdownStyle::Code), "")
            } else if let Some(content) = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
            {
                (content, None, "• ")
            } else if let Some((hashes, content)) = trimmed.split_once(' ') {
                if hashes.chars().all(|c| c == '#') && (1..=6).contains(&hashes.len()) {
                    (
                        content,
                        Some(MarkdownStyle::Heading(hashes.len() as u32)),
                        "",
                    )
                } else {
                    (line, None, "")
                }
            } else if let Some(content) = trimmed.strip_prefix("> ") {
                (content, Some(MarkdownStyle::Italic), "│ ")
            } else if let Some(dot) = trimmed.find(". ") {
                if dot > 0 && trimmed[..dot].chars().all(|c| c.is_ascii_digit()) {
                    (&trimmed[dot + 2..], None, &trimmed[..dot + 2])
                } else {
                    (line, None, "")
                }
            } else if trimmed.chars().all(|c| c == '-' || c == '*' || c == '_')
                && trimmed.len() >= 3
            {
                ("────────", None, "")
            } else {
                (line, None, "")
            };
            text.push_str(prefix);
            let start = text.encode_utf16().count();
            append_inline(content, &mut text, &mut spans);
            let end = text.encode_utf16().count();
            if let Some(style) = style {
                spans.push(FormatSpan { start, end, style });
            }
        }
        RenderedMarkdown { text, spans }
    }

    fn set_rich_format(hwnd: HWND, span: FormatSpan) {
        let (mask, effects, height) = match span.style {
            MarkdownStyle::Bold => (CFM_BOLD, CFE_BOLD, 0),
            MarkdownStyle::Italic => (CFM_ITALIC, CFE_ITALIC, 0),
            MarkdownStyle::Strike => (CFM_STRIKEOUT, CFE_STRIKEOUT, 0),
            MarkdownStyle::Code => (CFM_SIZE, CFE_EFFECTS(0), 190),
            MarkdownStyle::Heading(level) => (CFM_SIZE, CFE_EFFECTS(0), 280 - (level as i32 * 20)),
        };
        let format = CHARFORMATW {
            cbSize: std::mem::size_of::<CHARFORMATW>() as u32,
            dwMask: mask,
            dwEffects: effects,
            yHeight: height,
            ..Default::default()
        };
        unsafe {
            let _ = SendMessageW(
                hwnd,
                EM_SETSEL,
                Some(WPARAM(span.start)),
                Some(LPARAM(span.end as isize)),
            );
            let _ = SendMessageW(
                hwnd,
                EM_SETCHARFORMAT,
                Some(WPARAM(SCF_SELECTION)),
                Some(LPARAM((&format as *const CHARFORMATW) as isize)),
            );
        }
    }

    fn reset_rich_format(hwnd: HWND, utf16_len: usize) {
        let mut format = CHARFORMATW {
            cbSize: std::mem::size_of::<CHARFORMATW>() as u32,
            dwMask: CFM_BOLD
                | CFM_ITALIC
                | CFM_STRIKEOUT
                | CFM_SIZE
                | CFM_COLOR
                | CFM_FACE
                | CFM_CHARSET,
            dwEffects: windows::Win32::UI::Controls::RichEdit::CFE_EFFECTS(0),
            yHeight: BASE_FONT_HEIGHT_TWIPS,
            crTextColor: POPUP_TEXT,
            bCharSet: FONT_CHARSET(1), // DEFAULT_CHARSET
            bPitchAndFamily: 0x20,     // FF_SWISS
            ..Default::default()
        };
        let face: Vec<u16> = "Segoe UI".encode_utf16().collect();
        format.szFaceName[..face.len()].copy_from_slice(&face);
        unsafe {
            let _ = SendMessageW(
                hwnd,
                EM_SETSEL,
                Some(WPARAM(0)),
                Some(LPARAM(utf16_len as isize)),
            );
            let _ = SendMessageW(
                hwnd,
                EM_SETCHARFORMAT,
                Some(WPARAM(SCF_SELECTION)),
                Some(LPARAM((&format as *const CHARFORMATW) as isize)),
            );
        }
    }

    pub(super) fn set_output(hwnd: HWND, raw: &str, markdown: bool) {
        // RichEdit automatically follows the caret when text is replaced. The
        // old implementation explicitly selected the final character after
        // every delta, which made the popup jump to the last line and caused a
        // visible repaint flash. Preserve the reader's viewport while doing a
        // single redraw-suppressed synchronization instead.
        let first_visible_line = unsafe {
            SendMessageW(
                hwnd,
                EM_GETFIRSTVISIBLELINE,
                Some(WPARAM(0)),
                Some(LPARAM(0)),
            )
            .0 as i32
        };
        // Format the accumulated Markdown synchronously for every response
        // update. Loading and local errors remain literal. The raw buffer is
        // still the source for Copy; only the visible RichEdit surface gets
        // this projection. Unclosed delimiters remain literal until a later
        // delta completes them.
        let rendered = if markdown {
            render_markdown(raw)
        } else {
            RenderedMarkdown {
                text: raw.to_owned(),
                spans: Vec::new(),
            }
        };
        let utf16_len = rendered.text.encode_utf16().count();
        unsafe {
            let _ = SendMessageW(hwnd, WM_SETREDRAW, Some(WPARAM(0)), Some(LPARAM(0)));
            let _ = SendMessageW(
                hwnd,
                EM_EXLIMITTEXT,
                Some(WPARAM(0)),
                Some(LPARAM(MAX_OUTPUT_UTF16_UNITS as isize)),
            );
        }
        set_control_text(hwnd, &rendered.text);
        reset_rich_format(hwnd, utf16_len);
        for span in rendered.spans {
            set_rich_format(hwnd, span);
        }
        unsafe {
            // Reset the caret before restoring the viewport. Both operations
            // must happen while redraw is still disabled; otherwise RichEdit
            // can briefly paint the end of the response between them.
            let _ = SendMessageW(hwnd, EM_SETSEL, Some(WPARAM(0)), Some(LPARAM(0)));
            let current_first_line = SendMessageW(
                hwnd,
                EM_GETFIRSTVISIBLELINE,
                Some(WPARAM(0)),
                Some(LPARAM(0)),
            )
            .0 as i32;
            let line_delta = first_visible_line.saturating_sub(current_first_line);
            if line_delta != 0 {
                let _ = SendMessageW(
                    hwnd,
                    EM_LINESCROLL,
                    Some(WPARAM(0)),
                    Some(LPARAM(line_delta as isize)),
                );
            }
            let _ = SendMessageW(hwnd, WM_SETREDRAW, Some(WPARAM(1)), Some(LPARAM(0)));
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }

    /// Hit-test coordinates received by WM_LBUTTONDOWN are client coordinates.
    /// The band stays an ordinary client region so this explicit routing path
    /// also works for the non-activating popup style.
    pub(super) fn drag_band_client_contains(
        point: Point,
        client_size: (i32, i32),
        dpi: u32,
        choosing_profile: bool,
    ) -> bool {
        if choosing_profile {
            return false;
        }
        let height = scale(DRAG_BAND_HEIGHT, dpi);
        point.x >= 0
            && point.x < client_size.0
            && point.y >= 0
            && point.y < client_size.1
            && point.y < height
    }

    #[cfg(test)]
    pub(super) fn mark_render_pending(in_native_move: bool, render_pending: &mut bool) -> bool {
        if in_native_move {
            *render_pending = true;
            true
        } else {
            false
        }
    }

    /// Accumulate state immediately, but present it at most once per 40ms
    /// burst. This keeps RichEdit/Markdown work off the hot streaming path.
    fn request_render(hwnd: HWND, data: &mut PopupData) -> bool {
        if data.in_native_move {
            data.render_pending = true;
            return false;
        }
        if !should_arm_render(data.in_native_move, data.render_timer_armed) {
            return false;
        }
        data.render_timer_armed = true;
        let armed = unsafe {
            windows::Win32::UI::WindowsAndMessaging::SetTimer(
                Some(hwnd),
                RENDER_TIMER_ID,
                RENDER_TIMER_MS,
                None,
            ) != 0
        };
        if !armed {
            data.render_timer_armed = false;
            return true;
        }
        false
    }

    pub(super) const fn should_arm_render(in_native_move: bool, timer_armed: bool) -> bool {
        !in_native_move && !timer_armed
    }

    pub(super) fn take_render_pending(render_pending: &mut bool) -> bool {
        std::mem::take(render_pending)
    }

    fn sync_controls(hwnd: HWND) {
        // Snapshot all values before calling Win32. RichEdit/parent messages
        // can re-enter the window procedure, so never hold PopupData's
        // fabricated mutable user-data borrow across synchronous sends.
        let Some((output, text, markdown, pin_button, pin_label)) = data_mut(hwnd).map(|data| {
            (
                data.output,
                state_text(&data.state).to_owned(),
                matches!(
                    &data.state,
                    PopupState::Streaming(_) | PopupState::Completed(_)
                ),
                data.buttons[3],
                if data.pinned { "Unpin" } else { "Pin" },
            )
        }) else {
            return;
        };
        if !output.0.is_null() {
            set_output(output, &text, markdown);
        }
        if !pin_button.0.is_null() {
            set_control_text(pin_button, pin_label);
        }
    }

    fn set_control_visible(hwnd: HWND, visible: bool) {
        if hwnd.0.is_null() {
            return;
        }
        unsafe {
            let _ = ShowWindow(hwnd, if visible { SW_SHOWNOACTIVATE } else { SW_HIDE });
        }
    }

    fn set_standard_controls_visible(data: &PopupData, visible: bool) {
        set_control_visible(data.input, visible);
        set_control_visible(data.context_input, visible);
        set_control_visible(data.output, visible);
        for button in data.buttons {
            set_control_visible(button, visible);
        }
    }

    fn clear_profile_buttons(data: &mut PopupData) {
        for button in data.profile_buttons.drain(..) {
            if !button.0.is_null() {
                unsafe {
                    let _ = DestroyWindow(button);
                }
            }
        }
        data.profile_button_widths.clear();
        data.choosing_profile = false;
    }

    fn leave_profile_chooser(data: &mut PopupData) {
        if !data.choosing_profile && data.profile_buttons.is_empty() {
            return;
        }
        clear_profile_buttons(data);
        data.profile_labels.clear();
        set_standard_controls_visible(data, true);
    }

    fn child_menu(id: usize) -> HMENU {
        HMENU(id as *mut core::ffi::c_void)
    }

    fn apply_dark_scrollbar_theme(hwnd: HWND) {
        if hwnd.0.is_null() {
            return;
        }
        // This is a best-effort Windows theme hint. Older systems may reject
        // the dark Explorer theme; the control remains functional and uses
        // the explicit popup foreground/background colors in that case.
        unsafe {
            let _ = SetWindowTheme(hwnd, w!("DarkMode_Explorer"), PCWSTR::null());
        }
    }

    fn create_controls(hwnd: HWND) {
        let Ok(instance) =
            (unsafe { windows::Win32::System::LibraryLoader::GetModuleHandleW(None) })
        else {
            return;
        };
        let dpi = dpi_for_window(hwnd);
        let Some(data) = data_mut(hwnd) else { return };
        {
            data.dpi = dpi;
            let body_height = -scale(13, dpi);
            // Buttons need a readable face for CJK labels; 9pt was too tight.
            let label_height = -scale(12, dpi);
            let face = w!("Segoe UI");
            data.fonts = [
                unsafe {
                    CreateFontW(
                        body_height,
                        0,
                        0,
                        0,
                        400,
                        0,
                        0,
                        0,
                        FONT_CHARSET(1),
                        FONT_OUTPUT_PRECISION(0),
                        FONT_CLIP_PRECISION(0),
                        FONT_QUALITY(5),
                        0,
                        face,
                    )
                },
                unsafe {
                    CreateFontW(
                        label_height,
                        0,
                        0,
                        0,
                        600,
                        0,
                        0,
                        0,
                        FONT_CHARSET(1),
                        FONT_OUTPUT_PRECISION(0),
                        FONT_CLIP_PRECISION(0),
                        FONT_QUALITY(5),
                        0,
                        face,
                    )
                },
            ];
            let edit_style = WS_CHILD
                | WS_VISIBLE
                | WS_TABSTOP
                | WS_VSCROLL
                | WINDOW_STYLE((ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL | ES_NOHIDESEL) as u32);
            let input_style = WS_CHILD
                | WS_VISIBLE
                | WS_TABSTOP
                | WINDOW_STYLE((ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL) as u32);
            data.input = unsafe {
                CreateWindowExW(
                    Default::default(),
                    w!("EDIT"),
                    w!(""),
                    input_style,
                    0,
                    0,
                    1,
                    1,
                    Some(hwnd),
                    Some(child_menu(INPUT_ID)),
                    Some(HINSTANCE(instance.0)),
                    None,
                )
            }
            .unwrap_or_default();
            data.context_input = unsafe {
                CreateWindowExW(
                    Default::default(),
                    w!("EDIT"),
                    w!(""),
                    input_style,
                    0,
                    0,
                    1,
                    1,
                    Some(hwnd),
                    Some(child_menu(INPUT_ID)),
                    Some(HINSTANCE(instance.0)),
                    None,
                )
            }
            .unwrap_or_default();
            // msftedit.dll is part of Windows; loading it dynamically keeps the
            // resident independent of a bundled UI runtime. Older systems fall
            // back to the standard EDIT control below.
            let rich_edit_module = unsafe { LoadLibraryW(w!("msftedit.dll")).ok() };
            let rich_class = rich_edit_module.map(|_| RICH_EDIT_CLASS);
            data.output = unsafe {
                CreateWindowExW(
                    Default::default(),
                    rich_class.unwrap_or(w!("EDIT")),
                    w!(""),
                    edit_style,
                    0,
                    0,
                    1,
                    1,
                    Some(hwnd),
                    Some(child_menu(OUTPUT_ID)),
                    Some(HINSTANCE(instance.0)),
                    None,
                )
            }
            .unwrap_or_default();
            if data.output.0.is_null() {
                if let Some(module) = rich_edit_module {
                    let _ = unsafe { FreeLibrary(module) };
                }
                data.rich_edit_module = windows::Win32::Foundation::HMODULE::default();
                data.output = unsafe {
                    CreateWindowExW(
                        Default::default(),
                        w!("EDIT"),
                        w!(""),
                        edit_style,
                        0,
                        0,
                        1,
                        1,
                        Some(hwnd),
                        Some(child_menu(OUTPUT_ID)),
                        Some(HINSTANCE(instance.0)),
                        None,
                    )
                    .unwrap_or_default()
                };
            } else if let Some(module) = rich_edit_module {
                data.rich_edit_module = module;
            }
            if !data.output.0.is_null() {
                unsafe {
                    // RichEdit does not consistently honor CTLCOLOR for its own
                    // document background, so set it once before the popup is
                    // presented. The fallback EDIT safely ignores this message.
                    let _ = SendMessageW(
                        data.output,
                        EM_SETBKGNDCOLOR,
                        Some(WPARAM(0)),
                        Some(LPARAM(POPUP_BG.0 as isize)),
                    );
                }
            }
            for control in [data.input, data.context_input] {
                if !control.0.is_null() {
                    unsafe {
                        let _ = SendMessageW(
                            control,
                            EM_SETBKGNDCOLOR,
                            Some(WPARAM(0)),
                            Some(LPARAM(POPUP_SECTION_BG.0 as isize)),
                        );
                        // 2px side margins keep the pane tight around the text
                        // without changing the 12pt font.
                        let _ = SendMessageW(
                            control,
                            EM_SETMARGINS,
                            Some(WPARAM(0x0003)),
                            Some(LPARAM(((2i32) << 16 | 2i32) as isize)),
                        );
                    }
                }
            }
            apply_dark_scrollbar_theme(data.input);
            apply_dark_scrollbar_theme(data.context_input);
            apply_dark_scrollbar_theme(data.output);

            let labels = ["▣  Copy", "↻  Retry", "☵  Prompt", "⌖  Pin", "×  Close"];
            let ids = [COPY_ID, RETRY_ID, PROMPT_ID, PIN_ID, CLOSE_ID];
            for ((button, label), id) in data.buttons.iter_mut().zip(labels).zip(ids) {
                let mut value: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
                *button = unsafe {
                    CreateWindowExW(
                        Default::default(),
                        w!("BUTTON"),
                        PCWSTR(value.as_mut_ptr()),
                        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(OWNER_DRAW_BUTTON_STYLE),
                        0,
                        0,
                        1,
                        1,
                        Some(hwnd),
                        Some(child_menu(id)),
                        Some(HINSTANCE(instance.0)),
                        None,
                    )
                }
                .unwrap_or_default();
            }
            for control in [data.input, data.context_input, data.output] {
                if !control.0.is_null() && !data.fonts[0].0.is_null() {
                    unsafe {
                        let _ = SendMessageW(
                            control,
                            0x0030,
                            Some(WPARAM(data.fonts[0].0 as usize)),
                            Some(LPARAM(1)),
                        );
                    }
                }
            }
            for button in data.buttons {
                if !button.0.is_null() && !data.fonts[1].0.is_null() {
                    unsafe {
                        let _ = SendMessageW(
                            button,
                            0x0030,
                            Some(WPARAM(data.fonts[1].0 as usize)),
                            Some(LPARAM(1)),
                        );
                    }
                }
            }
        }
        sync_controls(hwnd);
        layout_children(hwnd, dpi);
    }

    fn popup_brush(color: COLORREF) -> HBRUSH {
        // Brushes are short-lived, used only for the current native paint
        // callback, and released by the caller after FillRect.
        unsafe { CreateSolidBrush(color) }
    }

    fn popup_background_brush() -> HBRUSH {
        static BRUSH: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
        let raw = *BRUSH.get_or_init(|| unsafe { CreateSolidBrush(POPUP_BG).0 as usize });
        HBRUSH(raw as *mut core::ffi::c_void)
    }

    fn popup_section_brush() -> HBRUSH {
        static BRUSH: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
        let raw = *BRUSH.get_or_init(|| unsafe { CreateSolidBrush(POPUP_SECTION_BG).0 as usize });
        HBRUSH(raw as *mut core::ffi::c_void)
    }

    fn draw_button(item: &DRAWITEMSTRUCT) {
        if item.CtlType != ODT_BUTTON {
            return;
        }
        let selected = item.itemState.0 & 0x0001 != 0; // ODS_SELECTED
        let disabled = item.itemState.0 & 0x0004 != 0; // ODS_DISABLED
        let focused = item.itemState.0 & 0x0010 != 0; // ODS_FOCUS
        let state = if disabled {
            ButtonVisualState::Disabled
        } else if selected {
            ButtonVisualState::Pressed
        } else if focused {
            ButtonVisualState::Focused
        } else {
            ButtonVisualState::Normal
        };
        // Primary action (Copy) uses the accent fill so the action row has
        // hierarchy instead of five identical grey pills.
        let mut primary_label = [0u16; 16];
        let primary_len = unsafe { GetWindowTextW(item.hwndItem, &mut primary_label) } as usize;
        let is_primary = String::from_utf16_lossy(&primary_label[..primary_len]).contains("Copy");
        let fill = if is_primary {
            match state {
                ButtonVisualState::Normal | ButtonVisualState::Focused => POPUP_ACCENT_DIM,
                ButtonVisualState::Pressed => POPUP_ACCENT,
                ButtonVisualState::Disabled => POPUP_BUTTON_DISABLED,
            }
        } else {
            button_fill(state)
        };
        let brush = popup_brush(fill);
        let border = popup_brush(if is_primary {
            fill
        } else if focused {
            POPUP_ACCENT
        } else {
            POPUP_BUTTON_STROKE
        });
        let mut rect = item.rcItem;
        let radius = ((rect.bottom - rect.top) / 2).clamp(6, 10);
        let region = unsafe {
            CreateRoundRectRgn(
                rect.left,
                rect.top,
                rect.right + 1,
                rect.bottom + 1,
                radius,
                radius,
            )
        };
        unsafe {
            if !region.0.is_null() {
                let _ = FillRgn(item.hDC, region, brush);
                let _ = FrameRgn(item.hDC, region, border, 1, 1);
                let _ = DeleteObject(region.into());
            } else {
                let _ = FillRect(item.hDC, &rect, brush);
                let _ = FrameRect(item.hDC, &rect, border);
            }
            let _ = DeleteObject(brush.into());
            let _ = DeleteObject(border.into());
            let mut text = [0u16; 128];
            let length = GetWindowTextW(item.hwndItem, &mut text) as i32;
            let font = SendMessageW(item.hwndItem, 0x0031, Some(WPARAM(0)), Some(LPARAM(0)));
            let old_font = if font.0 != 0 {
                Some(SelectObject(item.hDC, HGDIOBJ(font.0 as *mut _)))
            } else {
                None
            };
            // Transparent bk keeps glyph edges clean; opaque fill was showing
            // as a misaligned slab around CJK text.
            let old_bk = SetBkMode(item.hDC, TRANSPARENT);
            SetTextColor(item.hDC, POPUP_BUTTON_TEXT);
            // Inset the text rect so DrawText centers the face, not the
            // full owner-rect including the 1px frame.
            let mut text_rect = rect;
            text_rect.left += 2;
            text_rect.right -= 2;
            let _ = DrawTextW(
                item.hDC,
                &mut text[..length.max(0) as usize],
                &mut text_rect,
                // DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX
                DRAW_TEXT_FORMAT(0x0001 | 0x0020 | 0x0100 | 0x0800),
            );
            let _ = SetBkMode(item.hDC, BACKGROUND_MODE(old_bk as u32));
            if focused {
                rect.left += 4;
                rect.top += 4;
                rect.right -= 4;
                rect.bottom -= 4;
                let _ = DrawFocusRect(item.hDC, &rect);
            }
            if let Some(old_font) = old_font {
                let _ = SelectObject(item.hDC, old_font);
            }
        }
    }

    fn paint_round_rect(
        hdc: windows::Win32::Graphics::Gdi::HDC,
        rect: RECT,
        radius: i32,
        fill: COLORREF,
        border: Option<COLORREF>,
    ) {
        let brush = popup_brush(fill);
        let region = unsafe {
            CreateRoundRectRgn(
                rect.left,
                rect.top,
                rect.right + 1,
                rect.bottom + 1,
                radius,
                radius,
            )
        };
        unsafe {
            if !region.0.is_null() {
                let _ = FillRgn(hdc, region, brush);
                if let Some(color) = border {
                    let frame = popup_brush(color);
                    let _ = FrameRgn(hdc, region, frame, 1, 1);
                    let _ = DeleteObject(frame.into());
                }
                let _ = DeleteObject(region.into());
            } else {
                let _ = FillRect(hdc, &rect, brush);
            }
            let _ = DeleteObject(brush.into());
        }
    }

    fn paint_section_label(
        hdc: windows::Win32::Graphics::Gdi::HDC,
        font: Option<HFONT>,
        text: &str,
        rect: RECT,
        color: COLORREF,
    ) {
        let mut label = rect;
        let old_font = font.map(|font| unsafe { SelectObject(hdc, HGDIOBJ(font.0)) });
        unsafe {
            let old_bk = SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, color);
            let mut buffer: Vec<u16> = text.encode_utf16().collect();
            let _ = DrawTextW(
                hdc,
                &mut buffer,
                &mut label,
                // DT_VCENTER | DT_SINGLELINE
                DRAW_TEXT_FORMAT(0x0020 | 0x0100),
            );
            let _ = SetBkMode(hdc, BACKGROUND_MODE(old_bk as u32));
        }
        if let Some(old_font) = old_font {
            unsafe {
                let _ = SelectObject(hdc, old_font);
            }
        }
    }

    fn paint_surface(hwnd: HWND, hdc: windows::Win32::Graphics::Gdi::HDC, dpi: u32) {
        let mut client = RECT::default();
        if unsafe { GetClientRect(hwnd, &mut client) }.is_err() {
            return;
        }
        let background = popup_brush(POPUP_BG);
        unsafe {
            let _ = FillRect(hdc, &client, background);
            let _ = DeleteObject(background.into());
        }
        let border = popup_brush(POPUP_BORDER);
        unsafe {
            let _ = FrameRect(hdc, &client, border);
            let _ = DeleteObject(border.into());
        }
        let layout = compute_layout(client.right, client.bottom, dpi, 5);
        let radius = popup_corner_radius(dpi).max(12);
        let left = layout.margin;
        let right = client.right - layout.margin;
        let label_font = data_mut(hwnd)
            .map(|data| data.fonts[1])
            .filter(|font| !font.0.is_null());

        // Header band with icon chip, title, subtitle.
        paint_round_rect(
            hdc,
            RECT {
                left: 0,
                top: 0,
                right: client.right,
                bottom: layout.header_height,
            },
            0,
            POPUP_HEADER_BG,
            None,
        );
        paint_round_rect(
            hdc,
            RECT {
                left,
                top: scale(18, dpi),
                right: left + scale(36, dpi),
                bottom: scale(54, dpi),
            },
            scale(10, dpi),
            POPUP_ACCENT,
            None,
        );
        paint_section_label(
            hdc,
            label_font,
            "文",
            RECT {
                left,
                top: scale(14, dpi),
                right: left + scale(36, dpi),
                bottom: scale(50, dpi),
            },
            POPUP_LOGO_INK,
        );
        paint_section_label(
            hdc,
            label_font,
            "Selection Translate",
            RECT {
                left: left + scale(48, dpi),
                top: scale(12, dpi),
                right,
                bottom: scale(36, dpi),
            },
            POPUP_TITLE,
        );
        paint_section_label(
            hdc,
            label_font,
            "Translate your selection with context",
            RECT {
                left: left + scale(48, dpi),
                top: scale(36, dpi),
                right,
                bottom: scale(54, dpi),
            },
            POPUP_MUTED,
        );

        // Input card (Target + Context) and Result card.
        paint_round_rect(
            hdc,
            RECT {
                left,
                top: layout.input_card_top,
                right,
                bottom: layout.input_card_bottom,
            },
            radius,
            POPUP_SECTION_BG,
            Some(POPUP_CARD_BORDER),
        );
        paint_round_rect(
            hdc,
            RECT {
                left,
                top: layout.output_card_top,
                right,
                bottom: layout.output_card_bottom,
            },
            radius,
            POPUP_HEADER_BG,
            Some(POPUP_RESULT_BORDER),
        );
        let inset = scale(4, dpi);
        paint_round_rect(
            hdc,
            RECT {
                left: left + inset,
                top: layout.target_top - inset,
                right: right - inset,
                bottom: layout.target_top + layout.target_height + inset,
            },
            scale(9, dpi),
            POPUP_FIELD_BG,
            Some(POPUP_FIELD_BORDER),
        );
        paint_round_rect(
            hdc,
            RECT {
                left: left + inset,
                top: layout.context_top - inset,
                right: right - inset,
                bottom: layout.context_top + layout.context_height + inset,
            },
            scale(9, dpi),
            POPUP_FIELD_BG,
            Some(POPUP_FIELD_BORDER),
        );

        paint_section_label(
            hdc,
            label_font,
            "◎  Target",
            RECT {
                left: left + scale(12, dpi),
                top: layout.target_label_top,
                right,
                bottom: layout.target_label_top + layout.label_height,
            },
            POPUP_LABEL,
        );
        paint_section_label(
            hdc,
            label_font,
            "▤  Context",
            RECT {
                left: left + scale(12, dpi),
                top: layout.context_label_top,
                right,
                bottom: layout.context_label_top + layout.label_height,
            },
            POPUP_LABEL,
        );
        paint_section_label(
            hdc,
            label_font,
            "✦  Result",
            RECT {
                left: left + scale(12, dpi),
                top: layout.output_label_top,
                right,
                bottom: layout.output_label_top + layout.label_height,
            },
            POPUP_RESULT_ICON,
        );
    }

    fn copy_text(text: &str) {
        // Allocate and fill before EmptyClipboard, so allocation/encoding
        // failure cannot erase the user's existing clipboard.
        let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let memory =
            unsafe { GlobalAlloc(GMEM_MOVEABLE, utf16.len() * std::mem::size_of::<u16>()) };
        let Ok(memory) = memory else { return };
        let destination = unsafe { GlobalLock(memory).cast::<u16>() };
        if destination.is_null() {
            unsafe {
                let _ = GlobalFree(Some(memory));
            }
            return;
        }
        unsafe {
            std::ptr::copy_nonoverlapping(utf16.as_ptr(), destination, utf16.len());
            let _ = GlobalUnlock(memory);
            if OpenClipboard(None).is_err() {
                let _ = GlobalFree(Some(memory));
                return;
            }
            if EmptyClipboard().is_err() {
                let _ = GlobalFree(Some(memory));
                let _ = CloseClipboard();
                return;
            }
            // CF_UNICODETEXT = 13. Windows takes ownership on success.
            if SetClipboardData(13, Some(HANDLE(memory.0))).is_err() {
                let _ = GlobalFree(Some(memory));
            }
            let _ = CloseClipboard();
        }
    }

    fn post_owner(hwnd: HWND, message: u32) {
        post_owner_with_value(hwnd, message, 0);
    }

    fn post_owner_with_value(hwnd: HWND, message: u32, value: usize) {
        if let Some(data) = data_mut(hwnd) {
            let target = data.callback_target;
            let popup_id = data.id;
            if target.0.is_null() {
                return;
            }
            unsafe {
                let _ = PostMessageW(
                    Some(target),
                    message,
                    WPARAM(value),
                    LPARAM(popup_id as isize),
                );
            }
        }
    }

    fn close_by_user(hwnd: HWND) {
        runtime_trace::record("popup_user_close");
        if let Some(data) = data_mut(hwnd) {
            data.notify_owner = true;
        }
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }

    fn handle_command(hwnd: HWND, id: usize) {
        if id == PROFILE_MORE_ID {
            show_more_profiles(hwnd);
            return;
        }
        let profile_choice =
            data_mut(hwnd).and_then(|data| profile_choice_index(id, data.profile_labels.len()));
        if let Some(index) = profile_choice {
            post_owner_with_value(hwnd, POPUP_PROFILE_SELECTED, index);
            return;
        }
        match id {
            COPY_ID => {
                let text = data_mut(hwnd)
                    .map(|data| state_text(&data.state).to_owned())
                    .unwrap_or_default();
                copy_text(&text);
            }
            RETRY_ID => post_owner(hwnd, POPUP_RETRY),
            PROMPT_ID => post_owner(hwnd, POPUP_PROMPT),
            PIN_ID => {
                let button = if let Some(data) = data_mut(hwnd) {
                    data.pinned = !data.pinned;
                    Some((data.buttons[3], if data.pinned { "Unpin" } else { "Pin" }))
                } else {
                    None
                };
                if let Some((button, label)) = button {
                    set_control_text(button, label);
                }
            }
            CLOSE_ID => close_by_user(hwnd),
            _ => {}
        }
    }

    fn show_more_profiles(hwnd: HWND) {
        let labels = data_mut(hwnd)
            .map(|data| data.profile_labels.clone())
            .unwrap_or_default();
        if labels.len() <= INLINE_PROFILE_LIMIT {
            return;
        }
        let Ok(menu) = (unsafe { CreatePopupMenu() }) else {
            return;
        };
        let mut appended = true;
        for (index, label) in labels.iter().enumerate().skip(INLINE_PROFILE_LIMIT) {
            let mut value: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
            if unsafe {
                AppendMenuW(
                    menu,
                    MF_STRING,
                    PROFILE_CHOICE_ID_START + index,
                    PCWSTR(value.as_mut_ptr()),
                )
            }
            .is_err()
            {
                appended = false;
                break;
            }
        }
        if appended {
            let mut rect = RECT::default();
            if unsafe { GetWindowRect(hwnd, &mut rect) }.is_ok() {
                unsafe {
                    let _ = SetForegroundWindow(hwnd);
                    let command = TrackPopupMenu(
                        menu,
                        TPM_LEFTALIGN | TPM_TOPALIGN | TPM_RETURNCMD,
                        rect.left,
                        rect.bottom,
                        Some(0),
                        hwnd,
                        None,
                    );
                    let command_id = command.0 as usize;
                    if let Some(index) = profile_choice_index(command_id, labels.len()) {
                        post_owner_with_value(hwnd, POPUP_PROFILE_SELECTED, index);
                    }
                }
            }
        }
        unsafe {
            let _ = DestroyMenu(menu);
        }
    }

    pub(super) fn profile_choice_index(command_id: usize, profile_count: usize) -> Option<usize> {
        command_id
            .checked_sub(PROFILE_CHOICE_ID_START)
            .filter(|index| *index < profile_count)
    }

    unsafe extern "system" fn popup_wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_PAINT => {
                let mut paint = windows::Win32::Graphics::Gdi::PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut paint);
                let dpi = data_mut(hwnd).map(|data| data.dpi).unwrap_or(DEFAULT_DPI);
                paint_surface(hwnd, hdc, dpi);
                let _ = EndPaint(hwnd, &paint);
                return LRESULT(0);
            }
            WM_ERASEBKGND => {
                let mut rect = RECT::default();
                if GetClientRect(hwnd, &mut rect).is_ok() {
                    let hdc = windows::Win32::Graphics::Gdi::HDC(wparam.0 as *mut _);
                    let _ = FillRect(hdc, &rect, popup_background_brush());
                }
                return LRESULT(1);
            }
            WM_DRAWITEM => {
                if lparam.0 != 0 {
                    draw_button(&*(lparam.0 as *const DRAWITEMSTRUCT));
                }
                return LRESULT(1);
            }
            // EDIT/RichEdit ask their parent for the background and text
            // colors. Return the process-lifetime class brush: Windows keeps
            // using it after this callback. Distinguish the muted input from
            // the primary result by child HWND because both read-only controls
            // may send WM_CTLCOLORSTATIC.
            0x0133 | 0x0135 | 0x0138 => {
                let hdc = windows::Win32::Graphics::Gdi::HDC(wparam.0 as *mut _);
                let child = HWND(lparam.0 as *mut core::ffi::c_void);
                let muted = data_mut(hwnd)
                    .is_some_and(|data| data.input == child || data.context_input == child);
                SetBkColor(hdc, POPUP_SECTION_BG);
                SetTextColor(hdc, if muted { POPUP_MUTED } else { POPUP_TEXT });
                return LRESULT(popup_section_brush().0 as isize);
            }
            // The popup is initially passive. A Ctrl-click on its background
            // is the explicit user activation path for keyboard navigation.
            WM_LBUTTONDOWN if (wparam.0 & 0x0008) != 0 => {
                if let Some(data) = data_mut(hwnd) {
                    let _ = SetForegroundWindow(hwnd);
                    let _ = SetFocus(Some(data.output));
                }
                return LRESULT(0);
            }
            WM_LBUTTONDOWN => {
                let point = Point {
                    x: (lparam.0 as u32 & 0xffff) as i16 as i32,
                    y: ((lparam.0 as u32 >> 16) & 0xffff) as i16 as i32,
                };
                let mut client = RECT::default();
                if GetClientRect(hwnd, &mut client).is_ok()
                    && data_mut(hwnd).is_some_and(|data| {
                        drag_band_client_contains(
                            point,
                            (client.right, client.bottom),
                            data.dpi,
                            data.choosing_profile,
                        )
                    })
                {
                    // Child controls do not receive this message because the
                    // band is intentionally left empty. Explicitly asking
                    // DefWindowProc to process HTCAPTION makes dragging work
                    // consistently even when WM_NCHITTEST is bypassed.
                    let _ = ReleaseCapture();
                    let _ = SendMessageW(
                        hwnd,
                        WM_NCLBUTTONDOWN,
                        Some(WPARAM(2)), // HTCAPTION
                        Some(LPARAM(0)),
                    );
                    return LRESULT(0);
                }
            }
            WM_NCHITTEST => {
                // Custom resize edges on a frameless popup: no OS thick frame,
                // so the dark surface stays intact.
                let mut point = windows::Win32::Foundation::POINT {
                    x: (lparam.0 as u32 & 0xffff) as i16 as i32,
                    y: ((lparam.0 as u32 >> 16) & 0xffff) as i16 as i32,
                };
                unsafe {
                    let _ = windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut point);
                }
                let mut client = RECT::default();
                if GetClientRect(hwnd, &mut client).is_ok() {
                    let border = scale(
                        RESIZE_BORDER,
                        data_mut(hwnd).map(|data| data.dpi).unwrap_or(DEFAULT_DPI),
                    );
                    let choosing = data_mut(hwnd).is_some_and(|data| data.choosing_profile);
                    let hit = resize_hit_test(
                        Point {
                            x: point.x,
                            y: point.y,
                        },
                        client.right,
                        client.bottom,
                        border,
                        choosing,
                    );
                    if hit.0 != HTCLIENT as isize {
                        return hit;
                    }
                }
            }
            WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
            WM_KEYDOWN if wparam.0 as u32 == VK_ESCAPE.0 as u32 => {
                close_by_user(hwnd);
                return LRESULT(0);
            }
            WM_CLOSE => {
                runtime_trace::record("popup_wm_close");
                close_by_user(hwnd);
                return LRESULT(0);
            }
            WM_COMMAND => {
                handle_command(hwnd, wparam.0 & 0xffff);
                return LRESULT(0);
            }
            WM_CREATE => {
                create_controls(hwnd);
                return LRESULT(0);
            }
            WM_SIZE => {
                // SIZE_MINIMIZED == 1. Relayout on normal and maximized sizes.
                if wparam.0 != 1 {
                    let width = (lparam.0 as u32 & 0xffff) as i16 as i32;
                    let height = ((lparam.0 as u32 >> 16) & 0xffff) as i16 as i32;
                    if width > 0 && height > 0 {
                        let choosing = data_mut(hwnd).is_some_and(|data| data.choosing_profile);
                        let dpi = data_mut(hwnd).map(|data| data.dpi).unwrap_or(DEFAULT_DPI);
                        // Ignore chooser strip sizes when recording the durable
                        // result-popup size.
                        if !choosing {
                            let min = min_popup_size(dpi);
                            if width >= min.0 && height >= min.1 {
                                if let Some(data) = data_mut(hwnd) {
                                    data.window_size = Some((width, height));
                                }
                            }
                        }
                        layout_children(hwnd, dpi);
                        apply_round_region(hwnd, (width, height), dpi);
                        let _ = InvalidateRect(Some(hwnd), None, true);
                    }
                }
                return LRESULT(0);
            }
            WM_GETMINMAXINFO => {
                if lparam.0 != 0 {
                    let dpi = data_mut(hwnd).map(|data| data.dpi).unwrap_or(DEFAULT_DPI);
                    let (min_width, min_height) = min_popup_size(dpi);
                    let info = &mut *(lparam.0 as *mut MINMAXINFO);
                    info.ptMinTrackSize.x = min_width;
                    info.ptMinTrackSize.y = min_height;
                }
                return LRESULT(0);
            }
            WM_DPICHANGED => {
                let dpi = (wparam.0 & 0xffff) as u32;
                let dpi = if dpi == 0 { DEFAULT_DPI } else { dpi };
                if lparam.0 != 0 {
                    let suggested = &*(lparam.0 as *const RECT);
                    let size = (
                        (suggested.right - suggested.left).max(1),
                        (suggested.bottom - suggested.top).max(1),
                    );
                    let suggested_origin = Point {
                        x: suggested.left,
                        y: suggested.top,
                    };
                    let origin = work_area_for(suggested_origin)
                        .map(|area| clamped_origin(suggested_origin, size, area))
                        .unwrap_or(suggested_origin);
                    let _ = SetWindowPos(
                        hwnd,
                        None,
                        origin.x,
                        origin.y,
                        size.0,
                        size.1,
                        SWP_NOACTIVATE | SWP_NOZORDER,
                    );
                    if let Some(data) = data_mut(hwnd) {
                        data.dpi = dpi;
                        data.anchor = origin;
                    }
                    layout_children(hwnd, dpi);
                    apply_round_region(hwnd, size, dpi);
                    let _ = InvalidateRect(Some(hwnd), None, true);
                }
                return LRESULT(0);
            }
            WM_TIMER if wparam.0 == RENDER_TIMER_ID => {
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::KillTimer(
                        Some(hwnd),
                        RENDER_TIMER_ID,
                    );
                }
                let render = if let Some(data) = data_mut(hwnd) {
                    let was_armed = data.render_timer_armed;
                    data.render_timer_armed = false;
                    if !was_armed {
                        false
                    } else if data.in_native_move {
                        data.render_pending = true;
                        false
                    } else {
                        true
                    }
                } else {
                    false
                };
                if render {
                    sync_controls(hwnd);
                }
                return LRESULT(0);
            }
            WM_ENTERSIZEMOVE => {
                if let Some(data) = data_mut(hwnd) {
                    data.in_native_move = true;
                }
                return LRESULT(0);
            }
            WM_EXITSIZEMOVE => {
                let mut rect = RECT::default();
                if GetWindowRect(hwnd, &mut rect).is_ok() {
                    let size = (
                        (rect.right - rect.left).max(1),
                        (rect.bottom - rect.top).max(1),
                    );
                    let current = Point {
                        x: rect.left,
                        y: rect.top,
                    };
                    let origin = work_area_for(current)
                        .map(|area| clamped_origin(current, size, area))
                        .unwrap_or(current);
                    let _ = SetWindowPos(
                        hwnd,
                        None,
                        origin.x,
                        origin.y,
                        0,
                        0,
                        SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
                    );
                    if let Some(data) = data_mut(hwnd) {
                        data.anchor = origin;
                    }
                }
                // Finish/stream messages can arrive while the native move
                // loop owns the thread. Flush the accumulated state exactly
                // once, after the final position has settled.
                let render_pending = if let Some(data) = data_mut(hwnd) {
                    data.in_native_move = false;
                    let scheduled = data.render_timer_armed;
                    if scheduled {
                        unsafe {
                            let _ = windows::Win32::UI::WindowsAndMessaging::KillTimer(
                                Some(hwnd),
                                RENDER_TIMER_ID,
                            );
                        }
                        data.render_timer_armed = false;
                    }
                    take_render_pending(&mut data.render_pending) || scheduled
                } else {
                    false
                };
                if render_pending {
                    sync_controls(hwnd);
                }
                return LRESULT(0);
            }
            WM_DESTROY => {
                runtime_trace::record("popup_wm_destroy");
                let notify = data_mut(hwnd).map(|data| {
                    let notify = data.notify_owner;
                    data.notify_owner = false;
                    notify
                }) == Some(true);
                if notify {
                    post_owner(hwnd, POPUP_DISMISSED);
                }
                return LRESULT(0);
            }
            WM_NCCREATE => {
                let create =
                    &*(lparam.0 as *const windows::Win32::UI::WindowsAndMessaging::CREATESTRUCTW);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
            }
            WM_NCDESTROY => {
                runtime_trace::record("popup_wm_ncdestroy");
                let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut PopupData;
                if !pointer.is_null() {
                    let data = Box::from_raw(pointer);
                    for font in data.fonts {
                        if !font.0.is_null() {
                            let _ = DeleteObject(HGDIOBJ(font.0));
                        }
                    }
                    if !data.rich_edit_module.0.is_null() {
                        let _ = FreeLibrary(data.rich_edit_module);
                    }
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                }
            }
            _ => {}
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

#[cfg(windows)]
pub use windows_impl::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn delayed_provider_keeps_loading_state_until_output_completes() {
        use super::windows_impl::PopupState;

        let mut state = PopupState::Loading;
        // No timer or elapsed-time transition is attached to Loading. A slow
        // provider therefore leaves the surface in place until an explicit
        // delta or terminal event arrives.
        assert_eq!(state, PopupState::Loading);
        state.append("translated ");
        state.append("result");
        state.finish();

        assert_eq!(
            state,
            PopupState::Completed(windows_impl::OutputBuffer::new("translated result"))
        );
    }

    #[cfg(windows)]
    #[test]
    fn profile_choice_commands_map_only_to_visible_names() {
        use super::windows_impl::profile_choice_index;

        assert_eq!(profile_choice_index(999, 3), None);
        assert_eq!(profile_choice_index(1000, 3), Some(0));
        assert_eq!(profile_choice_index(1002, 3), Some(2));
        assert_eq!(profile_choice_index(1003, 3), None);
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "visual capture; run with --ignored to write popup_preview.bmp"]
    fn capture_result_popup_bitmap() {
        use super::windows_impl::Popup;
        use windows::core::w;
        use windows::Win32::Foundation::{HINSTANCE, RECT};
        use windows::Win32::Graphics::Gdi::{
            BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
            GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS,
            SRCCOPY,
        };
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, FindWindowW, GetWindowRect, WS_POPUP,
        };

        let instance = unsafe { GetModuleHandleW(None) }.expect("module");
        let parent = unsafe {
            CreateWindowExW(
                Default::default(),
                w!("STATIC"),
                w!(""),
                WS_POPUP,
                80,
                80,
                200,
                200,
                None,
                None,
                Some(HINSTANCE(instance.0)),
                None,
            )
        }
        .expect("parent");
        let mut popup =
            Popup::show(parent, 99, super::Point { x: 120, y: 120 }).expect("popup show");
        popup.set_input("annotation", Some("Corner radius is capped at 8px."));
        popup.set_text("# Translation\n\n• Chinese: 注释\n• English: annotation\n");
        // Pump a few messages so WM_PAINT can run.
        for _ in 0..20 {
            let mut msg = windows::Win32::UI::WindowsAndMessaging::MSG::default();
            while unsafe {
                windows::Win32::UI::WindowsAndMessaging::PeekMessageW(
                    &mut msg,
                    None,
                    0,
                    0,
                    windows::Win32::UI::WindowsAndMessaging::PM_REMOVE,
                )
                .as_bool()
            } {
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::TranslateMessage(&msg);
                    let _ = windows::Win32::UI::WindowsAndMessaging::DispatchMessageW(&msg);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let class: Vec<u16> = "SelectionTranslatePopup\0".encode_utf16().collect();
        let popup_hwnd = unsafe { FindWindowW(windows::core::PCWSTR(class.as_ptr()), None) }
            .expect("popup class window");
        assert!(!popup_hwnd.0.is_null(), "popup class window must exist");
        let mut rect = RECT::default();
        unsafe { GetWindowRect(popup_hwnd, &mut rect) }.expect("rect");
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        assert!(width >= 400, "result popup width {width}");
        assert!(
            height >= 300,
            "result popup height {height}, chooser pollution?"
        );

        let screen = unsafe { GetDC(Some(popup_hwnd)) };
        let mem = unsafe { CreateCompatibleDC(Some(screen)) };
        let bmp = unsafe { CreateCompatibleBitmap(screen, width, height) };
        let old = unsafe { SelectObject(mem, bmp.into()) };
        unsafe {
            let _ = BitBlt(mem, 0, 0, width, height, Some(screen), 0, 0, SRCCOPY);
        }
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: 0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        unsafe {
            let _ = GetDIBits(
                mem,
                bmp,
                0,
                height as u32,
                Some(pixels.as_mut_ptr() as *mut _),
                &mut info,
                DIB_RGB_COLORS,
            );
            SelectObject(mem, old);
            let _ = DeleteObject(bmp.into());
            let _ = DeleteDC(mem);
            ReleaseDC(Some(popup_hwnd), screen);
        }
        // Write 32-bit BI_RGB BMP (bottom-up).
        let out =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp/popup_preview.bmp");
        std::fs::create_dir_all(out.parent().unwrap()).ok();
        let mut file = std::fs::File::create(&out).expect("create bmp");
        use std::io::Write;
        let row = (width * 4) as usize;
        let data_size = (row * height as usize) as u32;
        let mut header = Vec::new();
        header.extend_from_slice(b"BM");
        header.extend_from_slice(&(54u32 + data_size).to_le_bytes());
        header.extend_from_slice(&0u32.to_le_bytes());
        header.extend_from_slice(&54u32.to_le_bytes());
        header.extend_from_slice(&40u32.to_le_bytes());
        header.extend_from_slice(&width.to_le_bytes());
        header.extend_from_slice(&height.to_le_bytes());
        header.extend_from_slice(&1u16.to_le_bytes());
        header.extend_from_slice(&32u16.to_le_bytes());
        header.extend_from_slice(&0u32.to_le_bytes());
        header.extend_from_slice(&data_size.to_le_bytes());
        header.extend_from_slice(&0i32.to_le_bytes());
        header.extend_from_slice(&0i32.to_le_bytes());
        header.extend_from_slice(&0u32.to_le_bytes());
        header.extend_from_slice(&0u32.to_le_bytes());
        file.write_all(&header).unwrap();
        // DIBits came top-down because biHeight is negative; BMP wants bottom-up.
        for y in (0..height as usize).rev() {
            file.write_all(&pixels[y * row..(y + 1) * row]).unwrap();
        }
        eprintln!("wrote {}", out.display());
        popup.dismiss();
        unsafe {
            let _ = DestroyWindow(parent);
        }
    }

    #[cfg(windows)]
    #[test]
    fn chooser_height_cannot_persist_as_result_popup_size() {
        use super::windows_impl::valid_result_size;

        let min = (360, 280);
        let fallback = (540, 400);
        // Chooser strip must not become the result size.
        assert_eq!(valid_result_size(Some((400, 38)), min, fallback), fallback);
        assert_eq!(valid_result_size(Some((200, 400)), min, fallback), fallback);
        assert_eq!(
            valid_result_size(Some((700, 500)), min, fallback),
            (700, 500)
        );
        assert_eq!(valid_result_size(None, min, fallback), fallback);
    }

    #[cfg(windows)]
    #[test]
    fn profile_chooser_geometry_never_fills_client_height() {
        use super::windows_impl::{chooser_button_width, chooser_size, chooser_strip_layout};

        let widths: Vec<i32> = ["Expert", "Program", "Concise", "More…"]
            .iter()
            .map(|label| chooser_button_width(label))
            .collect();
        let size = chooser_size(Point { x: 200, y: 200 }, &widths, 96);
        assert_eq!(size.1, 38, "chooser window height must stay compact");

        let (row_top, row_height, rects) = chooser_strip_layout(size.0, 400, &widths, 96);
        assert_eq!(row_height, 38);
        assert!(
            row_top > 100,
            "strip is vertically centered in a tall client"
        );
        assert_eq!(rects.len(), 4);
        assert!(rects.iter().all(|(_, w)| *w > 20 && *w < 200));
        assert!(row_height * 3 < 400);
    }

    #[cfg(windows)]
    #[test]
    fn resize_hit_test_uses_edges_and_keeps_interior_client() {
        use super::windows_impl::resize_hit_test;
        use windows::Win32::UI::WindowsAndMessaging::{
            HTBOTTOMRIGHT, HTCLIENT, HTLEFT, HTTOP, HTTOPLEFT,
        };

        assert_eq!(
            resize_hit_test(Point { x: 2, y: 2 }, 400, 300, 6, false).0,
            HTTOPLEFT as isize
        );
        assert_eq!(
            resize_hit_test(Point { x: 2, y: 100 }, 400, 300, 6, false).0,
            HTLEFT as isize
        );
        assert_eq!(
            resize_hit_test(Point { x: 398, y: 298 }, 400, 300, 6, false).0,
            HTBOTTOMRIGHT as isize
        );
        assert_eq!(
            resize_hit_test(Point { x: 200, y: 2 }, 400, 300, 6, false).0,
            HTTOP as isize
        );
        // Profile chooser has no top drag band, so top-center is not a resize
        // zone only for the main popup; interior stays client either way.
        assert_eq!(
            resize_hit_test(Point { x: 200, y: 150 }, 400, 300, 6, false).0,
            HTCLIENT as isize
        );
        assert_eq!(
            resize_hit_test(Point { x: 200, y: 150 }, 400, 300, 6, true).0,
            HTCLIENT as isize
        );
    }

    #[cfg(windows)]
    #[test]
    fn layout_fills_width_and_lets_result_absorb_extra_height() {
        use super::windows_impl::compute_layout;

        let base = compute_layout(440, 260, 96, 5);
        let wide = compute_layout(800, 260, 96, 5);
        let tall = compute_layout(440, 500, 96, 5);

        // Buttons stretch with content width (integer division may leave 1px).
        assert!(wide.button_width > base.button_width);
        let wide_used = wide.button_width * 5 + wide.button_gap * 4 + wide.margin * 2;
        let base_used = base.button_width * 5 + base.button_gap * 4 + base.margin * 2;
        // Integer division may leave a few leftover pixels.
        assert!((790..=800).contains(&wide_used));
        assert!((430..=440).contains(&base_used));

        // Target/context stay compact; result grows with height.
        assert_eq!(tall.target_height, base.target_height);
        assert_eq!(tall.context_height, base.context_height);
        assert_eq!(tall.output_top, base.output_top);
        assert!(tall.output_height > base.output_height);
        assert!(tall.button_top > base.button_top);

        // Shorter client shrinks panes instead of overflowing buttons.
        let short = compute_layout(440, 180, 96, 5);
        assert!(short.context_height <= base.context_height);
        assert!(short.target_height <= base.target_height);
        assert!(short.output_height >= 1);
        assert!(short.button_top + short.button_height <= 180);
    }

    #[cfg(windows)]
    #[test]
    fn unified_popup_palette_and_control_styles_are_dark_and_borderless() {
        use super::windows_impl::{OWNER_DRAW_BUTTON_STYLE, POPUP_ACCENT, POPUP_BG, POPUP_TEXT};
        use windows::Win32::UI::WindowsAndMessaging::BS_PUSHBUTTON;

        assert_ne!(POPUP_BG, POPUP_TEXT);
        assert_ne!(POPUP_BG, POPUP_ACCENT);
        assert_eq!(
            OWNER_DRAW_BUTTON_STYLE & BS_PUSHBUTTON as u32,
            BS_PUSHBUTTON as u32
        );
        assert_ne!(OWNER_DRAW_BUTTON_STYLE & 0x0000000b, 0);
    }

    #[cfg(windows)]
    #[test]
    fn visual_geometry_and_button_states_scale_at_common_dpi_values() {
        use super::windows_impl::{
            button_fill, popup_corner_radius, scaled_size, ButtonVisualState, POPUP_ACCENT_DIM,
            POPUP_BUTTON_BG,
        };
        assert_eq!(scaled_size((660, 470), 96), (660, 470));
        assert_eq!(scaled_size((660, 470), 144), (990, 705));
        assert_eq!(scaled_size((660, 470), 192), (1320, 940));
        assert_eq!(popup_corner_radius(96), 12);
        assert_eq!(popup_corner_radius(144), 18);
        assert_eq!(popup_corner_radius(192), 24);
        assert_eq!(button_fill(ButtonVisualState::Normal), POPUP_BUTTON_BG);
        assert_eq!(button_fill(ButtonVisualState::Pressed), POPUP_ACCENT_DIM);
        assert_ne!(button_fill(ButtonVisualState::Disabled), POPUP_BUTTON_BG);
        assert_ne!(button_fill(ButtonVisualState::Focused), POPUP_BUTTON_BG);
    }

    #[cfg(windows)]
    #[test]
    fn profile_names_are_reduced_to_one_bounded_word() {
        use super::windows_impl::{chooser_button_width, compact_profile_label};

        assert_eq!(compact_profile_label("Word explanation"), "Word");
        assert_eq!(compact_profile_label("code-specialist"), "code");
        assert_eq!(compact_profile_label("简洁解释"), "简洁解释");
        assert_eq!(compact_profile_label("abcdefghijklmnop"), "abcdefghijk…");
        assert_eq!(chooser_button_width("Contextual"), 100);
        assert_eq!(chooser_button_width("Word"), 52);
        assert_eq!(chooser_button_width("Wiki"), 52);
        assert_eq!(chooser_button_width("More…"), 60);
    }

    #[test]
    fn clamps_to_monitor_edges() {
        let area = Rect {
            left: 0,
            top: 0,
            right: 1_920,
            bottom: 1_080,
        };
        assert_eq!(
            clamped_origin(Point { x: 1_900, y: 1_070 }, (360, 132), area),
            Point { x: 1_560, y: 948 }
        );
        assert_eq!(
            clamped_origin(Point { x: -50, y: -20 }, (360, 132), area),
            Point { x: 0, y: 0 }
        );
    }

    #[test]
    fn cascade_prefers_right_and_falls_back_left() {
        let area = Rect {
            left: 0,
            top: 0,
            right: 1_920,
            bottom: 1_080,
        };
        assert_eq!(
            cascade_origin(
                Rect {
                    left: 100,
                    top: 80,
                    right: 560,
                    bottom: 350,
                },
                (440, 260),
                8,
                area,
            ),
            Point { x: 568, y: 80 }
        );
        assert_eq!(
            cascade_origin(
                Rect {
                    left: 1_400,
                    top: 80,
                    right: 1_860,
                    bottom: 350,
                },
                (440, 260),
                8,
                area,
            ),
            Point { x: 952, y: 80 }
        );
    }

    #[test]
    fn oversized_popup_stays_inside_area() {
        let area = Rect {
            left: 10,
            top: 20,
            right: 100,
            bottom: 80,
        };
        assert_eq!(
            clamped_origin(Point { x: 40, y: 40 }, (500, 500), area),
            Point { x: 10, y: 20 }
        );
    }

    #[test]
    fn degenerate_work_area_does_not_panic() {
        assert_eq!(
            clamped_origin(
                Point { x: 50, y: 50 },
                (360, 132),
                Rect {
                    left: 10,
                    top: 20,
                    right: 10,
                    bottom: 20,
                },
            ),
            Point { x: 10, y: 20 }
        );
    }

    #[cfg(windows)]
    #[test]
    fn output_buffer_is_bounded_and_marks_truncation() {
        let mut value = super::windows_impl::OutputBuffer::new("");
        value.append(&"x".repeat(super::windows_impl::MAX_OUTPUT_CHARS + 100));
        assert!(value.truncated);
        assert!(value.text.chars().count() <= super::windows_impl::MAX_OUTPUT_CHARS);
        assert!(value.text.ends_with("[Output truncated]"));
    }

    #[cfg(windows)]
    #[test]
    fn markdown_renderer_keeps_raw_copy_text_but_formats_visible_content() {
        let rendered = super::windows_impl::render_markdown(
            "# Title\n- **bold** and *italic*\n[docs](https://example.test) <b>raw</b>\n```\nlet 😀 = 1;\n```",
        );
        assert_eq!(
            rendered.text,
            "Title\n• bold and italic\ndocs (https://example.test) <b>raw</b>\nlet 😀 = 1;"
        );
        assert!(rendered
            .spans
            .iter()
            .any(|span| matches!(span.style, super::windows_impl::MarkdownStyle::Heading(1))));
        assert!(rendered
            .spans
            .iter()
            .any(|span| span.style == super::windows_impl::MarkdownStyle::Bold));
        let emoji = rendered.text.encode_utf16().position(|unit| unit == 0xd83d);
        assert!(emoji.is_some());
        assert!(rendered.spans.iter().all(|span| span.start <= span.end));
        assert_eq!(
            super::windows_impl::render_markdown("[a](u)tail").text,
            "a (u)tail"
        );
    }

    #[cfg(windows)]
    #[test]
    fn markdown_streaming_preserves_split_delimiters_until_completion() {
        use super::windows_impl::render_markdown;

        let first = render_markdown("**bo");
        assert_eq!(first.text, "**bo");
        assert!(first.spans.is_empty());

        let completed = render_markdown("**bold**");
        assert_eq!(completed.text, "bold");
        assert!(completed
            .spans
            .iter()
            .any(|span| span.style == super::windows_impl::MarkdownStyle::Bold));

        let link_partial = render_markdown("[docs](https://example.test");
        assert_eq!(link_partial.text, "[docs](https://example.test");
        let link_complete = render_markdown("[docs](https://example.test)");
        assert_eq!(link_complete.text, "docs (https://example.test)");
    }

    #[cfg(windows)]
    #[test]
    fn drag_band_client_routing_only_accepts_blank_top_strip() {
        use super::windows_impl::drag_band_client_contains;

        assert!(drag_band_client_contains(
            Point { x: 100, y: 23 },
            (440, 260),
            96,
            false
        ));
        assert!(!drag_band_client_contains(
            Point { x: 100, y: 24 },
            (440, 260),
            96,
            false
        ));
        assert!(!drag_band_client_contains(
            Point { x: 100, y: 10 },
            (440, 260),
            96,
            true
        ));
        assert!(!drag_band_client_contains(
            Point { x: -1, y: 10 },
            (440, 260),
            96,
            false
        ));
        assert!(drag_band_client_contains(
            Point { x: 100, y: 35 },
            (660, 390),
            144,
            false
        ));
        assert!(!drag_band_client_contains(
            Point { x: 100, y: 36 },
            (660, 390),
            144,
            false
        ));
        assert!(drag_band_client_contains(
            Point { x: 100, y: 47 },
            (880, 520),
            192,
            false
        ));
        assert!(!drag_band_client_contains(
            Point { x: 100, y: 48 },
            (880, 520),
            192,
            false
        ));
    }

    #[cfg(windows)]
    #[test]
    fn native_move_defers_updates_and_flushes_only_once() {
        use super::windows_impl::{mark_render_pending, should_arm_render, take_render_pending};

        let mut pending = false;
        assert!(mark_render_pending(true, &mut pending));
        assert!(pending);
        // A finish arriving after several deltas remains coalesced into the
        // same final render.
        assert!(mark_render_pending(true, &mut pending));
        assert!(take_render_pending(&mut pending));
        assert!(!pending);
        assert!(!take_render_pending(&mut pending));
        // Outside the move loop, updates render immediately.
        assert!(!mark_render_pending(false, &mut pending));
        assert!(!pending);
        assert!(should_arm_render(false, false));
        assert!(!should_arm_render(false, true));
        assert!(!should_arm_render(true, false));
    }

    #[cfg(windows)]
    #[test]
    fn native_move_flush_uses_the_last_state_including_terminal_replacement() {
        use super::windows_impl::{
            mark_render_pending, state_text, take_render_pending, OutputBuffer, PopupState,
        };

        let mut pending = false;
        let mut state = PopupState::Streaming(OutputBuffer::new("partial"));
        assert!(mark_render_pending(true, &mut pending));
        assert_eq!(state_text(&state), "partial");
        state = PopupState::Completed(OutputBuffer::new("complete"));
        assert!(mark_render_pending(true, &mut pending));
        assert_eq!(state_text(&state), "complete");
        state = PopupState::LocalError("provider failed".to_owned());
        assert!(mark_render_pending(true, &mut pending));
        assert_eq!(state_text(&state), "provider failed");
        assert!(take_render_pending(&mut pending));
        assert!(!pending);
    }

    #[cfg(windows)]
    #[test]
    fn stream_state_accumulates_before_single_terminal_render() {
        use super::windows_impl::{state_text, OutputBuffer, PopupState};

        let mut state = PopupState::Loading;
        state.append("first ");
        state.append("second");
        assert_eq!(state_text(&state), "first second");
        state.finish();
        assert!(matches!(state, PopupState::Completed(_)));
        assert_eq!(state_text(&state), "first second");

        // A late delta cannot overwrite a terminal result.
        state.append(" ignored");
        assert_eq!(state_text(&state), "first second");
        let mut empty = PopupState::Completed(OutputBuffer::new("cached"));
        empty.finish();
        assert_eq!(state_text(&empty), "cached");
    }

    #[cfg(windows)]
    #[test]
    fn fixed_input_pane_is_labeled_and_bounded() {
        assert_eq!(super::windows_impl::bounded_input("selected"), "selected");
        let value = super::windows_impl::bounded_input(&"x".repeat(5_000));
        assert_eq!(value.chars().count(), 4_096);
    }

    #[cfg(windows)]
    #[test]
    fn self_hover_accepts_only_completed_popup_text() {
        use super::windows_impl::{popup_allows_hover_text, OutputBuffer, PopupState};

        assert!(!popup_allows_hover_text(&PopupState::Loading));
        assert!(!popup_allows_hover_text(&PopupState::Streaming(
            OutputBuffer::new("partial")
        )));
        assert!(popup_allows_hover_text(&PopupState::Completed(
            OutputBuffer::new("complete")
        )));
        assert!(!popup_allows_hover_text(&PopupState::LocalError(
            "failed".to_owned()
        )));
    }

    #[cfg(windows)]
    #[test]
    fn hidden_richedit_applies_bold_format_to_completed_markdown() {
        use super::windows_impl::{set_output, POPUP_TEXT, RICH_EDIT_CLASS};
        use windows::core::w;
        use windows::Win32::Foundation::{FreeLibrary, HINSTANCE, LPARAM, WPARAM};
        use windows::Win32::System::LibraryLoader::{GetModuleHandleW, LoadLibraryW};
        use windows::Win32::UI::Controls::RichEdit::{
            CFE_BOLD, CFM_BOLD, CFM_COLOR, CFM_FACE, CHARFORMATW,
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, SendMessageW, ShowWindow, ES_AUTOVSCROLL, ES_MULTILINE,
            ES_NOHIDESEL, SW_HIDE, WINDOW_STYLE, WS_CHILD, WS_POPUP, WS_VISIBLE,
        };
        const EM_GETCHARFORMAT: u32 = 0x043a;
        const EM_SETSEL: u32 = 0x00b1;
        const SCF_SELECTION: usize = 0x0001;

        let module = unsafe { LoadLibraryW(w!("msftedit.dll")).expect("msftedit.dll") };
        let instance = unsafe { GetModuleHandleW(None).expect("module handle") };
        let parent = unsafe {
            CreateWindowExW(
                Default::default(),
                w!("STATIC"),
                w!(""),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(HINSTANCE(instance.0)),
                None,
            )
            .expect("hidden parent creation")
        };
        let hwnd = unsafe {
            CreateWindowExW(
                Default::default(),
                RICH_EDIT_CLASS,
                w!(""),
                WS_CHILD
                    | WS_VISIBLE
                    | WINDOW_STYLE((ES_MULTILINE | ES_AUTOVSCROLL | ES_NOHIDESEL) as u32),
                0,
                0,
                1,
                1,
                Some(parent),
                None,
                Some(HINSTANCE(instance.0)),
                None,
            )
            .expect("RICHEDIT50W creation")
        };
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }

        set_output(hwnd, "**bold**", true);
        let mut format = CHARFORMATW {
            cbSize: std::mem::size_of::<CHARFORMATW>() as u32,
            ..Default::default()
        };
        unsafe {
            let _ = SendMessageW(hwnd, EM_SETSEL, Some(WPARAM(0)), Some(LPARAM(4)));
            let _ = SendMessageW(
                hwnd,
                EM_GETCHARFORMAT,
                Some(WPARAM(SCF_SELECTION)),
                Some(LPARAM((&mut format as *mut CHARFORMATW) as isize)),
            );
            assert_ne!(format.dwMask.0 & CFM_BOLD.0, 0);
            assert_ne!(format.dwEffects.0 & CFE_BOLD.0, 0);
            assert_ne!(format.dwMask.0 & CFM_COLOR.0, 0);
            assert_eq!(format.crTextColor, POPUP_TEXT);
            assert_ne!(format.dwMask.0 & CFM_FACE.0, 0);
            let face = String::from_utf16_lossy(&format.szFaceName);
            assert_eq!(face.trim_end_matches('\0'), "Segoe UI");
            let _ = DestroyWindow(hwnd);
            let _ = DestroyWindow(parent);
            let _ = FreeLibrary(module);
        }
    }

    #[cfg(windows)]
    #[test]
    fn hidden_richedit_keeps_first_line_visible_during_streaming_and_completion() {
        use super::windows_impl::{set_output, RICH_EDIT_CLASS};
        use windows::core::w;
        use windows::Win32::Foundation::{FreeLibrary, HINSTANCE, LPARAM, WPARAM};
        use windows::Win32::System::LibraryLoader::{GetModuleHandleW, LoadLibraryW};
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, SendMessageW, ShowWindow, ES_AUTOVSCROLL, ES_MULTILINE,
            ES_NOHIDESEL, ES_READONLY, SW_HIDE, WINDOW_STYLE, WS_CHILD, WS_POPUP, WS_VISIBLE,
            WS_VSCROLL,
        };
        const EM_GETFIRSTVISIBLELINE: u32 = 0x00ce;

        let module = unsafe { LoadLibraryW(w!("msftedit.dll")).expect("msftedit.dll") };
        let instance = unsafe { GetModuleHandleW(None).expect("module handle") };
        let parent = unsafe {
            CreateWindowExW(
                Default::default(),
                w!("STATIC"),
                w!(""),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(HINSTANCE(instance.0)),
                None,
            )
            .expect("hidden parent creation")
        };
        let hwnd = unsafe {
            CreateWindowExW(
                Default::default(),
                RICH_EDIT_CLASS,
                w!(""),
                WS_CHILD
                    | WS_VISIBLE
                    | WS_VSCROLL
                    | WINDOW_STYLE(
                        (ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL | ES_NOHIDESEL) as u32,
                    ),
                0,
                0,
                320,
                100,
                Some(parent),
                None,
                Some(HINSTANCE(instance.0)),
                None,
            )
            .expect("RICHEDIT50W creation")
        };
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }

        let streaming = (0..40)
            .map(|line| format!("streaming line {line}\n"))
            .collect::<String>();
        set_output(hwnd, &streaming, false);
        let first_line_after_streaming = unsafe {
            SendMessageW(
                hwnd,
                EM_GETFIRSTVISIBLELINE,
                Some(WPARAM(0)),
                Some(LPARAM(0)),
            )
            .0 as i32
        };
        assert_eq!(
            first_line_after_streaming, 0,
            "streaming output must remain readable from its first line"
        );

        let completed = (0..40)
            .map(|line| format!("## completed line {line}\n- **value**\n"))
            .collect::<String>();
        set_output(hwnd, &completed, true);
        let first_line_after_completion = unsafe {
            SendMessageW(
                hwnd,
                EM_GETFIRSTVISIBLELINE,
                Some(WPARAM(0)),
                Some(LPARAM(0)),
            )
            .0 as i32
        };
        assert_eq!(
            first_line_after_completion, 0,
            "completed Markdown must remain readable from its first line"
        );

        unsafe {
            let _ = DestroyWindow(hwnd);
            let _ = DestroyWindow(parent);
            let _ = FreeLibrary(module);
        }
    }

    #[cfg(windows)]
    #[test]
    fn opacity_normalizes_to_unit_interval_and_maps_to_byte_alpha() {
        assert_eq!(super::normalize_opacity(0.75), 0.75);
        assert_eq!(super::normalize_opacity(-0.2), 0.0);
        assert_eq!(super::normalize_opacity(1.4), 1.0);
        assert_eq!(super::normalize_opacity(f32::NAN), 1.0);
        assert_eq!((0.5_f32 * 255.0).round() as u8, 128);
        super::set_popup_opacity(0.4);
        assert!((super::current_popup_opacity() - 0.4).abs() < f32::EPSILON);
        super::set_popup_opacity(1.0);
    }

    #[cfg(windows)]
    #[test]
    fn presentation_predicates_require_topmost_and_nonempty_on_screen_rect() {
        use super::windows_impl::{popup_ex_style_is_presentable, popup_rect_is_presentable};

        assert!(popup_ex_style_is_presentable(0x08 | 0x80 | 0x0800_0000));
        assert!(!popup_ex_style_is_presentable(0x80 | 0x0800_0000));
        assert!(popup_rect_is_presentable(
            Rect {
                left: 100,
                top: 100,
                right: 560,
                bottom: 314,
            },
            Rect {
                left: 0,
                top: 0,
                right: 1_920,
                bottom: 1_080,
            },
        ));
        assert!(!popup_rect_is_presentable(
            Rect {
                left: 100,
                top: 100,
                right: 100,
                bottom: 314,
            },
            Rect {
                left: 0,
                top: 0,
                right: 1_920,
                bottom: 1_080,
            },
        ));
        assert!(!popup_rect_is_presentable(
            Rect {
                left: 2_000,
                top: 100,
                right: 2_460,
                bottom: 314,
            },
            Rect {
                left: 0,
                top: 0,
                right: 1_920,
                bottom: 1_080,
            },
        ));
    }
}
