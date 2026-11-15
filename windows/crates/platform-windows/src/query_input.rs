//! Manual Query: a small always-on-top window with a text box. The user types
//! or pastes text and presses Enter; the resident then offers the profile
//! chooser for that text, exactly as it does for text selected in a result.
//!
//! The window is a plain Win32 EDIT inside a normal (activatable) window, so
//! keyboard focus and IME input (Chinese, Japanese, ...) work. The popups
//! themselves are never activated, which is why they cannot take typing.

use lingosnap_core::UiLanguage;
use std::sync::Mutex;

/// Posted to the resident's window when text was submitted. Fetch the text
/// with [`take_submitted`].
pub const QUERY_SUBMITTED: u32 = 0x8000 + 37; // WM_APP + 37 (WM_APP + 36 is the fair-copy command)

/// What the submit produced: the text and where the window was (its
/// bottom-left corner, in screen pixels), so the chooser appears there.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Submission {
    pub text: String,
    pub anchor: (i32, i32),
}

static SUBMISSION: Mutex<Option<Submission>> = Mutex::new(None);

pub fn take_submitted() -> Option<Submission> {
    SUBMISSION.lock().ok().and_then(|mut slot| slot.take())
}

/// Text that counts as a target: not empty, not only whitespace or
/// zero-width formatting characters.
pub fn has_target_text(text: &str) -> bool {
    text.chars().any(|c| {
        !(c.is_whitespace() || matches!(c, '\u{200b}'..='\u{200d}' | '\u{2060}' | '\u{feff}'))
    })
}

fn window_title(language: UiLanguage) -> &'static str {
    match language {
        UiLanguage::English => "LingoSnap \u{2014} Manual query",
        UiLanguage::SimplifiedChinese => "LingoSnap \u{2014} \u{624b}\u{52a8}\u{67e5}\u{8be2}",
    }
}

fn hint_text(language: UiLanguage) -> &'static str {
    match language {
        UiLanguage::English => "Enter: continue    Shift+Enter: new line    Esc: cancel",
        UiLanguage::SimplifiedChinese => {
            "\u{56de}\u{8f66}\u{ff1a}\u{7ee7}\u{7eed}    Shift+\u{56de}\u{8f66}\u{ff1a}\u{6362}\u{884c}    Esc\u{ff1a}\u{53d6}\u{6d88}"
        }
    }
}

#[cfg(windows)]
pub use windows_impl::show;

#[cfg(not(windows))]
pub fn show(_owner: (), _language: UiLanguage, _at: (i32, i32)) {}

