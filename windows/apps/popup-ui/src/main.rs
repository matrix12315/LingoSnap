//! Fair-copy UI companion: hosts the mockup pages in WebView2 and speaks
//! protocol v1 with the resident over the named pipe. See FAIR_COPY_UI_PLAN.md.

#![cfg_attr(windows, windows_subsystem = "windows")]
#![cfg(windows)]

use selection_ui_protocol::{
    Envelope, Handshake, Surface, UiCommand, UiEvent, MANAGER_PIPE_NAME, PIPE_NAME,
};
use std::io::Write;
use std::sync::{Arc, Mutex};

use webview2_com::Microsoft::Web::WebView2::Win32::{
    CreateCoreWebView2EnvironmentWithOptions, ICoreWebView2, ICoreWebView2Controller,
    ICoreWebView2Controller2, ICoreWebView2Controller3, ICoreWebView2Environment,
};
use webview2_com::{
    CreateCoreWebView2ControllerCompletedHandler, CreateCoreWebView2EnvironmentCompletedHandler,
    ExecuteScriptCompletedHandler,
};
use windows::core::Interface as _;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{
    GENERIC_READ, GENERIC_WRITE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_NONE, OPEN_EXISTING,
};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetCursorPos,
    GetMessageW, GetWindowLongPtrW, GetWindowRect, KillTimer, MessageBoxW, PostMessageW,
    PostQuitMessage, RegisterClassW, SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    TranslateMessage, CS_HREDRAW, CS_VREDRAW, GWLP_USERDATA, HTCAPTION, HWND_TOPMOST, MB_ICONERROR,
    MSG, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SW_HIDE, SW_SHOW,
    WINDOW_LONG_PTR_INDEX, WM_APP, WM_DESTROY, WM_KEYDOWN, WM_NCLBUTTONDOWN, WM_TIMER, WNDCLASSW,
    WS_EX_APPWINDOW, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_MINIMIZEBOX,
    WS_POPUP, WS_SYSMENU,
};

const WM_APP_EVENT: u32 = WM_APP + 1;
/// Dev mode (--hold=ms): quits the message loop so the process can be
/// measured and does not require the resident's pipe.
const DEMO_QUIT_TIMER_ID: usize = 1;
/// Dev capture target shared between run() and the window proc.
static PENDING_CAPTURE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
const DEMO_FORCE_QUIT_TIMER_ID: usize = 2;
/// The window becomes visible only when the page has rendered; if the page
/// never loads, this timer exits the process so no invisible topmost window
/// can linger over the desktop.
const LOAD_TIMEOUT_TIMER_ID: usize = 3;
/// A companion that loaded but never received content exits after this long
/// (the plan's 600 s idle kill); the host restarts it on demand.
const IDLE_QUIT_TIMER_ID: usize = 4;
const IDLE_QUIT_MS: u32 = 600_000;
/// The last event anchor, applied after the window height settles: an
/// immediate reposition races the content-height resize and clamps the
/// window to the wrong screen edge.
static PENDING_ANCHOR: std::sync::Mutex<Option<Placement>> = std::sync::Mutex::new(None);

/// Where the next settled size report places the window.
#[derive(Clone, Copy, Debug)]
enum Placement {
    /// Centered on the anchor x, above (chooser) or below (card) anchor y.
    Anchor { x: i32, y: i32, above: bool },
    /// Keep the window's bottom-left corner fixed: a chooser raised over a
    /// kept card grows the window upward so the card does not move, the way
    /// the classic chooser floats above the source popup without moving it.
    KeepBottom { left: i32, bottom: i32 },
}
/// Popup id of the card currently presented (from ShowLoading).
static CARD_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// True while a chooser rail floats over a kept card (in-card selection).
static CHOOSER_OVER_CARD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Set when a render event arrives, cleared when the surface is dismissed.
/// The settled content_size re-posts only show the window while this is set,
/// so a dismissed chooser is not resurrected by a stale layout report.
static SHOW_PENDING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Set once per process; demo mode shows the window without waiting for a
/// host event.
static DEMO_MODE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Shared with the window proc for Esc handling; the pipe handle is cheap to
/// clone (it wraps an Arc).
static SHARED_PIPE: std::sync::Mutex<Option<pipe_connection::Pipe>> = std::sync::Mutex::new(None);
static WINDOW_SHOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
// Keep in sync with selection_ui_protocol::COMPANION_WINDOW_CLASS (the
// resident matches clicks against this class) and MANAGER_WINDOW_CLASS.
const CLASS_NAME: PCWSTR = w!("SelectionTranslatePopupUi");
const MANAGER_CLASS_NAME: PCWSTR = w!("SelectionTranslateManagerUi");
/// GWLP slots on the window: the boxed live WebView2 (user data) and its
/// controller (the class's first extra-bytes slot; the class reserves it via
/// `cbWndExtra`). An offset from GWLP_USERDATA is NOT a valid index — it
/// silently failed, leaking the controller and leaving resizes unbounded.
const WEBVIEW_SLOT: WINDOW_LONG_PTR_INDEX = WINDOW_LONG_PTR_INDEX(GWLP_USERDATA.0);
const CONTROLLER_SLOT: WINDOW_LONG_PTR_INDEX = WINDOW_LONG_PTR_INDEX(0);

mod bridge;
mod bridge_manager;

/// Embedded mockup assets. The page files link `fair-copy.css`, `theme.js`,
/// and `liquid-lens.js` by relative path, which cannot resolve from a virtual
/// document; `inlined_page` substitutes those references at load time.
mod assets {
    macro_rules! asset {
        ($name:literal) => {
            include_str!(concat!(
                "../../../../selection-translate-redesign/fair-copy/ui/",
                $name
            ))
        };
    }
    pub const FAIR_COPY_CSS: &str = asset!("fair-copy.css");
    pub const THEME_JS: &str = asset!("theme.js");
    pub const LIQUID_LENS_JS: &str = asset!("liquid-lens.js");
    pub const CHOOSER_HTML: &str = asset!("chooser.html");
    pub const POPUP_HTML: &str = asset!("popup.html");
    pub const POPUP_STREAMING_HTML: &str = asset!("popup-streaming.html");
    pub const POPUP_ERROR_HTML: &str = asset!("popup-error.html");
    pub const MANAGER_HTML: &str = asset!("manager.html");
}

/// One surface per process; marks whether the page speaks the manager data
/// bridge instead of the popup event bridge.
static MANAGER_SURFACE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn is_manager_surface(surface: Surface) -> bool {
    surface == Surface::Manager
}

