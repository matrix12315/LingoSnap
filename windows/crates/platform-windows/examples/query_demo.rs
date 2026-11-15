//! Throwaway harness: opens the Manual query window and prints what was submitted.
#[cfg(windows)]
fn main() {
    use lingosnap_core::UiLanguage;
    use lingosnap_platform_windows::query_input;
    use windows::core::w;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::*;

    unsafe extern "system" fn proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        if m == query_input::QUERY_SUBMITTED {
            let s = query_input::take_submitted().expect("submission");
            println!("SUBMITTED [{}] at {:?}", s.text, s.anchor);
            unsafe { PostQuitMessage(0) };
            return LRESULT(0);
        }
        unsafe { DefWindowProcW(h, m, w, l) }
    }
    unsafe {
        let inst = GetModuleHandleW(None).unwrap();
        let class = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: inst.into(),
            lpszClassName: w!("QDemo"),
            ..Default::default()
        };
        RegisterClassW(&class);
        let owner = CreateWindowExW(
            Default::default(),
            w!("QDemo"),
            w!("owner"),
            WS_OVERLAPPED,
            0,
            0,
            10,
            10,
            None,
            None,
            Some(inst.into()),
            None,
        )
        .unwrap();
        query_input::show(owner, UiLanguage::English, (300, 300));
        println!("SHOWN");
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
#[cfg(not(windows))]
fn main() {}
