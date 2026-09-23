//! Small native, non-activating result popup.

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

#[cfg(windows)]
mod windows_impl {
    use super::super::runtime_trace;
    use super::super::theme;
    use super::{cascade_origin, clamped_origin, Point, Rect};
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{
        FreeLibrary, GlobalFree, COLORREF, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT,
        WPARAM,
    };
    use windows::Win32::Graphics::Gdi::{
        BeginPaint, CreateFontW, CreateRoundRectRgn, CreateSolidBrush, DeleteObject, DrawTextW,
        EndPaint, FillRect, FillRgn, FrameRect, FrameRgn, GetMonitorInfoW, InvalidateRect,
        MonitorFromPoint, MonitorFromWindow, SelectObject, SetBkColor, SetBkMode, SetTextColor,
        BACKGROUND_MODE, DRAW_TEXT_FORMAT, FONT_CHARSET, FONT_CLIP_PRECISION,
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
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        ReleaseCapture, SetCapture, SetFocus, VK_ESCAPE,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetDlgCtrlID;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, GetAncestor, GetClassNameW, GetClientRect,
        GetParent, GetWindow, GetWindowLongPtrW, GetWindowRect, GetWindowTextW, IsWindow,
        IsWindowVisible, MoveWindow, PostMessageW, RegisterClassW, SendMessageW,
        SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, SetWindowTextW, ShowWindow,
        WindowFromPoint, BS_PUSHBUTTON, CS_DROPSHADOW, CS_HREDRAW, CS_VREDRAW, ES_AUTOVSCROLL,
        ES_MULTILINE, ES_NOHIDESEL, ES_READONLY, GA_ROOT, GWLP_USERDATA, GWL_EXSTYLE, GW_OWNER,
        HMENU, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCLIENT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT,
        HTTOPRIGHT, HWND_TOPMOST, MA_NOACTIVATE, MINMAXINFO, SWP_NOACTIVATE, SWP_NOSIZE,
        SWP_NOZORDER, SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNOACTIVATE, WINDOW_STYLE, WM_APP,
        WM_CAPTURECHANGED, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_DPICHANGED, WM_DRAWITEM,
        WM_ENTERSIZEMOVE, WM_ERASEBKGND, WM_EXITSIZEMOVE, WM_GETMINMAXINFO, WM_KEYDOWN,
        WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_NCCREATE, WM_NCDESTROY,
        WM_NCHITTEST, WM_NCLBUTTONDOWN, WM_PAINT, WM_SETREDRAW, WM_SIZE, WM_TIMER, WNDCLASSW,
        WS_CHILD, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_TABSTOP,
        WS_VISIBLE, WS_VSCROLL,
    };

    const CLASS_NAME: PCWSTR = w!("SelectionTranslatePopup");
    // Geometry from the approved HTML mockup (logical px, DPI-scaled later).
    const WIDTH: i32 = 420;
    const HEIGHT: i32 = 520;
    const MIN_WIDTH: i32 = 380;
    const MAX_WIDTH: i32 = 460;
    const MIN_HEIGHT: i32 = 320;
    const BODY_PAD_Y: i32 = 12;
    const BODY_PAD_X: i32 = 14;
    const BODY_GAP: i32 = 10;
    const CARD_PAD_TOP: i32 = 8;
    const CARD_PAD_X: i32 = 12;
    const CARD_PAD_BOTTOM: i32 = 10;
    const CAPTION_HEIGHT: i32 = 14;
    const FOOTER_BUTTON_HEIGHT: i32 = 36;
    const FOOTER_GAP: i32 = 8;
    const TITLE_HEIGHT: i32 = 52;
    const TITLE_PAD_Y: i32 = 12;
    const TITLE_PAD_X: i32 = 14;
    const TITLE_MARK: i32 = 28;
    const TITLE_ICON_HIT: i32 = 32;
    const RAIL_HEIGHT: i32 = 48;
    const RAIL_PAD_X: i32 = 14;
    const PILL_HEIGHT: i32 = 28;
    const PILL_GAP: i32 = 4;
    const DRAG_BAND_HEIGHT: i32 = TITLE_HEIGHT;
    const CHOOSER_HEIGHT: i32 = 38;
    const CHOOSER_MARGIN: i32 = 4;
    const CHOOSER_BUTTON_GAP: i32 = 4;
    const CHOOSER_POINTER_GAP: i32 = 8;
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

    // Shared theme tokens (see theme.rs / DEVELOP_GUIDE). COLORREF is 0x00BBGGRR.
    pub(super) const OWNER_DRAW_BUTTON_STYLE: u32 = BS_PUSHBUTTON as u32 | 0x0000000b;

    pub const MAX_OUTPUT_CHARS: usize = 64 * 1024;
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
    /// Inline profile pills in the rail before overflowing into More….
    const INLINE_PROFILE_LIMIT: usize = 4;
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
        title_buttons: [HWND; 2],
        profile_buttons: Vec<HWND>,
        profile_button_widths: Vec<i32>,
        profile_labels: Vec<String>,
        active_profile: Option<usize>,
        choosing_profile: bool,
        /// Title-bar subtitle: “Resident” or the active profile name.
        subtitle: String,
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
        /// UI body, UI semibold, caption, subtitle, mono, CJK.
        fonts: [HFONT; 6],
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
                title_buttons: [HWND::default(); 2],
                profile_buttons: Vec::new(),
                profile_button_widths: Vec::new(),
                profile_labels: Vec::new(),
                active_profile: None,
                choosing_profile: false,
                subtitle: String::from("Resident"),
                notify_owner: true,
                in_native_move: false,
                render_pending: false,
                render_timer_armed: false,
                window_size: None,
                fonts: [HFONT::default(); 6],
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