/// Device scale of the window's monitor (1.0 at 96 DPI).
fn window_scale(hwnd: HWND) -> f64 {
    let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd) }.max(96);
    dpi as f64 / 96.0
}

fn window_size(surface: Surface) -> (i32, i32) {
    match surface {
        Surface::Chooser => (640, 132),
        Surface::Popup | Surface::Streaming => (460, 560),
        Surface::Error => (460, 380),
        Surface::Manager => (860, 700),
    }
}

fn inlined_page(surface: Surface) -> String {
    let page = match surface {
        Surface::Chooser => assets::CHOOSER_HTML,
        Surface::Streaming => assets::POPUP_STREAMING_HTML,
        Surface::Manager => assets::MANAGER_HTML,
        Surface::Popup => assets::POPUP_HTML,
        Surface::Error => assets::POPUP_ERROR_HTML,
    };
    let page = page
        .replace(
            r#"<link rel="stylesheet" href="fair-copy.css" />"#,
            &format!("<style>{}</style>", assets::FAIR_COPY_CSS),
        )
        .replace(
            r#"<script src="theme.js"></script>"#,
            &format!("<script>{}</script>", assets::THEME_JS),
        )
        .replace(
            r#"<script src="liquid-lens.js"></script>"#,
            &format!("<script>{}</script>", assets::LIQUID_LENS_JS),
        );
    // App mode: hide the demo caption row and wire the matching bridge.
    let (css, script) = if is_manager_surface(surface) {
        (
            bridge_manager::MANAGER_APP_CSS,
            bridge_manager::MANAGER_BRIDGE_JS,
        )
    } else {
        (bridge::APP_CSS, bridge::BRIDGE_JS)
    };
    page.replace(
        "</body>",
        &format!("<style>{}</style>{}</body>", css, script),
    )
}

fn parse_surface(arg: Option<&str>) -> Surface {
    match arg {
        Some("chooser") => Surface::Chooser,
        Some("streaming") => Surface::Streaming,
        Some("error") => Surface::Error,
        Some("manager") => Surface::Manager,
        _ => Surface::Popup,
    }
}

fn main() {
    capture_log(&format!(
        "start: pid={} exe={:?} cwd={:?}",
        std::process::id(),
        std::env::current_exe().map(|e| e.display().to_string()),
        std::env::current_dir().map(|d| d.display().to_string())
    ));
    // Per-monitor aware V2: the card must render at the monitor's device
    // scale and follow selections on ANY monitor. Everything below works in
    // physical pixels; the webview rasterization scale is set explicitly on
    // the controller after creation.
    let aware = unsafe {
        windows::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(
            windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        )
    }
    .is_ok();
    capture_log(&format!("start: per-monitor-aware={aware}"));
    let result = std::panic::catch_unwind(run);
    if result.is_err() {
        fatal("The fair-copy UI crashed.");
    }
}

fn run() {
    unsafe {
        if CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_err() {
            fatal("Could not initialize COM for the fair-copy UI.");
            return;
        }
    }
    let mut surface = Surface::Popup;
    let mut capture: Option<String> = None;
    let mut hold_ms: Option<u32> = None;
    let args: Vec<String> = std::env::args().collect();
    if let Some(index) = args.iter().position(|arg| arg == "--surface") {
        surface = parse_surface(args.get(index + 1).map(String::as_str));
    }
    for arg in &args {
        if let Some(value) = arg.strip_prefix("--capture=") {
            capture = Some(value.to_owned());
        } else if let Some(value) = arg.strip_prefix("--hold=") {
            hold_ms = value.parse::<u32>().ok();
        }
    }
    if capture.is_none() {
        if let Some(index) = args.iter().position(|arg| arg == "--capture") {
            capture = args.get(index + 1).cloned();
        }
    }
    if hold_ms.is_none() {
        if let Some(index) = args.iter().position(|arg| arg == "--hold") {
            hold_ms = args
                .get(index + 1)
                .and_then(|value| value.parse::<u32>().ok());
        }
    }

    let instance = match unsafe { GetModuleHandleW(None) } {
        Ok(instance) => instance,
        Err(_) => {
            fatal("Could not acquire the module handle.");
            return;
        }
    };
    let manager = is_manager_surface(surface);
    let class_name = if manager {
        MANAGER_CLASS_NAME
    } else {
        CLASS_NAME
    };
    let class = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        hInstance: instance.into(),
        lpszClassName: class_name,
        style: CS_HREDRAW | CS_VREDRAW,
        cbWndExtra: std::mem::size_of::<isize>() as i32,
        ..Default::default()
    };
    if unsafe { RegisterClassW(&class) } == 0 {
        fatal("Could not register the fair-copy UI window class.");
        return;
    }

    *PENDING_CAPTURE.lock().expect("capture lock") = capture.clone();
    let (width, height) = window_size(surface);
    let anchor = cursor_anchor();
    // No redirection bitmap: the window has no opaque surface of its own,
    // so the transparent WebView2 background composites straight onto the
    // desktop and the rail/card keep their rounded paper shape instead of
    // sitting in a black or paper rectangle.
    //
    // The popup surfaces are topmost tool windows (no taskbar button), like
    // the classic popup. The manager page is the manager's main window: a
    // normal, taskbar-visible, non-topmost app window titled like the native
    // manager, so it behaves like the window it replaces.
    let (ex_style, style, title) = if manager {
        (
            WS_EX_APPWINDOW | WS_EX_NOREDIRECTIONBITMAP,
            WS_POPUP | WS_SYSMENU | WS_MINIMIZEBOX,
            "Selection Translate — Manager",
        )
    } else {
        (
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOREDIRECTIONBITMAP,
            WS_POPUP,
            "LingoSnap fair-copy",
        )
    };
    let title: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
    let hwnd = match unsafe {
        CreateWindowExW(
            ex_style,
            class_name,
            PCWSTR(title.as_ptr()),
            style,
            anchor.x,
            anchor.y,
            width,
            height,
            None,
            None,
            Some(instance.into()),
            None,
        )
    } {
        Ok(hwnd) => hwnd,
        Err(_) => {
            fatal("Could not create the fair-copy UI window.");
            return;
        }
    };
    {
        let (base_w, base_h) = window_size(surface);
        let scale = window_scale(hwnd);
        let scaled_w = (base_w as f64 * scale).round() as i32;
        let scaled_h = (base_h as f64 * scale).round() as i32;
        if manager {
            // Open centered on the monitor under the cursor, like a normal
            // application window, never at the screen's top-left corner.
            let info = monitor_info_at(POINT {
                x: anchor.x,
                y: anchor.y,
            });
            let work = info.rcWork;
            let x = work.left + ((work.right - work.left - scaled_w) / 2).max(0);
            let y = work.top + ((work.bottom - work.top - scaled_h) / 2).max(0);
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    x,
                    y,
                    scaled_w,
                    scaled_h,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        } else {
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    0,
                    0,
                    scaled_w,
                    scaled_h,
                    SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }
    }

    let demo = hold_ms.is_some();
    DEMO_MODE.store(demo, std::sync::atomic::Ordering::Relaxed);
    let pipe = match pipe_connection::connect(is_manager_surface(surface)) {
        Ok(pipe) => Some(pipe),
        Err(_) if demo => None,
        Err(_) => {
            // Without the resident the companion has nothing to present.
            capture_log("exit: pipe connect failed");
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            return;
        }
    };
    if let Some(hold) = hold_ms {
        unsafe {
            let _ = SetTimer(Some(hwnd), DEMO_QUIT_TIMER_ID, hold, None);
        }
    }
    // Nothing may linger invisibly: if the page has not loaded within the
    // timeout, the process exits and the resident falls back to classic.
    unsafe {
        let _ = SetTimer(Some(hwnd), LOAD_TIMEOUT_TIMER_ID, 15_000, None);
    }
    *SHARED_PIPE.lock().expect("pipe lock") = pipe.clone();
    MANAGER_SURFACE.store(
        is_manager_surface(surface),
        std::sync::atomic::Ordering::Relaxed,
    );
    if !create_webview(hwnd, surface, pipe.clone(), capture.clone()) {
        capture_log("capture: create_webview failed");
        // Missing WebView2 runtime: exit quietly; the resident falls back to
        // the classic native popup when the pipe never comes up.
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        return;
    }
    // The window stays hidden until content arrives (see show_once), so a slow
    // or failed page load can never leave an invisible topmost window.

    if let Some(pipe) = pipe {
        pipe_thread::spawn(hwnd, pipe, surface);
    }

    let mut message = MSG::default();
    loop {
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if result.0 <= 0 {
            capture_log("exit: message loop ended");
            break;
        }
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

fn monitor_info_at(point: POINT) -> MONITORINFO {
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    unsafe {
        let _ = GetMonitorInfoW(MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST), &mut info);
    }
    info
}