#[cfg(windows)]
mod windows_impl {
    use super::{
        has_target_text, hint_text, window_title, Submission, QUERY_SUBMITTED, SUBMISSION,
    };
    use lingosnap_core::UiLanguage;
    use std::sync::atomic::{AtomicIsize, Ordering};
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, GetStockObject, MonitorFromPoint, COLOR_WINDOW, DEFAULT_GUI_FONT, HBRUSH,
        MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::HiDpi::GetDpiForSystem;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyState, SetFocus, VK_CONTROL, VK_ESCAPE, VK_RETURN, VK_SHIFT,
    };
    use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, FindWindowW, GetClientRect, GetWindowRect,
        GetWindowTextLengthW, GetWindowTextW, LoadCursorW, MoveWindow, PostMessageW,
        RegisterClassW, SendMessageW, SetForegroundWindow, ShowWindow, CS_HREDRAW, CS_VREDRAW,
        IDC_ARROW, SW_SHOW, WM_CHAR, WM_CLOSE, WM_KEYDOWN, WM_SETFOCUS, WM_SETFONT, WM_SIZE,
        WNDCLASSW, WS_CAPTION, WS_CHILD, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_OVERLAPPED,
        WS_SYSMENU, WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
    };

    const CLASS_NAME: PCWSTR = w!("LingoSnapManualQuery");
    const EM_SETSEL: u32 = 0x00B1;
    const ES_MULTILINE: u32 = 0x0004;
    const ES_AUTOVSCROLL: u32 = 0x0040;
    const ES_WANTRETURN: u32 = 0x1000;
    const MARGIN: i32 = 12;
    const HINT_HEIGHT: i32 = 20;

    static OWNER: AtomicIsize = AtomicIsize::new(0);
    static EDIT: AtomicIsize = AtomicIsize::new(0);
    static HINT: AtomicIsize = AtomicIsize::new(0);

    fn scale(value: i32) -> i32 {
        let dpi = unsafe { GetDpiForSystem() }.max(96) as i32;
        value * dpi / 96
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }

    /// Open the query window near `at` (screen pixels), or bring the
    /// existing one to the front.
    pub fn show(owner: HWND, language: UiLanguage, at: (i32, i32)) {
        OWNER.store(owner.0 as isize, Ordering::Relaxed);
        unsafe {
            if let Ok(existing) = FindWindowW(CLASS_NAME, PCWSTR::null()) {
                if !existing.0.is_null() {
                    focus(existing);
                    return;
                }
            }
            let Ok(module) = GetModuleHandleW(None) else {
                return;
            };
            let instance = HINSTANCE(module.0);
            let class = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(window_proc),
                hInstance: instance,
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as isize as *mut _),
                lpszClassName: CLASS_NAME,
                ..Default::default()
            };
            // Registering twice fails harmlessly with "class already exists".
            RegisterClassW(&class);

            let (width, height) = (scale(540), scale(190));
            let monitor = MonitorFromPoint(
                windows::Win32::Foundation::POINT { x: at.0, y: at.1 },
                MONITOR_DEFAULTTONEAREST,
            );
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            let work = if GetMonitorInfoW(monitor, &mut info).as_bool() {
                info.rcWork
            } else {
                RECT {
                    left: 0,
                    top: 0,
                    right: 1280,
                    bottom: 720,
                }
            };
            let x = at.0.clamp(work.left, (work.right - width).max(work.left));
            let y = at.1.clamp(work.top, (work.bottom - height).max(work.top));

            let title = wide(window_title(language));
            let created = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                CLASS_NAME,
                PCWSTR(title.as_ptr()),
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
                x,
                y,
                width,
                height,
                None,
                None,
                Some(instance),
                None,
            );
            let Ok(hwnd) = created else {
                return;
            };
            let font = GetStockObject(DEFAULT_GUI_FONT);
            let edit = CreateWindowExW(
                windows::Win32::UI::WindowsAndMessaging::WS_EX_CLIENTEDGE,
                w!("EDIT"),
                PCWSTR::null(),
                WS_CHILD
                    | WS_VISIBLE
                    | WS_TABSTOP
                    | WS_VSCROLL
                    | windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(
                        ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN,
                    ),
                0,
                0,
                10,
                10,
                Some(hwnd),
                None,
                Some(instance),
                None,
            )
            .unwrap_or_default();
            let hint_text = wide(hint_text(language));
            let hint = CreateWindowExW(
                Default::default(),
                w!("STATIC"),
                PCWSTR(hint_text.as_ptr()),
                WS_CHILD | WS_VISIBLE,
                0,
                0,
                10,
                10,
                Some(hwnd),
                None,
                Some(instance),
                None,
            )
            .unwrap_or_default();
            for control in [edit, hint] {
                SendMessageW(
                    control,
                    WM_SETFONT,
                    Some(WPARAM(font.0 as usize)),
                    Some(LPARAM(1)),
                );
            }
            EDIT.store(edit.0 as isize, Ordering::Relaxed);
            HINT.store(hint.0 as isize, Ordering::Relaxed);
            let _ = SetWindowSubclass(edit, Some(edit_proc), 1, 0);
            layout(hwnd);
            let _ = ShowWindow(hwnd, SW_SHOW);
            focus(hwnd);
        }
    }

    unsafe fn focus(hwnd: HWND) {
        let _ = unsafe { ShowWindow(hwnd, SW_SHOW) };
        let _ = unsafe { SetForegroundWindow(hwnd) };
        let edit = HWND(EDIT.load(Ordering::Relaxed) as *mut _);
        if !edit.0.is_null() {
            let _ = unsafe { SetFocus(Some(edit)) };
        }
    }

    unsafe fn layout(hwnd: HWND) {
        let mut client = RECT::default();
        if unsafe { GetClientRect(hwnd, &mut client) }.is_err() {
            return;
        }
        let margin = scale(MARGIN);
        let hint_height = scale(HINT_HEIGHT);
        let width = client.right - client.left - margin * 2;
        let edit_height = (client.bottom - client.top) - margin * 3 - hint_height;
        let edit = HWND(EDIT.load(Ordering::Relaxed) as *mut _);
        let hint = HWND(HINT.load(Ordering::Relaxed) as *mut _);
        if !edit.0.is_null() {
            let _ = unsafe { MoveWindow(edit, margin, margin, width, edit_height.max(40), true) };
        }
        if !hint.0.is_null() {
            let _ = unsafe {
                MoveWindow(
                    hint,
                    margin,
                    margin * 2 + edit_height,
                    width,
                    hint_height,
                    true,
                )
            };
        }
    }

    unsafe fn read_text(edit: HWND) -> String {
        let length = unsafe { GetWindowTextLengthW(edit) }.max(0) as usize;
        let mut buffer = vec![0u16; length + 1];
        let copied = unsafe { GetWindowTextW(edit, &mut buffer) }.max(0) as usize;
        String::from_utf16_lossy(&buffer[..copied.min(length)])
    }

    /// Hand the typed text to the resident and close the window. Text with
    /// no real characters is ignored, so nothing is ever sent for it.
    unsafe fn submit(hwnd: HWND) {
        let edit = HWND(EDIT.load(Ordering::Relaxed) as *mut _);
        let text = unsafe { read_text(edit) };
        if !has_target_text(&text) {
            return;
        }
        let mut rect = RECT::default();
        let _ = unsafe { GetWindowRect(hwnd, &mut rect) };
        if let Ok(mut slot) = SUBMISSION.lock() {
            *slot = Some(Submission {
                text: text.trim().to_owned(),
                anchor: (rect.left, rect.bottom),
            });
        }
        let owner = HWND(OWNER.load(Ordering::Relaxed) as *mut _);
        let _ = unsafe { DestroyWindow(hwnd) };
        let _ = unsafe { PostMessageW(Some(owner), QUERY_SUBMITTED, WPARAM(0), LPARAM(0)) };
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_SIZE => {
                unsafe { layout(hwnd) };
                LRESULT(0)
            }
            WM_SETFOCUS => {
                let edit = HWND(EDIT.load(Ordering::Relaxed) as *mut _);
                if !edit.0.is_null() {
                    let _ = unsafe { SetFocus(Some(edit)) };
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = unsafe { DestroyWindow(hwnd) };
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
        }
    }

    unsafe extern "system" fn edit_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        let shift_down = unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0;
        let ctrl_down = unsafe { GetKeyState(VK_CONTROL.0 as i32) } < 0;
        match message {
            WM_KEYDOWN if wparam.0 as u16 == VK_RETURN.0 && !shift_down => {
                let parent = unsafe { windows::Win32::UI::WindowsAndMessaging::GetParent(hwnd) }
                    .unwrap_or_default();
                unsafe { submit(parent) };
                return LRESULT(0);
            }
            WM_KEYDOWN if wparam.0 as u16 == VK_ESCAPE.0 => {
                let parent = unsafe { windows::Win32::UI::WindowsAndMessaging::GetParent(hwnd) }
                    .unwrap_or_default();
                let _ = unsafe { DestroyWindow(parent) };
                return LRESULT(0);
            }
            // Ctrl+A selects everything (multi-line edits do not by default).
            WM_KEYDOWN if ctrl_down && wparam.0 == 'A' as usize => {
                unsafe {
                    SendMessageW(hwnd, EM_SETSEL, Some(WPARAM(0)), Some(LPARAM(-1)));
                }
                return LRESULT(0);
            }
            // Swallow the character a plain Enter would insert (and beep for).
            WM_CHAR if wparam.0 == 0x0D && !shift_down => return LRESULT(0),
            _ => {}
        }
        unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
    }
}

#[cfg(test)]
mod tests {
    use super::{has_target_text, hint_text, window_title};
    use lingosnap_core::UiLanguage;

    #[test]
    fn only_real_characters_count_as_a_target() {
        assert!(has_target_text("hello"));
        assert!(has_target_text("  \u{4f60}\u{597d}\n"));
        assert!(!has_target_text(""));
        assert!(!has_target_text(" \t\r\n"));
        assert!(!has_target_text("\u{200b}\u{feff}\u{3000}"));
    }

    #[test]
    fn window_text_exists_in_both_languages() {
        for language in [UiLanguage::English, UiLanguage::SimplifiedChinese] {
            assert!(!window_title(language).is_empty());
            assert!(!hint_text(language).is_empty());
        }
        assert_ne!(
            window_title(UiLanguage::English),
            window_title(UiLanguage::SimplifiedChinese)
        );
    }
}