        /// Populate the profile rail with inline pills (up to 4 + More…).
        /// Selection / Result stay visible; changing a pill posts
        /// `POPUP_PROFILE_SELECTED` and the owner re-requests with the same
        /// selection/context. No selected text is placed in profile controls.
        pub fn show_profile_choices(&mut self, names: &[String]) -> bool {
            let Some(data) = data_mut(self.hwnd) else {
                return false;
            };
            if names.is_empty() {
                return false;
            }
            // Standalone chooser only (guide §3.4 / §4.1): never embed the
            // profile bar inside the popup.
            let use_inline_rail = false;
            clear_profile_buttons(data);
            let previous_active = data.active_profile;
            data.profile_labels = names
                .iter()
                .map(|name| compact_profile_label(name))
                .collect();
            if !use_inline_rail {
                set_standard_controls_visible(data, false);
            }
            let Ok(instance) =
                (unsafe { windows::Win32::System::LibraryLoader::GetModuleHandleW(None) })
            else {
                if !use_inline_rail {
                    set_standard_controls_visible(data, true);
                }
                return false;
            };
            let inline_count = data.profile_labels.len().min(INLINE_PROFILE_LIMIT);
            let mut visible_labels = data.profile_labels[..inline_count].to_vec();
            if data.profile_labels.len() > INLINE_PROFILE_LIMIT {
                visible_labels.push("More…".to_owned());
            }
            data.profile_button_widths = visible_labels
                .iter()
                .map(|label| pill_button_width(label))
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
                        Some(HMENU(command_id as *mut core::ffi::c_void)),
                        Some(HINSTANCE(instance.0)),
                        None,
                    )
                }
                .unwrap_or_default();
                if !button.0.is_null() {
                    unsafe {
                        // Pill slot in userdata (More… uses usize::MAX).
                        let slot = if command_id == PROFILE_MORE_ID {
                            usize::MAX
                        } else {
                            index
                        };
                        let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
                            button,
                            windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
                            slot as isize,
                        );
                    }
                }
                if button.0.is_null() {
                    clear_profile_buttons(data);
                    if !use_inline_rail {
                        set_standard_controls_visible(data, true);
                    }
                    return false;
                }
                data.profile_buttons.push(button);
            }
            data.active_profile = previous_active
                .filter(|index| *index < names.len())
                .or(Some(0));
            data.choosing_profile = !use_inline_rail;
            let anchor = data.anchor;
            let dpi = data.dpi;
            let buttons = data.profile_buttons.clone();
            let button_font = data.fonts[1];
            let button_widths = data.profile_button_widths.clone();
            if use_inline_rail {
                for button in buttons {
                    if !button.0.is_null() && !button_font.0.is_null() {
                        unsafe {
                            let _ = SendMessageW(
                                button,
                                0x0030,
                                Some(WPARAM(button_font.0 as usize)),
                                Some(LPARAM(1)),
                            );
                            let _ = InvalidateRect(Some(button), None, true);
                        }
                    }
                }
                apply_layout(self.hwnd, anchor, dpi);
                unsafe {
                    let _ = InvalidateRect(Some(self.hwnd), None, true);
                }
                return true;
            }
            let size = chooser_size(anchor, &button_widths, dpi);
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

        /// Target and context share one Selection card. The selected target is
        /// never replaced by context text.
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
            popup_rect_is_presentable(actual, work_area)
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

    /// Footer / rail control roles from the mockup button table.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) enum ButtonKind {
        /// Copy — only filled primary in the footer.
        Primary,
        /// Retry, Prompt.
        Default,
        /// Pin, Close (footer).
        Ghost,
        /// Title-bar Pin / Close icon buttons.
        GhostIcon,
        /// Inactive profile pill.
        Pill,
        /// Active profile pill (accent fill).
        PillActive,
        /// More… pill (raised + line).
        PillMore,
    }

    pub(super) fn button_kind_for_id(id: usize, active_profile: Option<usize>) -> ButtonKind {
        match id {
            COPY_ID => ButtonKind::Primary,
            RETRY_ID | PROMPT_ID => ButtonKind::Default,
            PIN_ID | CLOSE_ID => ButtonKind::Ghost,
            PROFILE_MORE_ID => ButtonKind::PillMore,
            _ => {
                if profile_choice_index(id, usize::MAX).is_some() {
                    let index = id - PROFILE_CHOICE_ID_START;
                    let active = active_profile.or(Some(0));
                    if active == Some(index) {
                        ButtonKind::PillActive
                    } else {
                        ButtonKind::Pill
                    }
                } else {
                    ButtonKind::Default
                }
            }
        }
    }

    pub(super) fn button_fill(kind: ButtonKind, state: ButtonVisualState) -> COLORREF {
        match kind {
            ButtonKind::Primary => match state {
                ButtonVisualState::Pressed => theme::LINE,
                ButtonVisualState::Disabled => theme::RAISED,
                _ => theme::ACCENT,
            },
            ButtonKind::Default => match state {
                ButtonVisualState::Normal | ButtonVisualState::Disabled => theme::RAISED,
                ButtonVisualState::Pressed | ButtonVisualState::Focused => theme::LINE,
            },
            ButtonKind::Ghost | ButtonKind::GhostIcon => match state {
                ButtonVisualState::Pressed => theme::LINE,
                ButtonVisualState::Disabled => theme::SURFACE,
                _ => theme::SURFACE,
            },
            ButtonKind::Pill => theme::SURFACE,
            ButtonKind::PillActive => theme::ACCENT,
            ButtonKind::PillMore => match state {
                ButtonVisualState::Pressed | ButtonVisualState::Focused => theme::LINE,
                _ => theme::RAISED,
            },
        }
    }

    pub(super) fn button_text_color(kind: ButtonKind) -> COLORREF {
        match kind {
            ButtonKind::Primary | ButtonKind::PillActive => theme::ON_ACCENT,
            ButtonKind::Default
            | ButtonKind::Ghost
            | ButtonKind::GhostIcon
            | ButtonKind::PillMore => theme::INK,
            ButtonKind::Pill => theme::MUTED,
        }
    }

    /// Approximate `rgba(accent, a)` over `base` (GDI has no alpha brush here).
    pub(super) fn blend_over(base: COLORREF, over: COLORREF, over_alpha: u8) -> COLORREF {
        let (br, bg, bb) = (base.0 & 0xff, (base.0 >> 8) & 0xff, (base.0 >> 16) & 0xff);
        let (fr, fg, fb) = (over.0 & 0xff, (over.0 >> 8) & 0xff, (over.0 >> 16) & 0xff);
        let a = u32::from(over_alpha);
        let mix = |b: u32, f: u32| ((b * (255 - a) + f * a + 127) / 255) as u8;
        theme::rgb(mix(br, fr), mix(bg, fg), mix(bb, fb))
    }

    pub(super) fn popup_corner_radius(dpi: u32) -> i32 {
        scale(theme::RADIUS_POPUP, dpi).max(8)
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
        ((label.chars().count() as i32).saturating_mul(8) + 20).clamp(48, CHOOSER_MAX_BUTTON_WIDTH)
    }

    /// Profile-rail pill natural width (pad 8×14 language).
    pub(super) fn pill_button_width(label: &str) -> i32 {
        chooser_button_width(label)
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
        max_width: i32,
    ) -> (i32, i32) {
        match stored {
            Some((width, height)) if width >= min.0 && height >= min.1 => {
                (width.min(max_width).max(min.0), height)
            }
            _ => fallback,
        }
    }

    fn resolve_window_size(hwnd: HWND, dpi: u32) -> (i32, i32) {
        let min = min_popup_size(dpi);
        let fallback = default_popup_size(dpi);
        let max_width = max_popup_width(dpi);
        if let Some(data) = data_mut(hwnd) {
            let size = valid_result_size(data.window_size, min, fallback, max_width);
            data.window_size = Some(size);
            return size;
        }
        fallback
    }

    pub(super) fn default_popup_size(dpi: u32) -> (i32, i32) {
        scaled_size((WIDTH, HEIGHT), dpi)
    }

    pub(super) fn min_popup_size(dpi: u32) -> (i32, i32) {
        scaled_size((MIN_WIDTH, MIN_HEIGHT), dpi)
    }

    pub(super) fn max_popup_width(dpi: u32) -> i32 {
        scale(MAX_WIDTH, dpi)
    }

    /// Footer buttons size to content (Copy is the only filled primary).
    pub(super) fn footer_button_width(label: &str, dpi: u32) -> i32 {
        let chars = label.chars().count() as i32;
        scale((chars * 7 + 28).clamp(56, 120), dpi)
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

    /// Shared geometry for child controls and parent paint. Metrics follow the
    /// mockup chrome stack: title → profile rail → body (Selection + Result) → footer.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) struct PopupLayout {
        pub(super) content_width: i32,
        pub(super) margin_x: i32,
        pub(super) title_height: i32,
        pub(super) rail_top: i32,
        pub(super) rail_height: i32,
        pub(super) body_top: i32,
        pub(super) body_height: i32,
        pub(super) selection_top: i32,
        pub(super) selection_height: i32,
        pub(super) selection_caption_top: i32,
        pub(super) target_top: i32,
        pub(super) target_height: i32,
        pub(super) context_top: i32,
        pub(super) context_height: i32,
        pub(super) result_top: i32,
        pub(super) result_height: i32,
        pub(super) result_caption_top: i32,
        pub(super) output_top: i32,
        pub(super) output_height: i32,
        pub(super) footer_top: i32,
        pub(super) button_top: i32,
        pub(super) button_height: i32,
        pub(super) button_widths: [i32; 5],
        pub(super) button_xs: [i32; 5],
        pub(super) button_gap: i32,
        pub(super) title_icon_xs: [i32; 2],
        pub(super) title_icon_size: i32,
        pub(super) drag_band: i32,
    }

    pub(super) fn compute_layout(
        client_width: i32,
        client_height: i32,
        dpi: u32,
        button_labels: [&str; 5],
    ) -> PopupLayout {
        let pad_x = scale(BODY_PAD_X, dpi);
        let pad_y = scale(BODY_PAD_Y, dpi);
        let gap = scale(BODY_GAP, dpi);
        let title_height = scale(TITLE_HEIGHT, dpi);
        let rail_height = scale(RAIL_HEIGHT, dpi);
        let caption_height = scale(CAPTION_HEIGHT, dpi);
        let footer_button = scale(FOOTER_BUTTON_HEIGHT, dpi);
        let footer_gap = scale(FOOTER_GAP, dpi);
        let card_pad_top = scale(CARD_PAD_TOP, dpi);
        let card_pad_bottom = scale(CARD_PAD_BOTTOM, dpi);
        let content_width = (client_width - pad_x * 2).max(1);
        let margin_x = pad_x;

        let footer_height = pad_y * 2 + footer_button;
        let footer_top = (client_height - footer_height).max(title_height + rail_height);
        let body_top = title_height + rail_height;
        let body_height = (footer_top - body_top - pad_y).max(scale(80, dpi));

        // Selection card max-height ≈ 30% of body (spec S2.3).
        let selection_max = (body_height * 30 / 100).max(scale(72, dpi));
        let target_preferred = scale(40, dpi);
        let context_preferred = scale(36, dpi);
        let selection_inner =
            caption_height + scale(6, dpi) + target_preferred + scale(6, dpi) + context_preferred;
        let selection_height = (selection_inner + card_pad_top + card_pad_bottom)
            .min(selection_max)
            .max(scale(64, dpi));
        let selection_top = body_top + pad_y / 2;

        let selection_caption_top = selection_top + card_pad_top;
        let target_top = selection_caption_top + caption_height + scale(6, dpi);
        let available_sel =
            selection_height - card_pad_top - card_pad_bottom - caption_height - scale(6, dpi);
        let target_height = (available_sel * 55 / 100).max(scale(24, dpi));
        let context_top = target_top + target_height + scale(6, dpi);
        let context_height = (available_sel - target_height - scale(6, dpi)).max(scale(20, dpi));

        let result_top = selection_top + selection_height + gap;
        let result_height = (footer_top - pad_y - result_top).max(scale(80, dpi));
        let result_caption_top = result_top + card_pad_top;
        let output_top = result_caption_top + caption_height + scale(6, dpi);
        let output_height =
            (result_top + result_height - card_pad_bottom - output_top).max(scale(48, dpi));

        let button_top = footer_top + pad_y;
        let button_gap = footer_gap;
        let mut button_widths = [0i32; 5];
        let mut button_xs = [0i32; 5];
        let natural: [i32; 5] = [
            footer_button_width(button_labels[0], dpi),
            footer_button_width(button_labels[1], dpi),
            footer_button_width(button_labels[2], dpi),
            footer_button_width(button_labels[3], dpi),
            footer_button_width(button_labels[4], dpi),
        ];
        let natural_total = natural.iter().sum::<i32>() + button_gap * 4;
        let mut x = margin_x;
        if natural_total <= content_width {
            for index in 0..5 {
                button_widths[index] = natural[index];
                button_xs[index] = x;
                x += natural[index] + button_gap;
            }
        } else {
            // Shrink evenly when the client is narrower than the natural row.
            let each = ((content_width - button_gap * 4) / 5).max(scale(40, dpi));
            for index in 0..5 {
                button_widths[index] = each;
                button_xs[index] = x;
                x += each + button_gap;
            }
        }

        let icon = scale(TITLE_ICON_HIT, dpi);
        let title_icon_xs = [
            client_width - pad_x - icon * 2 - scale(4, dpi),
            client_width - pad_x - icon,
        ];

        // Body max-height clamp (~78vh / 600) is applied by default window size;
        // a user-resized taller client simply grows Result.
        PopupLayout {
            content_width,
            margin_x,
            title_height,
            rail_top: title_height,
            rail_height,
            body_top,
            body_height,
            selection_top,
            selection_height,
            selection_caption_top,
            target_top,
            target_height,
            context_top,
            context_height,
            result_top,
            result_height,
            result_caption_top,
            output_top,
            output_height,
            footer_top,
            button_top,
            button_height: footer_button,
            button_widths,
            button_xs,
            button_gap,
            title_icon_xs,
            title_icon_size: icon,
            drag_band: title_height,
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
        let labels = [
            "Copy",
            "Retry",
            "Prompt",
            if data.pinned { "Unpin" } else { "Pin" },
            "Close",
        ];
        let layout = compute_layout(client.right, client.bottom, dpi, labels);

        // Title-bar Pin / Close icon buttons (right).
        for (index, button) in data.title_buttons.iter().enumerate() {
            let x = layout.title_icon_xs[index];
            move_child(
                *button,
                x,
                (layout.title_height - layout.title_icon_size) / 2,
                layout.title_icon_size,
                layout.title_icon_size,
            );
        }

        // Profile rail pills (inline language; compact chooser keeps strip mode).
        layout_profile_rail(data, &layout, dpi);

        // Integrated Selection card: target + context stacked inside one well.
        move_child(
            data.input,
            layout.margin_x + scale(CARD_PAD_X, dpi),
            layout.target_top,
            layout.content_width - scale(CARD_PAD_X, dpi) * 2,
            layout.target_height,
        );
        move_child(
            data.context_input,
            layout.margin_x + scale(CARD_PAD_X, dpi),
            layout.context_top,
            layout.content_width - scale(CARD_PAD_X, dpi) * 2,
            layout.context_height,
        );
        move_child(
            data.output,
            layout.margin_x + scale(CARD_PAD_X, dpi),
            layout.output_top,
            layout.content_width - scale(CARD_PAD_X, dpi) * 2,
            layout.output_height,
        );
        for (index, button) in data.buttons.iter().enumerate() {
            move_child(
                *button,
                layout.button_xs[index],
                layout.button_top,
                layout.button_widths[index],
                layout.button_height,
            );
        }
    }

    fn layout_profile_rail(data: &mut PopupData, layout: &PopupLayout, dpi: u32) {
        if data.profile_buttons.is_empty() {
            return;
        }
        let pad_x = scale(RAIL_PAD_X, dpi);
        let gap = scale(PILL_GAP, dpi);
        let height = scale(PILL_HEIGHT, dpi).max(scale(32, dpi));
        let top = layout.rail_top + (layout.rail_height - height) / 2;
        let natural: Vec<i32> = data
            .profile_button_widths
            .iter()
            .map(|width| scale(*width, dpi).max(1))
            .collect();
        let count = natural.len().max(1) as i32;
        let available = (layout.content_width + pad_x - gap * (count - 1)).max(1);
        let natural_total = natural.iter().copied().sum::<i32>().max(1);
        let mut x = pad_x;
        for (index, button) in data.profile_buttons.iter().enumerate() {
            let width = if natural_total > available {
                (natural.get(index).copied().unwrap_or(1) * available / natural_total).max(1)
            } else {
                natural.get(index).copied().unwrap_or(1)
            };
            move_child(*button, x, top, width, height);
            x += width + gap;
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
        // H2 14/600 · code mono 12 gold (spec S2.3 / DEVELOP_GUIDE §4.1).
        let (mask, effects, height, color, face) = match span.style {
            MarkdownStyle::Bold => (CFM_BOLD, CFE_BOLD, 0, theme::INK, None),
            MarkdownStyle::Italic => (CFM_ITALIC, CFE_ITALIC, 0, theme::INK, None),
            MarkdownStyle::Strike => (CFM_STRIKEOUT, CFE_STRIKEOUT, 0, theme::INK, None),
            MarkdownStyle::Code => (
                CFM_SIZE | CFM_COLOR | CFM_FACE,
                CFE_EFFECTS(0),
                240,
                theme::GOLD,
                Some(theme::MONO_FONT),
            ),
            MarkdownStyle::Heading(level) => {
                let height = match level {
                    1 => 320,
                    2 => 280,
                    3 => 260,
                    _ => 240,
                };
                (
                    CFM_SIZE | CFM_BOLD | CFM_COLOR,
                    CFE_BOLD,
                    height,
                    theme::INK,
                    None,
                )
            }
        };
        let mut format = CHARFORMATW {
            cbSize: std::mem::size_of::<CHARFORMATW>() as u32,
            dwMask: mask,
            dwEffects: effects,
            yHeight: height,
            crTextColor: color,
            ..Default::default()
        };
        if let Some(face) = face {
            let face: Vec<u16> = face.encode_utf16().collect();
            format.szFaceName[..face.len()].copy_from_slice(&face);
        }
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
            crTextColor: theme::INK,
            bCharSet: FONT_CHARSET(1), // DEFAULT_CHARSET
            bPitchAndFamily: 0x20,     // FF_SWISS
            ..Default::default()
        };
        let face: Vec<u16> = theme::UI_FONT.encode_utf16().collect();
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
        // Inline rail pills stay populated so the user can switch profiles
        // again; only a compact cascade chooser needs tearing down.
        if !data.choosing_profile {
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
            let ui_body = -scale(theme::BODY_SIZE_PT, dpi);
            let ui_semi = -scale(theme::BUTTON_SIZE_PT, dpi);
            let caption = -scale(theme::CAPTION_SIZE_PT, dpi);
            let subtitle = -scale(11, dpi);
            let mono = -scale(theme::BODY_SIZE_PT, dpi);
            let cjk = -scale(12, dpi);
            let make_font = |height: i32, weight: i32, face: &str| {
                let mut face: Vec<u16> = face.encode_utf16().chain(std::iter::once(0)).collect();
                unsafe {
                    CreateFontW(
                        height,
                        0,
                        0,
                        0,
                        weight,
                        0,
                        0,
                        0,
                        FONT_CHARSET(1),
                        FONT_OUTPUT_PRECISION(0),
                        FONT_CLIP_PRECISION(0),
                        FONT_QUALITY(5),
                        0,
                        PCWSTR(face.as_mut_ptr()),
                    )
                }
            };
            data.fonts = [
                make_font(ui_body, 400, theme::UI_FONT),
                make_font(ui_semi, 600, theme::UI_FONT),
                make_font(caption, 600, theme::UI_FONT),
                make_font(subtitle, 400, theme::UI_FONT),
                make_font(mono, 400, theme::MONO_FONT),
                make_font(cjk, 400, theme::CJK_FONT),
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
                        Some(LPARAM(theme::RAISED.0 as isize)),
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
                            Some(LPARAM(theme::RAISED.0 as isize)),
                        );
                        // Tight side margins inside the Selection card.
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

            // Footer: Copy (primary) · Retry · Prompt · Pin · Close (ghost).
            let labels = ["Copy", "Retry", "Prompt", "Pin", "Close"];
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
                if !button.0.is_null() {
                    unsafe {
                        let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
                            *button,
                            windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
                            -1,
                        );
                    }
                }
            }
            // Title-bar ghost icon buttons: Pin + Close.
            let title_labels = ["⌖", "✕"];
            let title_ids = [PIN_ID, CLOSE_ID];
            for ((button, label), id) in data
                .title_buttons
                .iter_mut()
                .zip(title_labels)
                .zip(title_ids)
            {
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
                if !button.0.is_null() {
                    unsafe {
                        let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
                            *button,
                            windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
                            -1,
                        );
                    }
                }
            }
            // Target mono · context CJK · result body UI.
            let control_fonts = [
                (data.input, data.fonts[4]),
                (data.context_input, data.fonts[5]),
                (data.output, data.fonts[0]),
            ];
            for (control, font) in control_fonts {
                if !control.0.is_null() && !font.0.is_null() {
                    unsafe {
                        let _ = SendMessageW(
                            control,
                            0x0030,
                            Some(WPARAM(font.0 as usize)),
                            Some(LPARAM(1)),
                        );
                    }
                }
            }
            for button in data.buttons.iter().chain(data.title_buttons.iter()) {
                if !button.0.is_null() && !data.fonts[1].0.is_null() {
                    unsafe {
                        let _ = SendMessageW(
                            *button,
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
        let raw = *BRUSH.get_or_init(|| unsafe { CreateSolidBrush(theme::SURFACE).0 as usize });
        HBRUSH(raw as *mut core::ffi::c_void)
    }

    fn popup_section_brush() -> HBRUSH {
        static BRUSH: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
        let raw = *BRUSH.get_or_init(|| unsafe { CreateSolidBrush(theme::RAISED).0 as usize });
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
        let ctrl_id = unsafe { GetDlgCtrlID(item.hwndItem) } as usize;
        let parent = unsafe { GetParent(item.hwndItem) }.unwrap_or_default();
        let active_profile = data_mut(parent).and_then(|data| data.active_profile);
        let is_title_icon =
            data_mut(parent).is_some_and(|data| data.title_buttons.contains(&item.hwndItem));
        let pill_slot = unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
                item.hwndItem,
                windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
            )
        };
        // Pill slots are stored as >=0; non-pills use -1 (set at create).
        let is_pill = pill_slot >= 0;
        let kind = if is_title_icon {
            ButtonKind::GhostIcon
        } else if is_pill {
            let slot = pill_slot as usize;
            if slot == usize::MAX {
                ButtonKind::PillMore
            } else {
                let active = active_profile.or(Some(0));
                if active == Some(slot) {
                    ButtonKind::PillActive
                } else {
                    ButtonKind::Pill
                }
            }
        } else {
            button_kind_for_id(ctrl_id, active_profile)
        };
        let fill = button_fill(kind, state);
        let text_color = button_text_color(kind);
        let border = popup_brush(match kind {
            ButtonKind::Primary | ButtonKind::PillActive => fill,
            ButtonKind::Ghost | ButtonKind::GhostIcon => {
                if focused {
                    theme::ACCENT
                } else {
                    fill
                }
            }
            _ => {
                if focused {
                    theme::ACCENT
                } else {
                    theme::LINE
                }
            }
        });
        let brush = popup_brush(fill);
        let rect = item.rcItem;
        let radius = match kind {
            ButtonKind::Pill | ButtonKind::PillActive | ButtonKind::PillMore => {
                ((rect.bottom - rect.top) / 2).max(12)
            }
            ButtonKind::GhostIcon => scale(theme::RADIUS_CONTROL, 96) / 2,
            _ => scale(theme::RADIUS_CONTROL, 96),
        };
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
                if kind != ButtonKind::Ghost && kind != ButtonKind::GhostIcon || focused {
                    let _ = FrameRgn(item.hDC, region, border, 1, 1);
                }
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
            let old_bk = SetBkMode(item.hDC, TRANSPARENT);
            SetTextColor(item.hDC, text_color);
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
                // Focus-visible: 2px accent outline + 2px offset (S2.5).
                let focus_brush = popup_brush(theme::ACCENT);
                let mut outer = rect;
                outer.left += 1;
                outer.top += 1;
                outer.right -= 1;
                outer.bottom -= 1;
                let _ = FrameRect(item.hDC, &outer, focus_brush);
                let mut inner = rect;
                inner.left += 3;
                inner.top += 3;
                inner.right -= 3;
                inner.bottom -= 3;
                let _ = FrameRect(item.hDC, &inner, focus_brush);
                let _ = DeleteObject(focus_brush.into());
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
        let background = popup_brush(theme::SURFACE);
        unsafe {
            let _ = FillRect(hdc, &client, background);
            let _ = DeleteObject(background.into());
        }
        let border = popup_brush(theme::LINE);
        unsafe {
            let _ = FrameRect(hdc, &client, border);
            let _ = DeleteObject(border.into());
        }
        let labels = ["Copy", "Retry", "Prompt", "Pin", "Close"];
        let layout = compute_layout(client.right, client.bottom, dpi, labels);
        let card_radius = scale(theme::RADIUS_CARD, dpi).max(8);
        let left = layout.margin_x;
        let right = client.right - layout.margin_x;
        let fonts = data_mut(hwnd)
            .map(|data| data.fonts)
            .unwrap_or([HFONT::default(); 6]);
        let title_font = Some(fonts[1]).filter(|font| !font.0.is_null());
        let caption_font = Some(fonts[2]).filter(|font| !font.0.is_null());
        let subtitle_font = Some(fonts[3]).filter(|font| !font.0.is_null());

        // Title bar (surface + border-bottom line).
        let title_bottom = layout.title_height;
        let line_brush = popup_brush(theme::LINE);
        unsafe {
            let divider = RECT {
                left: 0,
                top: title_bottom - 1,
                right: client.right,
                bottom: title_bottom,
            };
            let _ = FillRect(hdc, &divider, line_brush);
        }
        // 28×28 accent mark with 文.
        let mark = scale(TITLE_MARK, dpi);
        let mark_left = left + scale(TITLE_PAD_X, dpi);
        let mark_top = (title_bottom - mark) / 2;
        paint_round_rect(
            hdc,
            RECT {
                left: mark_left,
                top: mark_top,
                right: mark_left + mark,
                bottom: mark_top + mark,
            },
            scale(8, dpi),
            theme::ACCENT,
            None,
        );
        paint_section_label(
            hdc,
            title_font,
            "文",
            RECT {
                left: mark_left,
                top: mark_top,
                right: mark_left + mark,
                bottom: mark_top + mark,
            },
            theme::ON_ACCENT,
        );
        let text_left = mark_left + mark + scale(10, dpi);
        let text_right = layout.title_icon_xs[0] - scale(4, dpi);
        paint_section_label(
            hdc,
            title_font,
            "Selection Translate",
            RECT {
                left: text_left,
                top: scale(TITLE_PAD_Y, dpi) - scale(2, dpi),
                right: text_right,
                bottom: scale(TITLE_PAD_Y, dpi) + scale(16, dpi),
            },
            theme::INK,
        );
        let subtitle = data_mut(hwnd)
            .map(|data| data.subtitle.clone())
            .unwrap_or_else(|| "Resident".to_owned());
        paint_section_label(
            hdc,
            subtitle_font,
            &subtitle,
            RECT {
                left: text_left,
                top: scale(TITLE_PAD_Y, dpi) + scale(16, dpi),
                right: text_right,
                bottom: title_bottom - scale(4, dpi),
            },
            theme::MUTED,
        );

        // Profile rail (border-bottom line).
        let rail_bottom = layout.rail_top + layout.rail_height;
        unsafe {
            let divider = RECT {
                left: 0,
                top: rail_bottom - 1,
                right: client.right,
                bottom: rail_bottom,
            };
            let _ = FillRect(hdc, &divider, line_brush);
            let _ = DeleteObject(line_brush.into());
        }

        // Selection card (raised + line, radius 10) — integrated Target + Context.
        paint_round_rect(
            hdc,
            RECT {
                left,
                top: layout.selection_top,
                right,
                bottom: layout.selection_top + layout.selection_height,
            },
            card_radius,
            theme::RAISED,
            Some(theme::LINE),
        );
        paint_section_label(
            hdc,
            caption_font,
            "SELECTION",
            RECT {
                left: left + scale(CARD_PAD_X, dpi),
                top: layout.selection_caption_top,
                right,
                bottom: layout.selection_caption_top + scale(CAPTION_HEIGHT, dpi),
            },
            theme::MUTED,
        );
        // Selection card: two paragraphs only (target mono ink, context muted
        // CJK) — no ctx chip (guide §4.1).

        // Result card (raised + line, radius 10) — visual hero.
        paint_round_rect(
            hdc,
            RECT {
                left,
                top: layout.result_top,
                right,
                bottom: layout.result_top + layout.result_height,
            },
            card_radius,
            theme::RAISED,
            Some(theme::LINE),
        );
        paint_section_label(
            hdc,
            caption_font,
            "RESULT",
            RECT {
                left: left + scale(CARD_PAD_X, dpi),
                top: layout.result_caption_top,
                right,
                bottom: layout.result_caption_top + scale(CAPTION_HEIGHT, dpi),
            },
            theme::MUTED,
        );

        // Footer border-top line.
        unsafe {
            let line = popup_brush(theme::LINE);
            let divider = RECT {
                left: 0,
                top: layout.footer_top,
                right: client.right,
                bottom: layout.footer_top + 1,
            };
            let _ = FillRect(hdc, &divider, line);
            let _ = DeleteObject(line.into());
        }
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
            if let Some(data) = data_mut(hwnd) {
                data.active_profile = Some(index);
                if let Some(name) = data.profile_labels.get(index) {
                    data.subtitle = name.clone();
                }
            }
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, true);
            }
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
        // Dark raised command menu — never the light system menu.
        let items: Vec<(usize, String)> = labels
            .iter()
            .enumerate()
            .skip(INLINE_PROFILE_LIMIT)
            .map(|(index, label)| (index, label.clone()))
            .collect();
        if items.is_empty() {
            return;
        }
        show_dark_profile_menu(hwnd, &items);
    }

    /// Compact dark raised menu (radius 10, pad 6) for overflow profiles.
    fn show_dark_profile_menu(parent: HWND, items: &[(usize, String)]) {
        static MENU_CLASS: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        let _ = MENU_CLASS.get_or_init(|| {
            let instance =
                unsafe { windows::Win32::System::LibraryLoader::GetModuleHandleW(None) }.ok();
            if let Some(instance) = instance {
                let class = WNDCLASSW {
                    style: CS_HREDRAW | CS_VREDRAW | CS_DROPSHADOW,
                    lpfnWndProc: Some(profile_menu_wnd_proc),
                    hInstance: HINSTANCE(instance.0),
                    hbrBackground: popup_section_brush(),
                    lpszClassName: w!("SelectionTranslateProfileMenu"),
                    ..Default::default()
                };
                unsafe {
                    let _ = RegisterClassW(&class);
                }
            }
        });
        let Ok(instance) =
            (unsafe { windows::Win32::System::LibraryLoader::GetModuleHandleW(None) })
        else {
            return;
        };
        let dpi = dpi_for_window(parent);
        let item_h = scale(32, dpi);
        let pad = scale(6, dpi);
        let width = scale(176, dpi);
        let height = pad * 2 + item_h * items.len() as i32;
        let mut parent_rect = RECT::default();
        let origin = if unsafe { GetWindowRect(parent, &mut parent_rect) }.is_ok() {
            Point {
                x: parent_rect.left + scale(12, dpi),
                y: parent_rect.bottom,
            }
        } else {
            Point { x: 0, y: 0 }
        };
        let menu_data = Box::new(ProfileMenuData {
            items: items.to_vec(),
            hover: None,
            parent,
        });
        let data_ptr = Box::into_raw(menu_data);
        let menu = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                w!("SelectionTranslateProfileMenu"),
                w!(""),
                WS_POPUP | WS_VISIBLE,
                origin.x,
                origin.y,
                width,
                height,
                Some(parent),
                None,
                Some(HINSTANCE(instance.0)),
                Some(data_ptr.cast()),
            )
        };
        let Ok(menu) = menu else {
            unsafe { drop(Box::from_raw(data_ptr)) };
            return;
        };
        apply_round_region(menu, (width, height), dpi);
        unsafe {
            let _ = SetWindowPos(
                menu,
                Some(HWND_TOPMOST),
                origin.x,
                origin.y,
                width,
                height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            // Capture so an outside click dismisses the menu.
            let _ = SetCapture(menu);
        }
    }

    #[derive(Debug)]
    struct ProfileMenuData {
        items: Vec<(usize, String)>,
        hover: Option<usize>,
        parent: HWND,
    }

    unsafe extern "system" fn profile_menu_wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_NCCREATE => {
                let create =
                    &*(lparam.0 as *const windows::Win32::UI::WindowsAndMessaging::CREATESTRUCTW);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
            }
            WM_NCDESTROY => {
                let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ProfileMenuData;
                if !pointer.is_null() {
                    drop(Box::from_raw(pointer));
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                }
                let _ = ReleaseCapture();
            }
            WM_PAINT => {
                let mut paint = windows::Win32::Graphics::Gdi::PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut paint);
                let mut client = RECT::default();
                let _ = GetClientRect(hwnd, &mut client);
                let fill = popup_brush(theme::RAISED);
                let _ = FillRect(hdc, &client, fill);
                let _ = DeleteObject(fill.into());
                let frame = popup_brush(theme::LINE);
                let _ = FrameRect(hdc, &client, frame);
                let _ = DeleteObject(frame.into());
                let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ProfileMenuData;
                if !pointer.is_null() {
                    let data = &*pointer;
                    let dpi = DEFAULT_DPI;
                    let pad = scale(6, dpi);
                    let item_h = scale(32, dpi);
                    let font = data_mut(data.parent)
                        .map(|p| p.fonts[0])
                        .filter(|font| !font.0.is_null());
                    for (row, (_, label)) in data.items.iter().enumerate() {
                        let top = pad + row as i32 * item_h;
                        let rect = RECT {
                            left: pad,
                            top,
                            right: client.right - pad,
                            bottom: top + item_h,
                        };
                        if data.hover == Some(row) {
                            paint_round_rect(
                                hdc,
                                rect,
                                scale(6, dpi),
                                blend_over(theme::RAISED, theme::ACCENT, 30),
                                None,
                            );
                        }
                        paint_section_label(
                            hdc,
                            font,
                            label,
                            rect,
                            if data.hover == Some(row) {
                                theme::ACCENT
                            } else {
                                theme::INK
                            },
                        );
                    }
                }
                let _ = EndPaint(hwnd, &paint);
                return LRESULT(0);
            }
            WM_MOUSEMOVE => {
                let y = ((lparam.0 as u32 >> 16) & 0xffff) as i16 as i32;
                let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ProfileMenuData;
                if !pointer.is_null() {
                    let data = &mut *pointer;
                    let pad = scale(6, DEFAULT_DPI);
                    let item_h = scale(32, DEFAULT_DPI);
                    let row = ((y - pad) / item_h).max(0) as usize;
                    let hover = (row < data.items.len()).then_some(row);
                    if hover != data.hover {
                        data.hover = hover;
                        unsafe {
                            let _ = InvalidateRect(Some(hwnd), None, true);
                        }
                    }
                }
                return LRESULT(0);
            }
            WM_LBUTTONUP => {
                let y = ((lparam.0 as u32 >> 16) & 0xffff) as i16 as i32;
                let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ProfileMenuData;
                if !pointer.is_null() {
                    let data = &*pointer;
                    let pad = scale(6, DEFAULT_DPI);
                    let item_h = scale(32, DEFAULT_DPI);
                    let row = ((y - pad) / item_h).max(0) as usize;
                    if let Some((index, _)) = data.items.get(row) {
                        let parent = data.parent;
                        let index = *index;
                        post_owner_with_value(parent, POPUP_PROFILE_SELECTED, index);
                    }
                }
                let _ = DestroyWindow(hwnd);
                return LRESULT(0);
            }
            WM_CAPTURECHANGED => {
                let _ = DestroyWindow(hwnd);
                return LRESULT(0);
            }
            WM_KEYDOWN if wparam.0 as u32 == VK_ESCAPE.0 as u32 => {
                let _ = DestroyWindow(hwnd);
                return LRESULT(0);
            }
            _ => {}
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
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
                let is_context = data_mut(hwnd).is_some_and(|data| data.context_input == child);
                SetBkColor(hdc, theme::RAISED);
                SetTextColor(hdc, if is_context { theme::MUTED } else { theme::INK });
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
                            let max_w = max_popup_width(dpi);
                            if width >= min.0 && height >= min.1 {
                                if let Some(data) = data_mut(hwnd) {
                                    data.window_size = Some((width.min(max_w), height));
                                }
                            }
                        }
                        layout_children(hwnd, dpi);
                        apply_round_region(hwnd, (width, height), dpi);
                        unsafe {
                            let _ = InvalidateRect(Some(hwnd), None, true);
                        }
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
                    info.ptMaxTrackSize.x = max_popup_width(dpi);
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
                    unsafe {
                        let _ = InvalidateRect(Some(hwnd), None, true);
                    }
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
    fn first_profile_pill_is_active_by_default() {
        use super::windows_impl::{button_kind_for_id, ButtonKind};
        const ID0: usize = 1000;
        assert_eq!(button_kind_for_id(ID0, None), ButtonKind::PillActive);
        assert_eq!(button_kind_for_id(ID0, Some(0)), ButtonKind::PillActive);
        assert_eq!(button_kind_for_id(ID0 + 1, Some(0)), ButtonKind::Pill);
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
        popup.show_profile_choices(&[
            "Translate".to_owned(),
            "Expert".to_owned(),
            "Program".to_owned(),
            "Concise".to_owned(),
            "Contextual".to_owned(),
            "Word".to_owned(),
        ]);
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
        assert!(width >= 380, "result popup width {width}");
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
        // Write 32-bit BI_RGB BMP (bottom-up) under windows/tmp for visual QA.
        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tmp/ui-redesign-popup/popup_preview.bmp");
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

        let min = (380, 320);
        let fallback = (420, 520);
        // Chooser strip must not become the result size.
        assert_eq!(
            valid_result_size(Some((400, 38)), min, fallback, 460),
            fallback
        );
        assert_eq!(
            valid_result_size(Some((200, 400)), min, fallback, 460),
            fallback
        );
        assert_eq!(
            valid_result_size(Some((700, 500)), min, fallback, 460),
            (460, 500)
        );
        assert_eq!(
            valid_result_size(Some((420, 520)), min, fallback, 460),
            (420, 520)
        );
        assert_eq!(valid_result_size(None, min, fallback, 460), fallback);
    }

    #[cfg(windows)]
    #[test]
    fn default_popup_width_is_420_clamped_to_380_460() {
        use super::windows_impl::{
            default_popup_size, max_popup_width, min_popup_size, scaled_size,
        };

        let (w, h) = default_popup_size(96);
        assert_eq!(w, 420);
        assert_eq!(h, 520);
        assert_eq!(min_popup_size(96).0, 380);
        assert_eq!(max_popup_width(96), 460);
        assert_eq!(scaled_size((420, 520), 120), (525, 650));
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

        let labels = ["Copy", "Retry", "Prompt", "Pin", "Close"];
        let base = compute_layout(420, 520, 96, labels);
        let tall = compute_layout(420, 700, 96, labels);
        let short = compute_layout(420, 360, 96, labels);

        // Selection stays compact (~30% body); Result absorbs extra height.
        assert!(tall.result_height > base.result_height);
        assert!(tall.output_height > base.output_height);
        assert!(base.selection_height <= base.body_height * 35 / 100);
        assert!(short.output_height >= 1);
        assert!(short.button_top + short.button_height <= 360);
        // Footer buttons are content-sized, not equal-width dumps.
        assert!(base.button_widths[0] > 0);
        assert!(base.button_xs[1] > base.button_xs[0]);
        assert!(base.button_xs[4] + base.button_widths[4] <= 420);
        // Title drag band is the full title height.
        assert_eq!(base.drag_band, base.title_height);
    }

    #[cfg(windows)]
    #[test]
    fn unified_popup_palette_and_control_styles_are_dark_and_borderless() {
        use super::super::theme;
        use super::windows_impl::OWNER_DRAW_BUTTON_STYLE;
        use windows::Win32::UI::WindowsAndMessaging::BS_PUSHBUTTON;

        assert_ne!(theme::SURFACE, theme::INK);
        assert_ne!(theme::SURFACE, theme::ACCENT);
        assert_eq!(theme::VOID, theme::ON_ACCENT);
        assert_eq!(
            OWNER_DRAW_BUTTON_STYLE & BS_PUSHBUTTON as u32,
            BS_PUSHBUTTON as u32
        );
        assert_ne!(OWNER_DRAW_BUTTON_STYLE & 0x0000000b, 0);
    }

    #[cfg(windows)]
    #[test]
    fn visual_geometry_and_button_states_scale_at_common_dpi_values() {
        use super::super::theme;
        use super::windows_impl::{
            button_fill, popup_corner_radius, scaled_size, ButtonKind, ButtonVisualState,
        };
        assert_eq!(scaled_size((420, 520), 96), (420, 520));
        assert_eq!(scaled_size((420, 520), 144), (630, 780));
        assert_eq!(scaled_size((420, 520), 192), (840, 1040));
        assert_eq!(popup_corner_radius(96), 12);
        assert_eq!(popup_corner_radius(144), 18);
        assert_eq!(popup_corner_radius(192), 24);
        assert_eq!(
            button_fill(ButtonKind::Primary, ButtonVisualState::Normal),
            theme::ACCENT
        );
        assert_eq!(
            button_fill(ButtonKind::Default, ButtonVisualState::Normal),
            theme::RAISED
        );
        assert_eq!(
            button_fill(ButtonKind::Ghost, ButtonVisualState::Normal),
            theme::SURFACE
        );
        assert_eq!(
            button_fill(ButtonKind::PillActive, ButtonVisualState::Normal),
            theme::ACCENT
        );
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

        // Title height 52 is the drag band at 96 DPI.
        assert!(drag_band_client_contains(
            Point { x: 100, y: 51 },
            (420, 520),
            96,
            false
        ));
        assert!(!drag_band_client_contains(
            Point { x: 100, y: 52 },
            (420, 520),
            96,
            false
        ));
        assert!(!drag_band_client_contains(
            Point { x: 100, y: 10 },
            (420, 520),
            96,
            true
        ));
        assert!(!drag_band_client_contains(
            Point { x: -1, y: 10 },
            (420, 520),
            96,
            false
        ));
        assert!(drag_band_client_contains(
            Point { x: 100, y: 77 },
            (630, 780),
            144,
            false
        ));
        assert!(!drag_band_client_contains(
            Point { x: 100, y: 78 },
            (630, 780),
            144,
            false
        ));
        assert!(drag_band_client_contains(
            Point { x: 100, y: 103 },
            (840, 1040),
            192,
            false
        ));
        assert!(!drag_band_client_contains(
            Point { x: 100, y: 104 },
            (840, 1040),
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
        use super::super::theme;
        use super::windows_impl::{set_output, RICH_EDIT_CLASS};
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
            assert_eq!(format.crTextColor, theme::INK);
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
