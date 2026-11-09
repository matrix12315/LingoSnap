//! Fair-copy UI companion: hosts the mockup pages in WebView2 and speaks
//! protocol v1 with the resident over the named pipe. See FAIR_COPY_UI_PLAN.md.
//!
//! Every popup is its own window, exactly like the classic skin: a chooser
//! raised from a card opens a new window beside it, a pinned card stays while
//! new ones appear, and closing one never touches the others. Windows share
//! one WebView2 environment and are pooled: a closed popup's window is
//! cleared and kept hidden for the next popup, so a selection never waits for
//! a fresh page load.

#![cfg_attr(windows, windows_subsystem = "windows")]
#![cfg(windows)]

use selection_ui_protocol::{
    Anchor, Envelope, Handshake, Surface, UiCommand, UiEvent, MANAGER_PIPE_NAME, PIPE_NAME,
};
use std::cell::{Cell, RefCell};
use std::io::Write;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use webview2_com::Microsoft::Web::WebView2::Win32::{
    CreateCoreWebView2EnvironmentWithOptions, ICoreWebView2, ICoreWebView2Controller,
    ICoreWebView2Controller2, ICoreWebView2Controller3, ICoreWebView2Environment, ICoreWebView2_19,
    COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
    COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
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
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, ReleaseCapture, VK_LBUTTON};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetCursorPos,
    GetMessageW, GetWindowRect, IsWindowVisible, KillTimer, LoadCursorW, MessageBoxW, PostMessageW,
    PostQuitMessage, RegisterClassW, SetCursor, SetForegroundWindow, SetTimer, SetWindowPos,
    ShowWindow, TranslateMessage, CS_HREDRAW, CS_VREDRAW, HTCAPTION, HWND_MESSAGE, HWND_TOPMOST,
    IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE, IDC_SIZEWE, MB_ICONERROR, MSG, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SW_HIDE, SW_SHOW, SW_SHOWNOACTIVATE,
    WM_APP, WM_DESTROY, WM_DPICHANGED, WM_NCLBUTTONDOWN, WM_SIZE, WM_TIMER, WNDCLASSW,
    WS_EX_APPWINDOW, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_MINIMIZEBOX,
    WS_POPUP, WS_SYSMENU,
};

/// Host message window: one boxed protocol line in lparam.
const WM_APP_EVENT: u32 = WM_APP + 1;

// Timers on the host message window (process-wide).
/// Dev mode (--hold=ms) quits after the hold so the process can be measured.
const DEMO_QUIT_TIMER_ID: usize = 1;
const DEMO_FORCE_QUIT_TIMER_ID: usize = 2;
/// The first page must load in time, or the process exits and the resident
/// falls back to the classic popup.
const LOAD_TIMEOUT_TIMER_ID: usize = 3;
/// A companion that loaded but never presented anything exits after this
/// long (the plan's 600 s idle kill); the host restarts it on demand.
const IDLE_QUIT_TIMER_ID: usize = 4;
const IDLE_QUIT_MS: u32 = 600_000;

// Timers on a surface window.
/// Show a pending surface once its geometry has been quiet this long: the
/// page re-reports its size as fonts and layout settle, and the window must
/// not appear before the last report placed it.
const SHOW_TIMER_ID: usize = 11;
const SHOW_SETTLE_MS: u32 = 45;
/// Hide a released window only after its cleared page has painted, so the
/// next time it is shown it never flashes the previous card.
const HIDE_TIMER_ID: usize = 12;
const HIDE_AFTER_MS: u32 = 60;
/// Edge-resize loop (the page hands the press over; the host follows the
/// cursor until the button is released).
const RESIZE_TIMER_ID: usize = 13;
/// The manager page shows when it reports its first painted frame; this is
/// the fallback if that report never comes.
const MANAGER_SHOW_TIMER_ID: usize = 14;

/// Idle windows kept for reuse; more are destroyed. One is enough for the
/// next popup to open without a page load; each extra pooled window kept a
/// renderer (~20 MiB) alive.
const MAX_IDLE_WINDOWS: usize = 1;
/// Body padding of the popup page (APP_CSS): 6px top, 10px bottom, 8px sides.
const BODY_PAD_W: f64 = 16.0;
const BODY_PAD_H: f64 = 16.0;
/// Smallest card a user may resize to (CSS px).
const MIN_CARD_W: f64 = 320.0;
const MIN_CARD_H: f64 = 180.0;

static DEMO_MODE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// One surface kind per process: the manager page, or popups.
static MANAGER_SURFACE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static SHARED_PIPE: std::sync::Mutex<Option<pipe_connection::Pipe>> = std::sync::Mutex::new(None);
/// Dev capture target.
static PENDING_CAPTURE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

// Keep in sync with selection_ui_protocol::COMPANION_WINDOW_CLASS (the
// resident matches clicks against this class) and MANAGER_WINDOW_CLASS.
const CLASS_NAME: PCWSTR = w!("SelectionTranslatePopupUi");
const MANAGER_CLASS_NAME: PCWSTR = w!("SelectionTranslateManagerUi");
const HOST_CLASS_NAME: PCWSTR = w!("SelectionTranslatePopupUiHost");

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

fn is_manager_surface(surface: Surface) -> bool {
    surface == Surface::Manager
}

fn manager_mode() -> bool {
    MANAGER_SURFACE.load(std::sync::atomic::Ordering::Relaxed)
}