fn cursor_anchor() -> POINT {
    unsafe {
        let mut point = POINT::default();
        let _ = GetCursorPos(&mut point);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let _ = GetMonitorInfoW(MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST), &mut info);
        let work = info.rcWork;
        let x = point.x.clamp(work.left, (work.right - 460).max(work.left));
        let y = point.y.clamp(work.top, (work.bottom - 200).max(work.top));
        POINT { x, y }
    }
}

fn fatal(message: &str) {
    unsafe {
        let wide: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
        let caption: Vec<u16> = "LingoSnap\0".encode_utf16().collect();
        MessageBoxW(
            None,
            PCWSTR(wide.as_ptr()),
            PCWSTR(caption.as_ptr()),
            MB_ICONERROR,
        );
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_DESTROY => {
            drop_webview(hwnd);
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        windows::Win32::UI::WindowsAndMessaging::WM_SIZE => {
            // Keep the webview covering the client area: a window that grew
            // past its creation size (rail over a long card) would otherwise
            // clip the page at the old bounds.
            let raw =
                unsafe { GetWindowLongPtrW(hwnd, CONTROLLER_SLOT) } as *mut ICoreWebView2Controller;
            if !raw.is_null() {
                let mut rect = RECT::default();
                unsafe {
                    let _ = GetClientRect(hwnd, &mut rect);
                    let _ = (*raw).SetBounds(rect);
                }
            }
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == DEMO_QUIT_TIMER_ID => {
            unsafe {
                let _ = KillTimer(Some(hwnd), DEMO_QUIT_TIMER_ID);
            }
            if let Some(path) = PENDING_CAPTURE.lock().expect("capture lock").clone() {
                capture_page(hwnd, &path);
                // The preview write is asynchronous; the completion handler
                // closes the window. Force-quit in case it never returns.
                unsafe {
                    let _ = SetTimer(Some(hwnd), DEMO_FORCE_QUIT_TIMER_ID, 4000, None);
                }
            } else {
                unsafe { PostQuitMessage(0) };
            }
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == LOAD_TIMEOUT_TIMER_ID => {
            // The page never finished loading: exit instead of lingering as
            // an invisible topmost window.
            capture_log("timer: load timeout fired");
            unsafe {
                let _ = KillTimer(Some(hwnd), LOAD_TIMEOUT_TIMER_ID);
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == IDLE_QUIT_TIMER_ID => {
            // Loaded but never presented anything (the skin was switched but
            // no translation happened): exit so the idle companion does not
            // sit in memory forever. The host restarts it on demand.
            capture_log("timer: idle quit fired");
            unsafe {
                let _ = KillTimer(Some(hwnd), IDLE_QUIT_TIMER_ID);
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 as u32 == VK_ESCAPE.0 as u32 => {
            // Borderless companion window: Esc is the manual close affordance
            // (the page also maps Esc to its Close command while it has
            // keyboard focus). The manager page is a normal window and does
            // not close on Esc.
            if MANAGER_SURFACE.load(std::sync::atomic::Ordering::Relaxed) {
                return LRESULT(0);
            }
            if let Some(pipe) = SHARED_PIPE.lock().expect("pipe lock").clone() {
                if let Ok(line) =
                    serde_json::to_string(&Envelope::new(0, UiCommand::UserClose { id: 0 }))
                {
                    let _ = pipe.write_line(&line);
                }
            }
            // The resident answers with Close for the surface it retired.
            // Over a kept card only the rail closes, so that reply decides;
            // otherwise hide now so Esc never depends on a round trip.
            if !CHOOSER_OVER_CARD.load(std::sync::atomic::Ordering::Relaxed) {
                hide_surface(hwnd);
            }
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == DEMO_FORCE_QUIT_TIMER_ID => {
            unsafe {
                let _ = KillTimer(Some(hwnd), DEMO_FORCE_QUIT_TIMER_ID);
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        WM_APP_EVENT => {
            // Content arrived: do NOT show yet — the geometry settle (the
            // content_size handler) applies the anchor and sizes the window
            // first, then shows. Showing here would flash the window at its
            // pre-anchor position and visibly jump it to the selection.
            // lparam owns a Box<String> holding one protocol line.
            let raw = lparam.0 as *mut String;
            if !raw.is_null() {
                let line = unsafe { Box::from_raw(raw) };
                handle_event_line(hwnd, &line);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

/// Run one script on the page. A script failure is only visible in the page
/// console, never a crash here — so the completion result is traced.
fn execute_script(hwnd: HWND, script: &str) {
    let raw = unsafe { GetWindowLongPtrW(hwnd, WEBVIEW_SLOT) } as *mut ICoreWebView2;
    if raw.is_null() {
        capture_log("script: no webview slot");
        return;
    }
    let webview = unsafe { &*raw };
    // The page's return value ('applied' / 'no-lingo' / 'err:...') is the only
    // signal whether a bridged event actually reached the page; log it.
    let handler = ExecuteScriptCompletedHandler::create(Box::new(|result, value| {
        if result.is_ok() {
            capture_log(&format!("script: {:.80}", value));
        }
        Ok(())
    }));
    unsafe {
        let _ = webview.ExecuteScript(&windows::core::HSTRING::from(script), &handler);
    }
}

/// Move the companion window to an event anchor. The process is per-monitor
/// aware: anchors and window coordinates are both physical screen pixels,
/// no conversion needed.
/// The on-screen position for an anchored surface of the given size,
/// clamped into the monitor's work area. Chooser surfaces sit above the
/// Make the companion window visible exactly once. Any incoming content
/// counts, so a lazily or eagerly started companion never floats an empty
/// card over the desktop, and the idle-quit timer stops once there is
/// something to present.
fn show_once(hwnd: HWND) {
    unsafe {
        let _ = KillTimer(Some(hwnd), IDLE_QUIT_TIMER_ID);
    }
    if !WINDOW_SHOWN.swap(true, std::sync::atomic::Ordering::Relaxed) {
        if MANAGER_SURFACE.load(std::sync::atomic::Ordering::Relaxed) {
            // The manager page is a normal application window: shown and
            // activated, never topmost.
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd);
            }
            return;
        }
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }
}

/// The on-screen position for an anchored surface of the given size,
/// clamped into the monitor's work area. Chooser surfaces sit above the
/// anchor, result surfaces below.
fn anchored_position(
    anchor: (i32, i32),
    above: bool,
    width: i32,
    height: i32,
    info: &MONITORINFO,
) -> (i32, i32) {
    let mut x = anchor.0 - width / 2;
    let mut y = if above {
        anchor.1 - height - 12
    } else {
        anchor.1 + 12
    };
    x = x.clamp(
        info.rcWork.left,
        (info.rcWork.right - width).max(info.rcWork.left),
    );
    y = y.clamp(
        info.rcWork.top,
        (info.rcWork.bottom - height).max(info.rcWork.top),
    );
    (x, y)
}

fn handle_event_line(hwnd: HWND, line: &str) {
    // A broken host connection ends with a Shutdown line; every surface must
    // honor it, or an orphaned companion floats a window nobody owns. The
    // line may be a bare UiEvent (the local poll loop's own farewell) or a
    // sequenced Envelope (the host's explicit shutdown).
    let bare = serde_json::from_str::<UiEvent>(line).ok();
    let enveloped = serde_json::from_str::<Envelope<UiEvent>>(line)
        .ok()
        .map(|envelope| envelope.payload);
    if matches!(bare, Some(UiEvent::Shutdown)) || matches!(enveloped, Some(UiEvent::Shutdown)) {
        capture_log("exit: shutdown event");
        unsafe {
            let _ = DestroyWindow(hwnd);
        };
        return;
    }
    if MANAGER_SURFACE.load(std::sync::atomic::Ordering::Relaxed) {
        // Manager lines are opaque data snapshots: {"apply": {...}}.
        execute_script(
            hwnd,
            &format!(
                // The manager bridge exports LINGO_MANAGER, not LINGO; the
                // wrong name silently dropped every config snapshot and the
                // page kept its mockup defaults.
                "window.LINGO_MANAGER && window.LINGO_MANAGER.applyManager({}); undefined",
                line
            ),
        );
        return;
    }
    let Some(event) = enveloped else {
        return;
    };
    use std::sync::atomic::Ordering::Relaxed;
    // The companion is reused across requests, so its window must follow the
    // selection: chooser events anchor above it, translation events below.
    // Only events that START a presentation may (re)show the window; a late
    // delta, result, or pin update must never resurrect a dismissed surface.
    let placement = match &event {
        UiEvent::ShowChooser {
            anchor, keeps_card, ..
        } => {
            capture_log(&format!("chooser: keeps_card={keeps_card}"));
            let shown = WINDOW_SHOWN.load(Relaxed) && SHOW_PENDING.load(Relaxed);
            if *keeps_card && shown {
                // The rail floats over the card being read: grow upward and
                // leave the card where it is.
                CHOOSER_OVER_CARD.store(true, Relaxed);
                let mut rect = RECT::default();
                let _ = unsafe { GetWindowRect(hwnd, &mut rect) };
                Some(Placement::KeepBottom {
                    left: rect.left,
                    bottom: rect.bottom,
                })
            } else {
                CHOOSER_OVER_CARD.store(false, Relaxed);
                Some(Placement::Anchor {
                    x: anchor.x,
                    y: anchor.y,
                    above: true,
                })
            }
        }
        UiEvent::ShowLoading { id, anchor, .. } => {
            CARD_ID.store(*id, Relaxed);
            CHOOSER_OVER_CARD.store(false, Relaxed);
            Some(Placement::Anchor {
                x: anchor.x,
                y: anchor.y,
                above: false,
            })
        }
        UiEvent::LocalError {
            anchor: Some(anchor),
            ..
        } => {
            CHOOSER_OVER_CARD.store(false, Relaxed);
            Some(Placement::Anchor {
                x: anchor.x,
                y: anchor.y,
                above: false,
            })
        }
        _ => None,
    };
    if let Some(placement) = placement {
        SHOW_PENDING.store(true, Relaxed);
        let usable = !matches!(placement, Placement::Anchor { x: 0, y: 0, .. });
        if usable {
            capture_log(&format!("placement: {placement:?}"));
            // Deferred: applied after the content-height resize settles.
            *PENDING_ANCHOR.lock().expect("anchor lock") = Some(placement);
        }
    }
    match event {
        UiEvent::Shutdown => {
            unsafe {
                let _ = DestroyWindow(hwnd);
            };
        }
        UiEvent::Close { id } => {
            if CHOOSER_OVER_CARD.load(Relaxed) && id != CARD_ID.load(Relaxed) {
                // Only the rail over the kept card was retired (Esc, or a
                // profile pick that is about to replace the card): the card
                // stays on screen, exactly like the classic source popup.
                capture_log("close: chooser over card");
                CHOOSER_OVER_CARD.store(false, Relaxed);
                apply_page_event(hwnd, &UiEvent::Close { id });
                return;
            }
            capture_log("close: hiding window");
            hide_surface(hwnd);
            apply_page_event(hwnd, &UiEvent::Close { id });
        }
        // Everything else goes to the page bridge; the mockup stays the
        // presentation and the host stays stateless.
        event => apply_page_event(hwnd, &event),
    }
}

/// Hide the companion window and forget any pending presentation: the next
/// presentation event (a new selection) re-shows it, and a size report the
/// page scheduled before the dismissal can no longer bring it back.
fn hide_surface(hwnd: HWND) {
    use std::sync::atomic::Ordering::Relaxed;
    WINDOW_SHOWN.store(false, Relaxed);
    SHOW_PENDING.store(false, Relaxed);
    CHOOSER_OVER_CARD.store(false, Relaxed);
    *PENDING_ANCHOR.lock().expect("anchor lock") = None;
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
    };
}

fn apply_page_event(hwnd: HWND, event: &UiEvent) {
    if let Ok(payload) = serde_json::to_string(event) {
        execute_script(
            hwnd,
            &format!(
                "(function(){{ try {{ if (!window.LINGO) return 'no-lingo'; \
                 window.LINGO.apply({payload}); return 'applied'; }} \
                 catch (e) {{ return 'err:' + e.message; }} }})()",
                payload = payload
            ),
        );
    }
}

fn create_webview(
    hwnd: HWND,
    surface: Surface,
    pipe: Option<pipe_connection::Pipe>,
    capture: Option<String>,
) -> bool {
    let (env_tx, env_rx) = std::sync::mpsc::channel::<Option<ICoreWebView2Environment>>();
    let env_handler = CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(
        move |error_code, environment| {
            error_code?;
            let _ = env_tx.send(environment);
            Ok(())
        },
    ));
    let user_data = user_data_dir();
    let result = unsafe {
        CreateCoreWebView2EnvironmentWithOptions(
            None,
            PCWSTR(user_data.as_ptr()),
            None,
            &env_handler,
        )
    };
    if result.is_err() {
        capture_log("webview: environment creation call failed");
        return false;
    }
    // WebView2 completions are delivered through this thread's message
    // loop, so the wait must pump messages instead of blocking.
    let Some(Some(environment)) = wait_with_pump(env_rx, std::time::Duration::from_secs(15)) else {
        capture_log("webview: environment timeout");
        return false;
    };

    let (ctl_tx, ctl_rx) = std::sync::mpsc::channel::<Option<ICoreWebView2Controller>>();
    let ctl_handler = CreateCoreWebView2ControllerCompletedHandler::create(Box::new(
        move |error_code, controller| {
            error_code?;
            let _ = ctl_tx.send(controller);
            Ok(())
        },
    ));
    if unsafe { environment.CreateCoreWebView2Controller(hwnd, &ctl_handler) }.is_err() {
        capture_log("webview: controller creation call failed");
        return false;
    }
    let Some(Some(controller)) = wait_with_pump(ctl_rx, std::time::Duration::from_secs(15)) else {
        capture_log("webview: controller timeout");
        return false;
    };
    unsafe {
        let mut rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut rect);
        let _ = controller.SetBounds(rect);
        // The controller's IsVisible defaults to false: without this call the
        // browser never composites a single pixel into the window — the
        // window shows (and blocks clicks) while the page stays invisible.
        let _ = controller.SetIsVisible(true);
        // Fully transparent browser background: with the window's
        // redirection bitmap disabled (WS_EX_NOREDIRECTIONBITMAP) the pixels
        // outside the rounded rail and card show the desktop underneath.
        if let Ok(controller2) = controller.cast::<ICoreWebView2Controller2>() {
            let _ = controller2.SetDefaultBackgroundColor(
                webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_COLOR {
                    A: 0,
                    R: 0,
                    G: 0,
                    B: 0,
                },
            );
        }
        // Rasterize the page at the monitor's device scale: with per-monitor
        // awareness the webview's default rasterization is 1.0, so at 250%
        // the card rendered as a narrow strip inside the window.
        if let Ok(controller3) = controller.cast::<ICoreWebView2Controller3>() {
            let scale = window_scale(hwnd);
            capture_log(&format!("webview: rasterization scale {scale}"));
            let _ = controller3.SetRasterizationScale(scale);
        }
    }
    let Ok(webview) = (unsafe { controller.CoreWebView2() }) else {
        capture_log("webview: CoreWebView2 getter failed");
        return false;
    };

    // On page load, tell the host it can replay any event that arrived while
    // the page was still navigating; in dev capture mode also save a PNG.
    let navigation_pipe = pipe.clone();
    let navigation_handler =
        webview2_com::NavigationCompletedEventHandler::create(Box::new(move |result, _| {
            let _ = result;
            capture_log("navigation completed");
            unsafe {
                let _ = KillTimer(Some(hwnd), LOAD_TIMEOUT_TIMER_ID);
            }
            // The window becomes visible only when content arrives (see
            // show_once), so an eagerly started companion never floats an
            // empty card over the desktop. Demo mode shows immediately: the
            // dev preview has no host events to wait for.
            if DEMO_MODE.load(std::sync::atomic::Ordering::Relaxed) {
                show_once(hwnd);
            } else if MANAGER_SURFACE.load(std::sync::atomic::Ordering::Relaxed) {
                // The manager surface has no content-size negotiation: the
                // resident already positioned it over the native manager's
                // rect, so show as soon as the page is ready.
                show_once(hwnd);
            } else {
                // Idle kill: a companion that loaded but never received
                // content must not linger forever; the host restarts it on
                // demand.
                unsafe {
                    let _ = SetTimer(Some(hwnd), IDLE_QUIT_TIMER_ID, IDLE_QUIT_MS, None);
                }
            }
            if let Some(pipe) = &navigation_pipe {
                if let Ok(line) = serde_json::to_string(&Envelope::new(0, UiCommand::PageReady)) {
                    let _ = pipe.write_line(&line);
                }
            }
            if let Some(path) = &capture {
                capture_page(hwnd, path);
                // The preview write completes asynchronously; the completion
                // handler inside capture_page closes the window.
            }
            Ok(())
        }));
    unsafe {
        let mut token = 0i64;
        let _ = webview.add_NavigationCompleted(&navigation_handler, &mut token);
    }
    let page = inlined_page(surface);
    if unsafe { webview.NavigateToString(&windows::core::HSTRING::from(page)) }.is_err() {
        capture_log("webview: NavigateToString failed");
        return false;
    }
    capture_log("webview: navigation started");
    // Page buttons post {"cmd": ...} messages; forward them to the resident.
    let message_pipe = pipe.clone();
    let message_handler =
        webview2_com::WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let mut raw = windows::core::PWSTR::null();
            unsafe { args.TryGetWebMessageAsString(&mut raw) }?;
            let message = take_pwstr(raw);
            // The page reports its visible content height after every render;
            // the host shrinks or grows the window so the floating card never
            // sits inside an oversized slab that blocks the desktop.
            #[derive(serde::Deserialize)]
            struct ContentSize {
                w: f64,
                h: f64,
            }
            if let Ok(size) = serde_json::from_str::<ContentSize>(&message) {
                // A size report scheduled before a dismissal (the page
                // re-reports after 120/400 ms and on font load) must not
                // resize or re-show a surface the resident already closed.
                if !SHOW_PENDING.load(std::sync::atomic::Ordering::Relaxed)
                    && !MANAGER_SURFACE.load(std::sync::atomic::Ordering::Relaxed)
                    && !DEMO_MODE.load(std::sync::atomic::Ordering::Relaxed)
                {
                    return Ok(());
                }
                let scale = window_scale(hwnd);
                let mut rect = RECT::default();
                let _ = unsafe { GetWindowRect(hwnd, &mut rect) };
                let width = ((size.w * scale).round() as i32).clamp(48, 3000);
                // Clamp the height to the window's monitor work area: a long
                // analysis scrolls inside the card instead of running past
                // the screen edge.
                let mut info = MONITORINFO {
                    cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                    ..Default::default()
                };
                let _ = unsafe {
                    GetMonitorInfoW(
                        MonitorFromPoint(
                            POINT {
                                x: rect.left,
                                y: rect.top,
                            },
                            MONITOR_DEFAULTTONEAREST,
                        ),
                        &mut info,
                    )
                };
                let max_h = (info.rcWork.bottom - info.rcWork.top - 20).max(200);
                let height = ((size.h * scale).round() as i32).clamp(48, max_h);
                // Position: the pending anchor owns it, computed with the
                // NEW size so the clamp uses the settled geometry. The anchor
                // is re-applied on every settled report — the first one races
                // the height change; the last one lands the window correctly.
                let pending = *PENDING_ANCHOR.lock().expect("anchor lock");
                let (x, y) = match pending {
                    Some(Placement::Anchor { x, y, above }) => {
                        anchored_position((x, y), above, width, height, &info)
                    }
                    Some(Placement::KeepBottom { left, bottom }) => {
                        let top = (bottom - height).max(info.rcWork.top);
                        (left, top)
                    }
                    None => (rect.left, rect.top),
                };
                // No-op guard: the settled re-posts repeat the same size AND
                // position — resizing/moving a live WebView2 window forces a
                // full recomposite, and repeated identical geometry is the
                // flicker the user sees. Only apply real changes, in ONE call
                // (a separate resize then move shows as a visible two-step).
                let same = width == rect.right - rect.left
                    && height == rect.bottom - rect.top
                    && x == rect.left
                    && y == rect.top;
                if !same {
                    unsafe {
                        let _ = SetWindowPos(
                            hwnd,
                            Some(HWND_TOPMOST),
                            x,
                            y,
                            width,
                            height,
                            SWP_NOACTIVATE,
                        );
                    }
                }
                // The geometry is settled: this is the moment the window may
                // appear — showing earlier (on the raw event) would flash it
                // at the pre-anchor position and jump.
                show_once(hwnd);
                return Ok(());
            }
            // Manager messages can carry an API key being saved; never log
            // their content.
            if !MANAGER_SURFACE.load(std::sync::atomic::Ordering::Relaxed) {
                capture_log(&format!("fwd: {:.90}", message));
            }
            // Window controls of the borderless manager page are handled
            // here, not by the host.
            if MANAGER_SURFACE.load(std::sync::atomic::Ordering::Relaxed) {
                if message.contains("\"window_minimize\"") {
                    unsafe {
                        let _ =
                            ShowWindow(hwnd, windows::Win32::UI::WindowsAndMessaging::SW_MINIMIZE);
                    }
                    return Ok(());
                }
                if message.contains("\"window_close\"") {
                    // Ending the page ends the manager (it watches this
                    // process), like closing the native manager window.
                    unsafe {
                        let _ = DestroyWindow(hwnd);
                    }
                    return Ok(());
                }
            }
            // Drag hand-off (both surfaces): the page posts begin_drag once
            // its press-move threshold is crossed; the host releases the
            // webview's mouse capture and hands the move loop to Windows via
            // the HTCAPTION modal drag, which follows the cursor and swallows
            // the mouseup (a clean press-release stays a click).
            #[derive(serde::Deserialize)]
            struct BeginDrag {}
            if serde_json::from_str::<BeginDrag>(&message)
                .map(|_| message.contains("begin_drag"))
                .unwrap_or(false)
            {
                // The drag owns the position from here: a kept anchor would
                // re-apply on the next settled report and snap the card back
                // to the selection mid-drag.
                *PENDING_ANCHOR.lock().expect("anchor lock") = None;
                let mut pt = POINT::default();
                let _ = unsafe { GetCursorPos(&mut pt) };
                let lparam = (((pt.y as u32) & 0xffff) << 16 | ((pt.x as u32) & 0xffff)) as i32;
                unsafe {
                    let _ = ReleaseCapture();
                    let _ = PostMessageW(
                        Some(hwnd),
                        WM_NCLBUTTONDOWN,
                        WPARAM(HTCAPTION as usize),
                        LPARAM(lparam as isize),
                    );
                }
                return Ok(());
            }
            if MANAGER_SURFACE.load(std::sync::atomic::Ordering::Relaxed) {
                // Manager page commands are forwarded verbatim.
                if let Some(pipe) = &message_pipe {
                    let _ = pipe.write_line(&message);
                }
            } else if let Ok(command) = serde_json::from_str::<PageCommand>(&message) {
                // The page's Close button is a user close: hide the window
                // exactly like Esc does, in addition to notifying the host so
                // it can retire its native bookkeeping popup.
                let is_close = matches!(command, PageCommand::Close);
                // The resident parses Envelope<UiCommand> (seq + payload), so
                // the bare command must be wrapped — an unwrapped command is
                // silently dropped by the resident's parser.
                let command = command.into_protocol(0);
                if let Ok(line) = serde_json::to_string(&Envelope::new(0, command)) {
                    if let Some(pipe) = &message_pipe {
                        let _ = pipe.write_line(&line);
                    }
                }
                if is_close && !CHOOSER_OVER_CARD.load(std::sync::atomic::Ordering::Relaxed) {
                    hide_surface(hwnd);
                }
            }
            Ok(())
        }));
    unsafe {
        let mut token = 0i64;
        let _ = webview.add_WebMessageReceived(&message_handler, &mut token);
    }
    unsafe {
        SetWindowLongPtrW(
            hwnd,
            WEBVIEW_SLOT,
            Box::into_raw(Box::new(webview)) as isize,
        );
        // Keep the controller alive for the window's lifetime; released on
        // WM_DESTROY alongside the webview.
        SetWindowLongPtrW(
            hwnd,
            CONTROLLER_SLOT,
            Box::into_raw(Box::new(controller)) as isize,
        );
    }
    true
}

fn drop_webview(hwnd: HWND) {
    // Recover each slot with its real type so the COM reference is released
    // and the allocation is freed with the layout it was created with.
    let raw = unsafe { GetWindowLongPtrW(hwnd, WEBVIEW_SLOT) };
    if raw != 0 {
        unsafe {
            SetWindowLongPtrW(hwnd, WEBVIEW_SLOT, 0);
            drop(Box::from_raw(raw as *mut ICoreWebView2));
        }
    }
    let raw = unsafe { GetWindowLongPtrW(hwnd, CONTROLLER_SLOT) };
    if raw != 0 {
        unsafe {
            SetWindowLongPtrW(hwnd, CONTROLLER_SLOT, 0);
            let controller = Box::from_raw(raw as *mut ICoreWebView2Controller);
            let _ = controller.Close();
            drop(controller);
        }
    }
}

fn user_data_dir() -> Vec<u16> {
    // Keep WebView2's user-data folder beside the other local app data.
    let base = std::env::var_os("LOCALAPPDATA")
        .map(|value| {
            std::path::PathBuf::from(value)
                .join("SelectionTranslate")
                .join("webview")
        })
        .unwrap_or_else(|| std::env::temp_dir().join("SelectionTranslateWebview"));
    let _ = std::fs::create_dir_all(&base);
    format!("{}\0", base.display()).encode_utf16().collect()
}

/// A command posted by the page bridge (`window.chrome.webview.postMessage`).
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
enum PageCommand {
    Copy { text: String },
    Retry,
    Close,
    Pin { pinned: bool },
    SelectProfile { index: usize },
    SelfTranslate { text: String },
}

impl PageCommand {
    fn into_protocol(self, id: u64) -> UiCommand {
        match self {
            Self::Copy { text } => UiCommand::Copy { id, text },
            Self::Retry => UiCommand::Retry { id },
            Self::Close => UiCommand::UserClose { id },
            Self::Pin { pinned } => UiCommand::TogglePin { id, pinned },
            Self::SelectProfile { index } => UiCommand::SelectProfile { id, index },
            Self::SelfTranslate { text } => UiCommand::SelfTranslate { id, text },
        }
    }
}

fn take_pwstr(raw: windows::core::PWSTR) -> String {
    unsafe {
        if raw.0.is_null() {
            return String::new();
        }
        let mut length = 0usize;
        while *raw.0.add(length) != 0 {
            length += 1;
        }
        let slice = std::slice::from_raw_parts(raw.0, length);
        let text = String::from_utf16_lossy(slice);
        windows::Win32::System::Com::CoTaskMemFree(Some(raw.0 as *const _));
        text
    }
}

/// Dev diagnostics go to a file because the GUI subsystem has no console.
fn capture_log(message: &str) {
    use std::io::Write;
    // Log next to the executable: the process CWD varies with who spawned us
    // (resident, manager, or a dev probe), which silently scattered logs.
    let Some(path) = std::env::var_os("LOCALAPPDATA").map(|base| {
        std::path::PathBuf::from(base)
            .join("SelectionTranslate")
            .join("popup-ui-debug.log")
    }) else {
        return;
    };
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let elapsed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let _ = writeln!(file, "[{elapsed}] {message}");
    }
}

/// Receive a completion while pumping the thread's message queue. WebView2
/// delivers its async completions as window messages on the calling STA
/// thread, so a plain blocking recv would deadlock.
fn wait_with_pump<T>(
    receiver: std::sync::mpsc::Receiver<T>,
    timeout: std::time::Duration,
) -> Option<T> {
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, TranslateMessage, PM_REMOVE, WM_QUIT,
    };
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match receiver.try_recv() {
            Ok(value) => return Some(value),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => return None,
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        unsafe {
            let mut message = MSG::default();
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                // A WM_QUIT peeked here would silently swallow a
                // PostQuitMessage from a timer (for example the load-timeout
                // firing while the environment is still starting), leaving
                // the process alive but never quitting later. Treat it as an
                // abort of the wait; the caller then tears down.
                if message.message == WM_QUIT {
                    return None;
                }
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// Dev-only: write a PNG preview of the current page.
fn capture_page(hwnd: HWND, path: &str) {
    capture_log("capture: capture_page entered");
    // `hwnd` is moved into the completion handler below to close the window
    // once the PNG is on disk.
    use webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG;
    use windows::Win32::System::Com::{STGM_CREATE, STGM_READWRITE};
    use windows::Win32::UI::Shell::SHCreateStreamOnFileEx;
    let raw = unsafe { GetWindowLongPtrW(hwnd, WEBVIEW_SLOT) } as *mut ICoreWebView2;
    if raw.is_null() {
        capture_log("capture: no webview slot");
        return;
    }
    let webview = unsafe { &*raw };
    let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    let Ok(stream) = (unsafe {
        SHCreateStreamOnFileEx(
            windows::core::PCWSTR(wide.as_ptr()),
            (STGM_CREATE | STGM_READWRITE).0,
            windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL.0,
            false,
            None,
        )
    }) else {
        capture_log("capture: stream open failed");
        return;
    };
    let handler = webview2_com::CapturePreviewCompletedHandler::create(Box::new(|result| {
        let _ = result;
        Ok(())
    }));
    unsafe {
        let result = webview.CapturePreview(
            COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG,
            &stream,
            &handler,
        );
        capture_log(&format!(
            "capture: CapturePreview call result ok={}",
            result.is_ok()
        ));
    }
}

mod pipe_connection {
    use super::*;

    /// Byte-mode duplex named pipe client. Clones share the same handle
    /// storage; writes serialize through the mutex so the webview handler
    /// and the reader thread never interleave lines.
    pub struct Pipe(Arc<Mutex<std::fs::File>>);

    impl Clone for Pipe {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }

    impl Pipe {
        /// Pull every byte currently buffered in the pipe without ever
        /// parking in a blocking ReadFile. A synchronous file object allows
        /// only one outstanding I/O, so a parked read here would freeze every
        /// write from the webview handlers (and, mirrored on the host, every
        /// event the resident sends). `Ok(vec![])` means no data yet; `Err`
        /// means the peer hung up or the pipe broke.
        pub fn read_available_bytes(&self) -> std::io::Result<Vec<u8>> {
            use std::os::windows::io::AsRawHandle;
            use windows::Win32::Storage::FileSystem::ReadFile;
            use windows::Win32::System::Pipes::PeekNamedPipe;
            let file = self.0.lock().expect("pipe lock");
            let handle = windows::Win32::Foundation::HANDLE(file.as_raw_handle() as _);
            let mut available = 0u32;
            unsafe {
                PeekNamedPipe(handle, None, 0, None, Some(&mut available), None)
                    .map_err(|_| std::io::Error::other("pipe peek failed (peer closed?)"))?;
            }
            if available == 0 {
                return Ok(Vec::new());
            }
            let mut chunk = vec![0u8; available as usize];
            let mut read_total = 0u32;
            unsafe {
                ReadFile(handle, Some(&mut chunk[..]), Some(&mut read_total), None)
                    .map_err(|error| std::io::Error::from_raw_os_error(error.code().0))?;
            }
            chunk.truncate(read_total as usize);
            Ok(chunk)
        }

        pub fn write_line(&self, line: &str) -> std::io::Result<()> {
            let mut file = self.0.lock().expect("pipe lock");
            file.write_all(line.as_bytes())?;
            file.write_all(b"\n")?;
            file.flush()
        }
    }

    pub fn connect(manager: bool) -> Result<Pipe, ()> {
        // Test harnesses override the name so they can exercise the real
        // client without colliding with a live resident.
        let name = std::env::var("LINGOSNAP_PIPE_NAME").unwrap_or_else(|_| {
            if manager {
                MANAGER_PIPE_NAME.to_owned()
            } else {
                PIPE_NAME.to_owned()
            }
        });
        for _ in 0..50 {
            let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
            if let Ok(handle) = unsafe {
                CreateFileW(
                    PCWSTR(wide.as_ptr()),
                    (GENERIC_READ | GENERIC_WRITE).0,
                    FILE_SHARE_NONE,
                    None,
                    OPEN_EXISTING,
                    FILE_ATTRIBUTE_NORMAL,
                    None,
                )
            } {
                use std::os::windows::io::FromRawHandle;
                let file = unsafe { std::fs::File::from_raw_handle(handle.0 as _) };
                return Ok(Pipe(Arc::new(Mutex::new(file))));
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        Err(())
    }
}

mod pipe_thread {
    use super::*;

    pub fn spawn(hwnd: HWND, pipe: pipe_connection::Pipe, surface: Surface) {
        // HWND is not Send; the thread only needs the raw handle value to
        // post messages back to the window.
        let target = hwnd.0 as isize;
        std::thread::spawn(move || {
            let hwnd = HWND(target as *mut _);
            let _ =
                pipe.write_line(&serde_json::to_string(&Handshake::ready()).expect("handshake"));
            let _ = pipe.write_line(
                &serde_json::to_string(&Envelope::new(
                    1,
                    UiCommand::RegisterWindow { id: 0, surface },
                ))
                .expect("register"),
            );
            // Poll-based line reader. A blocking `lines()` on a synchronous
            // pipe handle would park the file object and freeze every write
            // (see `read_available_bytes`); polling keeps the pipe usable in
            // both directions. After two seconds without traffic a Ping is
            // sent: a dead host (crashed manager or resident) surfaces as a
            // failed write and the companion exits instead of floating a
            // window nobody owns anymore.
            let mut pending: Vec<u8> = Vec::new();
            let mut idle_since: Option<std::time::Instant> = None;
            'read: while let Ok(chunk) = pipe.read_available_bytes() {
                if chunk.is_empty() {
                    let now = std::time::Instant::now();
                    let idle = *idle_since.get_or_insert(now);
                    if now.duration_since(idle) > std::time::Duration::from_secs(2) {
                        let ping = serde_json::to_string(&Envelope::new(0, UiCommand::Ping))
                            .expect("ping");
                        if pipe.write_line(&ping).is_err() {
                            capture_log("poll: ping write failed, host gone");
                            break 'read;
                        }
                        idle_since = Some(now);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(25));
                    continue;
                }
                idle_since = None;
                pending.extend_from_slice(&chunk);
                let mut consumed = 0usize;
                for (index, byte) in pending.iter().enumerate() {
                    if *byte != b'\n' {
                        continue;
                    }
                    let line = String::from_utf8_lossy(&pending[consumed..index]).into_owned();
                    consumed = index + 1;
                    if line.trim().is_empty() {
                        continue;
                    }
                    let boxed = Box::into_raw(Box::new(line));
                    if unsafe {
                        PostMessageW(Some(hwnd), WM_APP_EVENT, WPARAM(0), LPARAM(boxed as isize))
                    }
                    .is_err()
                    {
                        unsafe { drop(Box::from_raw(boxed)) };
                        pending.clear();
                        break 'read;
                    }
                }
                pending.drain(..consumed);
                if pending.len() > 64 * 1024 {
                    // Protocol framing broke down; drop the garbage rather
                    // than growing without bound.
                    pending.clear();
                }
            }
            // The resident hung up: exit so the process never lingers.
            capture_log("exit: pipe reader ended, posting shutdown");
            unsafe {
                let _ = PostMessageW(
                    Some(hwnd),
                    WM_APP_EVENT,
                    WPARAM(0),
                    LPARAM(Box::into_raw(Box::new(
                        serde_json::to_string(&UiEvent::Shutdown).expect("shutdown"),
                    )) as isize),
                );
            }
        });
    }
}