fn demo_mode() -> bool {
    DEMO_MODE.load(std::sync::atomic::Ordering::Relaxed)
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

// —— Surface windows ——

/// Where the next settled size report places a window.
#[derive(Clone, Copy, Debug)]
enum Placement {
    /// Centered on the anchor x, above (chooser) or below (card) anchor y.
    Anchor { x: i32, y: i32, above: bool },
    /// Beside a source card (self-translation), like the classic cascade:
    /// to its right when it fits, otherwise to its left, top-aligned.
    Beside { source: RECT },
}

/// An edge-resize in progress (edge bits: 1 left, 2 top, 4 right, 8 bottom).
#[derive(Clone, Copy, Debug)]
struct Resize {
    edges: u8,
    cursor: POINT,
    rect: RECT,
}

/// One popup (or the manager) window and its page.
struct Win {
    hwnd: HWND,
    surface: Surface,
    webview: RefCell<Option<ICoreWebView2>>,
    controller: RefCell<Option<ICoreWebView2Controller>>,
    /// The resident's popup id this window presents; 0 while pooled.
    popup_id: Cell<u64>,
    /// The page finished loading; earlier scripts wait in `queue`.
    ready: Cell<bool>,
    queue: RefCell<Vec<String>>,
    placement: Cell<Option<Placement>>,
    /// A presentation is waiting to be shown (or is shown).
    show_pending: Cell<bool>,
    shown: Cell<bool>,
    /// A result card is on screen: a retry keeps its position.
    card: Cell<bool>,
    /// Popup id of the card this chooser was raised from.
    cascade_from: Cell<Option<u64>>,
    resize: Cell<Option<Resize>>,
    /// Window size the user chose by dragging an edge (physical px).
    user_size: Cell<Option<(i32, i32)>>,
    /// Current WebView controller bounds (physical px). Popups keep them a
    /// little larger than the window so a growing card never waits for the
    /// viewport, but no larger: the GPU process allocates by viewport area.
    bounds: Cell<(i32, i32)>,
}

thread_local! {
    static WINS: RefCell<Vec<Rc<Win>>> = const { RefCell::new(Vec::new()) };
    static ENVIRONMENT: RefCell<Option<ICoreWebView2Environment>> = const { RefCell::new(None) };
    static HOST: Cell<HWND> = const { Cell::new(HWND(std::ptr::null_mut())) };
    static INSTANCE: Cell<isize> = const { Cell::new(0) };
    static FIRST_LOAD: Cell<bool> = const { Cell::new(false) };
    /// The card size (CSS px) the user last dragged a popup to. Like the
    /// classic popup's remembered window size, every later result card opens
    /// at it for the rest of the session.
    static LAST_CARD_SIZE: Cell<Option<(f64, f64)>> = const { Cell::new(None) };
}

/// Open a new result card at the size the user last chose, if any.
fn apply_remembered_size(win: &Win) {
    if win.user_size.get().is_some() {
        return;
    }
    let Some((css_w, css_h)) = LAST_CARD_SIZE.with(Cell::get) else {
        return;
    };
    let scale = window_scale(win.hwnd);
    let width = ((css_w + BODY_PAD_W) * scale).round() as i32;
    let height = ((css_h + BODY_PAD_H) * scale).round() as i32;
    win.user_size.set(Some((width, height)));
    execute_script(
        win,
        &format!("window.LINGO && window.LINGO.userSize({css_w}, {css_h}); undefined"),
    );
}

fn wins() -> Vec<Rc<Win>> {
    WINS.with(|wins| wins.borrow().clone())
}

fn win_for(hwnd: HWND) -> Option<Rc<Win>> {
    WINS.with(|wins| wins.borrow().iter().find(|win| win.hwnd == hwnd).cloned())
}

fn win_by_id(id: u64) -> Option<Rc<Win>> {
    if id == 0 {
        return None;
    }
    WINS.with(|wins| {
        wins.borrow()
            .iter()
            .find(|win| win.popup_id.get() == id)
            .cloned()
    })
}

fn manager_win() -> Option<Rc<Win>> {
    WINS.with(|wins| {
        wins.borrow()
            .iter()
            .find(|win| win.surface == Surface::Manager)
            .cloned()
    })
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

fn window_rect(hwnd: HWND) -> RECT {
    let mut rect = RECT::default();
    let _ = unsafe { GetWindowRect(hwnd, &mut rect) };
    rect
}

fn cursor_anchor() -> POINT {
    let mut point = POINT::default();
    let _ = unsafe { GetCursorPos(&mut point) };
    let work = monitor_info_at(point).rcWork;
    let x = point.x.clamp(work.left, (work.right - 460).max(work.left));
    let y = point.y.clamp(work.top, (work.bottom - 200).max(work.top));
    POINT { x, y }
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

fn main() {
    capture_log(&format!(
        "start: pid={} exe={:?}",
        std::process::id(),
        std::env::current_exe().map(|e| e.display().to_string()),
    ));
    // Per-monitor aware V2: cards render at the monitor's device scale and
    // follow selections on ANY monitor; everything below is physical pixels.
    let _ = unsafe {
        windows::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(
            windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        )
    };
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
    let args: Vec<String> = std::env::args().collect();
    let mut surface = Surface::Popup;
    let mut capture: Option<String> = None;
    let mut hold_ms: Option<u32> = None;
    if let Some(index) = args.iter().position(|arg| arg == "--surface") {
        surface = parse_surface(args.get(index + 1).map(String::as_str));
    }
    for (index, arg) in args.iter().enumerate() {
        if let Some(value) = arg.strip_prefix("--capture=") {
            capture = Some(value.to_owned());
        } else if arg == "--capture" {
            capture = args.get(index + 1).cloned();
        } else if let Some(value) = arg.strip_prefix("--hold=") {
            hold_ms = value.parse::<u32>().ok();
        } else if arg == "--hold" {
            hold_ms = args.get(index + 1).and_then(|value| value.parse().ok());
        }
    }
    let Ok(instance) = (unsafe { GetModuleHandleW(None) }) else {
        fatal("Could not acquire the module handle.");
        return;
    };
    INSTANCE.with(|cell| cell.set(instance.0 as isize));
    for (class_name, proc_fn) in [
        (
            CLASS_NAME,
            surface_proc as unsafe extern "system" fn(_, _, _, _) -> _,
        ),
        (MANAGER_CLASS_NAME, surface_proc),
        (HOST_CLASS_NAME, host_proc),
    ] {
        let class = WNDCLASSW {
            lpfnWndProc: Some(proc_fn),
            hInstance: instance.into(),
            lpszClassName: class_name,
            style: CS_HREDRAW | CS_VREDRAW,
            ..Default::default()
        };
        if unsafe { RegisterClassW(&class) } == 0 {
            fatal("Could not register the fair-copy UI window class.");
            return;
        }
    }
    // A message-only window receives the pipe lines and process timers, so
    // popup windows can come and go freely.
    let Ok(host) = (unsafe {
        CreateWindowExW(
            Default::default(),
            HOST_CLASS_NAME,
            PCWSTR::null(),
            Default::default(),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(instance.into()),
            None,
        )
    }) else {
        fatal("Could not create the fair-copy UI host window.");
        return;
    };
    HOST.with(|cell| cell.set(host));
    *PENDING_CAPTURE.lock().expect("capture lock") = capture;
    let demo = hold_ms.is_some();
    DEMO_MODE.store(demo, std::sync::atomic::Ordering::Relaxed);
    MANAGER_SURFACE.store(
        is_manager_surface(surface),
        std::sync::atomic::Ordering::Relaxed,
    );
    let pipe = match pipe_connection::connect(is_manager_surface(surface)) {
        Ok(pipe) => Some(pipe),
        Err(_) if demo => None,
        Err(_) => {
            capture_log("exit: pipe connect failed");
            return;
        }
    };
    *SHARED_PIPE.lock().expect("pipe lock") = pipe.clone();
    if let Some(hold) = hold_ms {
        unsafe {
            let _ = SetTimer(Some(host), DEMO_QUIT_TIMER_ID, hold, None);
        }
    }
    unsafe {
        let _ = SetTimer(Some(host), LOAD_TIMEOUT_TIMER_ID, 15_000, None);
    }
    // The first window is created up front (the manager page, or a pooled
    // popup window that the first selection uses without a page load).
    if create_surface(surface).is_none() {
        capture_log("exit: first surface failed");
        return;
    }
    if let Some(pipe) = pipe {
        pipe_thread::spawn(host, pipe, surface);
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
    for win in wins() {
        unsafe {
            let _ = DestroyWindow(win.hwnd);
        }
    }
}

fn environment() -> Option<ICoreWebView2Environment> {
    if let Some(environment) = ENVIRONMENT.with(|cell| cell.borrow().clone()) {
        return Some(environment);
    }
    let (tx, rx) = std::sync::mpsc::channel::<Option<ICoreWebView2Environment>>();
    let handler = CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(
        move |error_code, environment| {
            error_code?;
            let _ = tx.send(environment);
            Ok(())
        },
    ));
    let user_data = user_data_dir();
    let options = webview2_com::CoreWebView2EnvironmentOptions::default();
    unsafe { options.set_additional_browser_arguments(browser_arguments()) };
    let options: webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2EnvironmentOptions =
        options.into();
    let started = unsafe {
        CreateCoreWebView2EnvironmentWithOptions(
            None,
            PCWSTR(user_data.as_ptr()),
            &options,
            &handler,
        )
    };
    if started.is_err() {
        capture_log("webview: environment creation call failed");
        return None;
    }
    let environment = wait_with_pump(rx, std::time::Duration::from_secs(15)).flatten()?;
    ENVIRONMENT.with(|cell| *cell.borrow_mut() = Some(environment.clone()));
    Some(environment)
}

/// Create one surface window with its page. Returns None (and leaves no
/// window behind) when the WebView2 runtime cannot host it.
fn create_surface(surface: Surface) -> Option<Rc<Win>> {
    let manager = is_manager_surface(surface);
    let instance = INSTANCE.with(Cell::get);
    let anchor = cursor_anchor();
    let (base_w, base_h) = window_size(surface);
    // No redirection bitmap: the window has no opaque surface of its own,
    // so the transparent page composites onto the desktop and the rail and
    // card keep their rounded paper shape. Popups are topmost tool windows
    // (no taskbar button), like the classic popup; the manager page is the
    // manager's normal, taskbar-visible main window.
    let (class_name, ex_style, style, title) = if manager {
        (
            MANAGER_CLASS_NAME,
            WS_EX_APPWINDOW | WS_EX_NOREDIRECTIONBITMAP,
            WS_POPUP | WS_SYSMENU | WS_MINIMIZEBOX,
            "Selection Translate — Manager",
        )
    } else {
        (
            CLASS_NAME,
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOREDIRECTIONBITMAP,
            WS_POPUP,
            "LingoSnap fair-copy",
        )
    };
    let title: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
    let hwnd = unsafe {
        CreateWindowExW(
            ex_style,
            class_name,
            PCWSTR(title.as_ptr()),
            style,
            anchor.x,
            anchor.y,
            base_w,
            base_h,
            None,
            None,
            Some(windows::Win32::Foundation::HINSTANCE(instance as *mut _)),
            None,
        )
    }
    .ok()?;
    let scale = window_scale(hwnd);
    let scaled_w = (base_w as f64 * scale).round() as i32;
    let scaled_h = (base_h as f64 * scale).round() as i32;
    if manager {
        // Open centered on the monitor under the cursor, like an app window.
        let work = monitor_info_at(anchor).rcWork;
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
    let win = Rc::new(Win {
        hwnd,
        surface,
        webview: RefCell::new(None),
        controller: RefCell::new(None),
        popup_id: Cell::new(0),
        ready: Cell::new(false),
        queue: RefCell::new(Vec::new()),
        placement: Cell::new(None),
        show_pending: Cell::new(false),
        shown: Cell::new(false),
        card: Cell::new(false),
        cascade_from: Cell::new(None),
        resize: Cell::new(None),
        user_size: Cell::new(None),
        bounds: Cell::new((0, 0)),
    });
    WINS.with(|wins| wins.borrow_mut().push(win.clone()));
    if !attach_webview(&win) {
        capture_log("webview: attach failed");
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        return None;
    }
    Some(win)
}

/// Viewport headroom beyond a popup window (CSS px): a card that grows while
/// streaming, or a chooser turning into its card, lands inside pixels the
/// browser has already painted, so no black or white block shows while the
/// window catches up. The bounds grow before the window does.
const BOUNDS_HEADROOM_W: f64 = 32.0;
const BOUNDS_HEADROOM_H: f64 = 240.0;

/// Controller bounds for a window of `size` (physical px). The manager page
/// fills its window. A popup's viewport is the window plus headroom, capped
/// to the monitor work area. (r58–r61 used the whole work area, which cost
/// the GPU process ~200 MiB per window.)
fn controller_bounds(win: &Win, size: (i32, i32)) -> RECT {
    if win.surface == Surface::Manager {
        let mut rect = RECT::default();
        let _ = unsafe { GetClientRect(win.hwnd, &mut rect) };
        return rect;
    }
    let rect = window_rect(win.hwnd);
    let work = monitor_info_at(POINT {
        x: rect.left,
        y: rect.top,
    })
    .rcWork;
    let scale = window_scale(win.hwnd);
    let (w, h) = (size.0.max(1), size.1.max(1));
    let want_w = w + (BOUNDS_HEADROOM_W * scale).round() as i32;
    let want_h = h + (BOUNDS_HEADROOM_H * scale).round() as i32;
    RECT {
        left: 0,
        top: 0,
        right: want_w.min(work.right - work.left).max(w),
        bottom: want_h.min(work.bottom - work.top).max(h),
    }
}

/// Make sure a popup's viewport covers a window of `size` before the window
/// takes that size. `force` re-fits even when the bounds already cover it
/// (a hidden window shrinking back, a DPI change).
fn fit_bounds(win: &Win, size: (i32, i32), force: bool) {
    if win.surface == Surface::Manager {
        return;
    }
    let (bw, bh) = win.bounds.get();
    if !force && size.0 <= bw && size.1 <= bh {
        return;
    }
    let Some(controller) = win.controller.borrow().clone() else {
        return;
    };
    let rect = controller_bounds(win, size);
    if (rect.right, rect.bottom) == (bw, bh) {
        return;
    }
    win.bounds.set((rect.right, rect.bottom));
    unsafe {
        let _ = controller.SetBounds(rect);
    }
}

fn window_size_of(hwnd: HWND) -> (i32, i32) {
    let rect = window_rect(hwnd);
    (rect.right - rect.left, rect.bottom - rect.top)
}

/// A pooled (hidden, unassigned) popup window gives its rendering memory
/// back: the controller stops compositing and the browser is asked to trim
/// its caches to the low memory target.
fn park(win: &Win) {
    if win.surface == Surface::Manager {
        return;
    }
    if let Some(controller) = win.controller.borrow().clone() {
        let _ = unsafe { controller.SetIsVisible(false) };
    }
    set_memory_target(win, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW);
}

/// A pooled window taken for a new popup renders again at full speed.
fn unpark(win: &Win) {
    if win.surface == Surface::Manager {
        return;
    }
    set_memory_target(win, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL);
    if let Some(controller) = win.controller.borrow().clone() {
        let _ = unsafe { controller.SetIsVisible(true) };
    }
}

fn set_memory_target(win: &Win, level: COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL) {
    let Some(webview) = win.webview.borrow().clone() else {
        return;
    };
    if let Ok(webview19) = webview.cast::<ICoreWebView2_19>() {
        let _ = unsafe { webview19.SetMemoryUsageTargetLevel(level) };
    }
}

fn attach_webview(win: &Rc<Win>) -> bool {
    let Some(environment) = environment() else {
        return false;
    };
    let hwnd = win.hwnd;
    let (tx, rx) = std::sync::mpsc::channel::<Option<ICoreWebView2Controller>>();
    let handler = CreateCoreWebView2ControllerCompletedHandler::create(Box::new(
        move |error_code, controller| {
            error_code?;
            let _ = tx.send(controller);
            Ok(())
        },
    ));
    if unsafe { environment.CreateCoreWebView2Controller(hwnd, &handler) }.is_err() {
        capture_log("webview: controller creation call failed");
        return false;
    }
    let Some(Some(controller)) = wait_with_pump(rx, std::time::Duration::from_secs(15)) else {
        capture_log("webview: controller timeout");
        return false;
    };
    unsafe {
        let bounds = controller_bounds(win, window_size_of(hwnd));
        win.bounds.set((bounds.right, bounds.bottom));
        let _ = controller.SetBounds(bounds);
        // IsVisible defaults to false: without this the browser never
        // composites a pixel into the window.
        let _ = controller.SetIsVisible(true);
        // Fully transparent browser background: with no redirection bitmap
        // the pixels outside the rounded rail and card show the desktop.
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
        if let Ok(controller3) = controller.cast::<ICoreWebView2Controller3>() {
            let _ = controller3.SetRasterizationScale(window_scale(hwnd));
        }
    }
    let Ok(webview) = (unsafe { controller.CoreWebView2() }) else {
        capture_log("webview: CoreWebView2 getter failed");
        return false;
    };
    let navigation_handler =
        webview2_com::NavigationCompletedEventHandler::create(Box::new(move |_, _| {
            on_page_loaded(hwnd);
            Ok(())
        }));
    let message_handler =
        webview2_com::WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let mut raw = windows::core::PWSTR::null();
            unsafe { args.TryGetWebMessageAsString(&mut raw) }?;
            let message = take_pwstr(raw);
            on_page_message(hwnd, &message);
            Ok(())
        }));
    unsafe {
        let mut token = 0i64;
        let _ = webview.add_NavigationCompleted(&navigation_handler, &mut token);
        let _ = webview.add_WebMessageReceived(&message_handler, &mut token);
    }
    *win.webview.borrow_mut() = Some(webview.clone());
    *win.controller.borrow_mut() = Some(controller);
    let page = inlined_page(win.surface);
    if unsafe { webview.NavigateToString(&windows::core::HSTRING::from(page)) }.is_err() {
        capture_log("webview: NavigateToString failed");
        return false;
    }
    true
}

fn on_page_loaded(hwnd: HWND) {
    let Some(win) = win_for(hwnd) else { return };
    win.ready.set(true);
    let host = HOST.with(Cell::get);
    if !FIRST_LOAD.with(|cell| cell.replace(true)) {
        unsafe {
            let _ = KillTimer(Some(host), LOAD_TIMEOUT_TIMER_ID);
            if !demo_mode() && !manager_mode() {
                let _ = SetTimer(Some(host), IDLE_QUIT_TIMER_ID, IDLE_QUIT_MS, None);
            }
        }
        if let Some(pipe) = SHARED_PIPE.lock().expect("pipe lock").clone() {
            if let Ok(line) = serde_json::to_string(&Envelope::new(0, UiCommand::PageReady)) {
                let _ = pipe.write_line(&line);
            }
        }
    }
    let queued = std::mem::take(&mut *win.queue.borrow_mut());
    for script in queued {
        execute_script(&win, &script);
    }
    if !demo_mode() && win.surface != Surface::Manager && win.popup_id.get() == 0 {
        // Preloaded for the pool: nothing to show yet.
        park(&win);
    }
    if demo_mode() {
        win.show_pending.set(true);
        show_now(&win);
        if let Some(path) = PENDING_CAPTURE.lock().expect("capture lock").clone() {
            capture_page(hwnd, &path);
        }
    } else if win.surface == Surface::Manager {
        // Shown when the page reports a painted frame (window_ready); this
        // fallback keeps a slow page from never appearing.
        unsafe {
            let _ = SetTimer(Some(hwnd), MANAGER_SHOW_TIMER_ID, 1500, None);
        }
    }
}

/// Run one script on the page, or queue it until the page has loaded.
fn run_on_page(win: &Win, script: String) {
    if win.ready.get() {
        execute_script(win, &script);
    } else {
        win.queue.borrow_mut().push(script);
    }
}

fn execute_script(win: &Win, script: &str) {
    let Some(webview) = win.webview.borrow().clone() else {
        return;
    };
    let handler = ExecuteScriptCompletedHandler::create(Box::new(|result, value| {
        if result.is_ok() && value.starts_with("\"err") {
            capture_log(&format!("script: {:.120}", value));
        }
        Ok(())
    }));
    unsafe {
        let _ = webview.ExecuteScript(&windows::core::HSTRING::from(script), &handler);
    }
}

fn apply_page_event(win: &Win, event: &UiEvent) {
    if let Ok(payload) = serde_json::to_string(event) {
        run_on_page(
            win,
            format!(
                "(function(){{ try {{ if (!window.LINGO) return 'no-lingo'; \
                 window.LINGO.apply({payload}); return 'applied'; }} \
                 catch (e) {{ return 'err:' + e.message; }} }})()"
            ),
        );
    }
}

/// Bring a window on screen without taking focus: a chooser raised by a
/// selection in another app (the manager's prompt editor, a document) must
/// leave that app's caret and typing untouched, like the classic popup.
fn show_now(win: &Win) {
    unsafe {
        let _ = KillTimer(Some(HOST.with(Cell::get)), IDLE_QUIT_TIMER_ID);
        let _ = KillTimer(Some(win.hwnd), HIDE_TIMER_ID);
    }
    if win.surface == Surface::Manager {
        if !win.shown.replace(true) {
            unsafe {
                let _ = ShowWindow(win.hwnd, SW_SHOW);
                let _ = SetForegroundWindow(win.hwnd);
            }
        }
        return;
    }
    let first = !win.shown.replace(true);
    unsafe {
        if first {
            let _ = ShowWindow(win.hwnd, SW_SHOWNOACTIVATE);
        }
        let _ = SetWindowPos(
            win.hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
}

/// Take a window for a popup id: its own window, else a pooled one, else a
/// new one.
fn assign(id: u64) -> Option<Rc<Win>> {
    if let Some(win) = win_by_id(id) {
        return Some(win);
    }
    let idle = WINS.with(|wins| {
        wins.borrow()
            .iter()
            .find(|win| win.surface != Surface::Manager && win.popup_id.get() == 0)
            .cloned()
    });
    let win = match idle {
        Some(win) => win,
        None => create_surface(Surface::Popup)?,
    };
    // A pooled window may still be on screen showing its cleared page (the
    // hide is delayed one paint); it must start hidden for its new popup.
    unsafe {
        let _ = KillTimer(Some(win.hwnd), HIDE_TIMER_ID);
        if IsWindowVisible(win.hwnd).as_bool() {
            let _ = ShowWindow(win.hwnd, SW_HIDE);
        }
    }
    win.shown.set(false);
    win.popup_id.set(id);
    win.placement.set(None);
    win.show_pending.set(false);
    win.card.set(false);
    win.cascade_from.set(None);
    win.user_size.set(None);
    win.resize.set(None);
    unpark(&win);
    // Hidden: the viewport shrinks back to this window plus headroom.
    fit_bounds(&win, window_size_of(win.hwnd), true);
    Some(win)
}

/// Return a closed popup's window to the pool: the page clears itself, and
/// the window hides once that cleared frame has painted.
fn release(win: &Win) {
    let id = win.popup_id.replace(0);
    win.show_pending.set(false);
    win.placement.set(None);
    win.card.set(false);
    win.cascade_from.set(None);
    win.user_size.set(None);
    win.resize.set(None);
    unsafe {
        let _ = KillTimer(Some(win.hwnd), SHOW_TIMER_ID);
        let _ = KillTimer(Some(win.hwnd), RESIZE_TIMER_ID);
    }
    apply_page_event(win, &UiEvent::Close { id });
    if win.shown.get() {
        unsafe {
            let _ = SetTimer(Some(win.hwnd), HIDE_TIMER_ID, HIDE_AFTER_MS, None);
        }
    }
    // Keep a small pool; extra idle windows (and their renderers) go.
    let idle: Vec<Rc<Win>> = WINS.with(|wins| {
        wins.borrow()
            .iter()
            .filter(|other| other.surface != Surface::Manager && other.popup_id.get() == 0)
            .cloned()
            .collect()
    });
    let mut surplus = idle.len().saturating_sub(MAX_IDLE_WINDOWS);
    for extra in idle.iter().filter(|other| other.hwnd != win.hwnd) {
        if surplus == 0 {
            break;
        }
        surplus -= 1;
        unsafe {
            let _ = DestroyWindow(extra.hwnd);
        }
    }
    // Nothing on screen any more: after the plan's 600 s idle period the
    // companion exits (and its browser processes with it); the resident
    // starts it again for the next fair-copy popup.
    let busy = WINS.with(|wins| wins.borrow().iter().any(|other| other.popup_id.get() != 0));
    if !busy && !demo_mode() && !manager_mode() {
        unsafe {
            let _ = SetTimer(
                Some(HOST.with(Cell::get)),
                IDLE_QUIT_TIMER_ID,
                IDLE_QUIT_MS,
                None,
            );
        }
    }
}

fn handle_event_line(line: &str) {
    // A broken host connection ends with a Shutdown line (bare from the
    // local poll loop, or enveloped from the host); every surface honours it.
    let bare = serde_json::from_str::<UiEvent>(line).ok();
    let enveloped = serde_json::from_str::<Envelope<UiEvent>>(line)
        .ok()
        .map(|envelope| envelope.payload);
    if matches!(bare, Some(UiEvent::Shutdown)) || matches!(enveloped, Some(UiEvent::Shutdown)) {
        capture_log("exit: shutdown event");
        unsafe { PostQuitMessage(0) };
        return;
    }
    if manager_mode() {
        // Manager lines are opaque data snapshots: {"apply": {...}}.
        if let Some(win) = manager_win() {
            run_on_page(
                &win,
                format!(
                    "window.LINGO_MANAGER && window.LINGO_MANAGER.applyManager({line}); undefined"
                ),
            );
        }
        return;
    }
    let Some(event) = enveloped else { return };
    match &event {
        UiEvent::ShowChooser {
            id, anchor, source, ..
        } => {
            let Some(win) = assign(*id) else { return };
            win.cascade_from.set(*source);
            if !(win.shown.get() && win.card.get()) {
                set_anchor(&win, *anchor, true);
            }
            win.show_pending.set(true);
            apply_page_event(&win, &event);
        }
        UiEvent::ShowLoading { id, anchor, .. } => {
            let Some(win) = assign(*id) else { return };
            // A retry keeps a card where it is (the user may have moved it);
            // a chooser turning into its result moves below the selection,
            // or beside the source card it was raised from.
            let fresh = !(win.shown.get() && win.card.get());
            if fresh {
                let beside = win
                    .cascade_from
                    .get()
                    .and_then(win_by_id)
                    .filter(|source| unsafe { IsWindowVisible(source.hwnd) }.as_bool())
                    .map(|source| Placement::Beside {
                        source: window_rect(source.hwnd),
                    });
                match beside {
                    Some(placement) => win.placement.set(Some(placement)),
                    None => set_anchor(&win, *anchor, false),
                }
            }
            win.card.set(true);
            win.show_pending.set(true);
            apply_page_event(&win, &event);
            if fresh {
                apply_remembered_size(&win);
            }
        }
        UiEvent::LocalError {
            id,
            anchor: Some(anchor),
            ..
        } => {
            let Some(win) = assign(*id) else { return };
            let fresh = !(win.shown.get() && win.card.get());
            if fresh {
                set_anchor(&win, *anchor, false);
            }
            win.card.set(true);
            win.show_pending.set(true);
            apply_page_event(&win, &event);
            if fresh {
                apply_remembered_size(&win);
            }
        }
        UiEvent::Close { id } => {
            if let Some(win) = win_by_id(*id) {
                capture_log("close: releasing window");
                release(&win);
            }
        }
        UiEvent::Delta { id, .. }
        | UiEvent::Completed { id, .. }
        | UiEvent::LocalError { id, .. }
        | UiEvent::SetPinned { id, .. } => {
            if let Some(win) = win_by_id(*id) {
                apply_page_event(&win, &event);
            }
        }
        UiEvent::Shutdown => {}
    }
}

fn set_anchor(win: &Win, anchor: Anchor, above: bool) {
    if anchor.x != 0 || anchor.y != 0 {
        win.placement.set(Some(Placement::Anchor {
            x: anchor.x,
            y: anchor.y,
            above,
        }));
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

/// Beside a source card: right of it when it fits, else left, top-aligned,
/// clamped into the work area (the classic cascade).
fn beside_position(
    source: RECT,
    width: i32,
    height: i32,
    gap: i32,
    info: &MONITORINFO,
) -> (i32, i32) {
    let work = info.rcWork;
    let mut x = source.right + gap;
    if x + width > work.right {
        x = source.left - gap - width;
    }
    let x = x.clamp(work.left, (work.right - width).max(work.left));
    let y = source
        .top
        .clamp(work.top, (work.bottom - height).max(work.top));
    (x, y)
}

#[derive(serde::Deserialize)]
struct ContentSize {
    w: f64,
    h: f64,
}

#[derive(serde::Deserialize)]
struct BeginResize {
    edges: u8,
}

/// A command posted by the page bridge (`window.chrome.webview.postMessage`).
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
enum PageCommand {
    Copy {
        text: String,
    },
    Retry,
    Close,
    Pin {
        pinned: bool,
    },
    SelectProfile {
        index: usize,
    },
    SelfTranslate {
        text: String,
        /// Selection rectangle in page CSS pixels.
        #[serde(default)]
        x: Option<f64>,
        #[serde(default)]
        y: Option<f64>,
    },
}

impl PageCommand {
    fn into_protocol(self, id: u64, anchor: Option<Anchor>) -> UiCommand {
        match self {
            Self::Copy { text } => UiCommand::Copy { id, text },
            Self::Retry => UiCommand::Retry { id },
            Self::Close => UiCommand::UserClose { id },
            Self::Pin { pinned } => UiCommand::TogglePin { id, pinned },
            Self::SelectProfile { index } => UiCommand::SelectProfile { id, index },
            Self::SelfTranslate { text, .. } => UiCommand::SelfTranslate { id, text, anchor },
        }
    }
}

fn send_to_host(command: UiCommand) {
    if let Some(pipe) = SHARED_PIPE.lock().expect("pipe lock").clone() {
        if let Ok(line) = serde_json::to_string(&Envelope::new(0, command)) {
            let _ = pipe.write_line(&line);
        }
    }
}

fn on_page_message(hwnd: HWND, message: &str) {
    let Some(win) = win_for(hwnd) else { return };
    if let Ok(size) = serde_json::from_str::<ContentSize>(message) {
        on_content_size(&win, size);
        return;
    }
    if message.contains("\"begin_drag\"") {
        // The drag owns the position from here on: a kept anchor would snap
        // the card back to the selection on the next size report.
        win.placement.set(None);
        let mut point = POINT::default();
        let _ = unsafe { GetCursorPos(&mut point) };
        let lparam = (((point.y as u32) & 0xffff) << 16 | ((point.x as u32) & 0xffff)) as i32;
        unsafe {
            let _ = ReleaseCapture();
            let _ = PostMessageW(
                Some(hwnd),
                WM_NCLBUTTONDOWN,
                WPARAM(HTCAPTION as usize),
                LPARAM(lparam as isize),
            );
        }
        return;
    }
    if message.contains("\"begin_resize\"") {
        if let Ok(resize) = serde_json::from_str::<BeginResize>(message) {
            begin_resize(&win, resize.edges);
        }
        return;
    }
    if win.surface == Surface::Manager {
        // Manager messages can carry an API key being saved: never log them.
        if message.contains("\"window_minimize\"") {
            unsafe {
                let _ = ShowWindow(hwnd, windows::Win32::UI::WindowsAndMessaging::SW_MINIMIZE);
            }
            return;
        }
        if message.contains("\"window_close\"") {
            // Ending the page ends the manager (it watches this process).
            unsafe { PostQuitMessage(0) };
            return;
        }
        if message.contains("\"window_ready\"") {
            unsafe {
                let _ = KillTimer(Some(hwnd), MANAGER_SHOW_TIMER_ID);
            }
            show_now(&win);
            return;
        }
        if let Some(pipe) = SHARED_PIPE.lock().expect("pipe lock").clone() {
            let _ = pipe.write_line(message);
        }
        return;
    }
    let Ok(command) = serde_json::from_str::<PageCommand>(message) else {
        return;
    };
    capture_log(&format!("fwd: {:.60}", message));
    let id = win.popup_id.get();
    if id == 0 {
        return;
    }
    let anchor = match &command {
        PageCommand::SelfTranslate {
            x: Some(x),
            y: Some(y),
            ..
        } => {
            let scale = window_scale(hwnd);
            let rect = window_rect(hwnd);
            Some(Anchor {
                x: rect.left + (x * scale).round() as i32,
                y: rect.top + (y * scale).round() as i32,
            })
        }
        _ => None,
    };
    let close = matches!(command, PageCommand::Close);
    send_to_host(command.into_protocol(id, anchor));
    if close {
        // Close at once; the resident's Close for this id then finds the
        // window already released.
        release(&win);
    }
}

fn on_content_size(win: &Win, size: ContentSize) {
    if win.surface == Surface::Manager {
        return;
    }
    // A size report scheduled before a dismissal (the page re-reports after
    // 120/400 ms and on font load) must not move or re-show the window.
    if !win.show_pending.get() && !demo_mode() {
        return;
    }
    let hwnd = win.hwnd;
    let scale = window_scale(hwnd);
    let rect = window_rect(hwnd);
    let info = monitor_info_at(POINT {
        x: rect.left,
        y: rect.top,
    });
    let max_h = (info.rcWork.bottom - info.rcWork.top - 20).max(200);
    let (width, height) = match win.user_size.get() {
        // The user sized this card; the page fits the card to it.
        Some(size) => size,
        None => (
            ((size.w * scale).round() as i32).clamp(48, 3000),
            ((size.h * scale).round() as i32).clamp(48, max_h),
        ),
    };
    let (x, y) = match win.placement.get() {
        Some(Placement::Anchor { x, y, above }) => {
            anchored_position((x, y), above, width, height, &info)
        }
        Some(Placement::Beside { source }) => {
            beside_position(source, width, height, (8.0 * scale).round() as i32, &info)
        }
        None => (rect.left, rect.top),
    };
    let same = width == rect.right - rect.left
        && height == rect.bottom - rect.top
        && x == rect.left
        && y == rect.top;
    if !same {
        fit_bounds(win, (width, height), false);
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
    if win.shown.get() {
        return;
    }
    // Hidden: appear once the reports go quiet, i.e. after the last layout
    // the page settles on, never at an intermediate size or spot.
    unsafe {
        let _ = SetTimer(Some(hwnd), SHOW_TIMER_ID, SHOW_SETTLE_MS, None);
    }
}

fn begin_resize(win: &Win, edges: u8) {
    if edges == 0 || win.surface == Surface::Manager {
        return;
    }
    let mut cursor = POINT::default();
    let _ = unsafe { GetCursorPos(&mut cursor) };
    win.placement.set(None);
    win.resize.set(Some(Resize {
        edges,
        cursor,
        rect: window_rect(win.hwnd),
    }));
    unsafe {
        let _ = ReleaseCapture();
        let _ = SetTimer(Some(win.hwnd), RESIZE_TIMER_ID, 15, None);
    }
}

fn resize_step(win: &Win) {
    let Some(resize) = win.resize.get() else {
        unsafe {
            let _ = KillTimer(Some(win.hwnd), RESIZE_TIMER_ID);
        }
        return;
    };
    let pressed = unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } as u16 & 0x8000 != 0;
    if !pressed {
        win.resize.set(None);
        unsafe {
            let _ = KillTimer(Some(win.hwnd), RESIZE_TIMER_ID);
        }
        return;
    }
    let cursor_id = match resize.edges {
        1 | 4 => IDC_SIZEWE,
        2 | 8 => IDC_SIZENS,
        3 | 12 => IDC_SIZENWSE,
        _ => IDC_SIZENESW,
    };
    if let Ok(cursor) = unsafe { LoadCursorW(None, cursor_id) } {
        unsafe {
            let _ = SetCursor(Some(cursor));
        }
    }
    let mut point = POINT::default();
    let _ = unsafe { GetCursorPos(&mut point) };
    let (dx, dy) = (point.x - resize.cursor.x, point.y - resize.cursor.y);
    let scale = window_scale(win.hwnd);
    let min_w = ((MIN_CARD_W + BODY_PAD_W) * scale).round() as i32;
    let min_h = ((MIN_CARD_H + BODY_PAD_H) * scale).round() as i32;
    let mut rect = resize.rect;
    if resize.edges & 1 != 0 {
        rect.left = (rect.left + dx).min(rect.right - min_w);
    }
    if resize.edges & 4 != 0 {
        rect.right = (rect.right + dx).max(rect.left + min_w);
    }
    if resize.edges & 2 != 0 {
        rect.top = (rect.top + dy).min(rect.bottom - min_h);
    }
    if resize.edges & 8 != 0 {
        rect.bottom = (rect.bottom + dy).max(rect.top + min_h);
    }
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    if win.user_size.get() == Some((width, height)) && window_rect(win.hwnd).left == rect.left {
        return;
    }
    win.user_size.set(Some((width, height)));
    fit_bounds(win, (width, height), false);
    unsafe {
        let _ = SetWindowPos(
            win.hwnd,
            None,
            rect.left,
            rect.top,
            width,
            height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    let css_w = (width as f64 / scale - BODY_PAD_W).floor();
    let css_h = (height as f64 / scale - BODY_PAD_H).floor();
    LAST_CARD_SIZE.with(|last| last.set(Some((css_w, css_h))));
    execute_script(
        win,
        &format!("window.LINGO && window.LINGO.userSize({css_w}, {css_h}); undefined"),
    );
}

unsafe extern "system" fn surface_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_TIMER => {
            let Some(win) = win_for(hwnd) else {
                return LRESULT(0);
            };
            match wparam.0 {
                SHOW_TIMER_ID => {
                    unsafe {
                        let _ = KillTimer(Some(hwnd), SHOW_TIMER_ID);
                    }
                    if win.show_pending.get() {
                        show_now(&win);
                    }
                }
                HIDE_TIMER_ID => {
                    unsafe {
                        let _ = KillTimer(Some(hwnd), HIDE_TIMER_ID);
                    }
                    if win.popup_id.get() == 0 {
                        win.shown.set(false);
                        unsafe {
                            let _ = ShowWindow(hwnd, SW_HIDE);
                        }
                        park(&win);
                    }
                }
                RESIZE_TIMER_ID => resize_step(&win),
                MANAGER_SHOW_TIMER_ID => {
                    unsafe {
                        let _ = KillTimer(Some(hwnd), MANAGER_SHOW_TIMER_ID);
                    }
                    show_now(&win);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_SIZE => {
            // The manager page fills its window; popups keep their fixed,
            // monitor-sized viewport (see controller_bounds).
            if let Some(win) = win_for(hwnd) {
                if win.surface == Surface::Manager {
                    if let Some(controller) = win.controller.borrow().clone() {
                        unsafe {
                            let _ = controller.SetBounds(controller_bounds(&win, (0, 0)));
                        }
                    }
                }
            }
            LRESULT(0)
        }
        WM_DPICHANGED => {
            // Moving to a monitor with another scale: rasterize for it.
            if let Some(win) = win_for(hwnd) {
                if let Some(controller) = win.controller.borrow().clone() {
                    if let Ok(controller3) = controller.cast::<ICoreWebView2Controller3>() {
                        let _ = unsafe { controller3.SetRasterizationScale(window_scale(hwnd)) };
                    }
                    if win.surface == Surface::Manager {
                        let _ = unsafe { controller.SetBounds(controller_bounds(&win, (0, 0))) };
                    }
                }
                if win.surface != Surface::Manager {
                    fit_bounds(&win, window_size_of(hwnd), true);
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            let removed = WINS.with(|wins| {
                let mut wins = wins.borrow_mut();
                wins.iter()
                    .position(|win| win.hwnd == hwnd)
                    .map(|index| wins.remove(index))
            });
            if let Some(win) = removed {
                if let Some(controller) = win.controller.borrow_mut().take() {
                    let _ = unsafe { controller.Close() };
                }
                win.webview.borrow_mut().take();
                if win.surface == Surface::Manager {
                    // The manager page is the manager window: closing it
                    // ends the process (the manager then exits too).
                    unsafe { PostQuitMessage(0) };
                }
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

unsafe extern "system" fn host_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_APP_EVENT => {
            let raw = lparam.0 as *mut String;
            if !raw.is_null() {
                let line = unsafe { Box::from_raw(raw) };
                handle_event_line(&line);
            }
            LRESULT(0)
        }
        WM_TIMER => {
            match wparam.0 {
                DEMO_QUIT_TIMER_ID => {
                    unsafe {
                        let _ = KillTimer(Some(hwnd), DEMO_QUIT_TIMER_ID);
                        let _ = SetTimer(Some(hwnd), DEMO_FORCE_QUIT_TIMER_ID, 4000, None);
                    }
                    if PENDING_CAPTURE.lock().expect("capture lock").is_none() {
                        unsafe { PostQuitMessage(0) };
                    }
                }
                DEMO_FORCE_QUIT_TIMER_ID => unsafe { PostQuitMessage(0) },
                LOAD_TIMEOUT_TIMER_ID => {
                    capture_log("timer: load timeout fired");
                    unsafe { PostQuitMessage(0) };
                }
                IDLE_QUIT_TIMER_ID => {
                    capture_log("timer: idle quit fired");
                    unsafe { PostQuitMessage(0) };
                }
                _ => {}
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

/// Browser switches for the companion's WebView2 environment. The pages are
/// local strings that never touch the network, so the browser's background
/// services are switched off. Rendering is in software by default: the GPU
/// process held ~150 MiB per visible card (measured, see
/// docs/WEBVIEW_MEMORY.md) for no visible difference; the cost is more
/// CPU while a card streams. `LINGOSNAP_WEBVIEW_GPU=1` restores GPU
/// rendering. (WebView2 lets `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`
/// replace all of these.)
fn browser_arguments() -> String {
    let mut args = vec![
        "--disable-features=msSmartScreenProtection,SpareRendererForSitePerProcess,\
         Translate,OptimizationHints,AutofillServerCommunication,MediaRouter",
        "--disable-background-networking",
        "--disable-component-update",
        "--disable-sync",
        "--no-pings",
    ];
    let gpu = std::env::var("LINGOSNAP_WEBVIEW_GPU").is_ok_and(|value| value == "1");
    if !gpu {
        args.push("--disable-gpu");
    }
    args.join(" ")
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
    let Some(webview) = win_for(hwnd).and_then(|win| win.webview.borrow().clone()) else {
        capture_log("capture: no webview");
        return;
    };
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
                // A line this long is a framing failure, not data: the
                // largest real line (a full history snapshot) stays far below.
                if pending.len() > 16 * 1024 * 1024 {
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
