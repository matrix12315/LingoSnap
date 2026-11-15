//! Notification-area icon and its small resident command menu.

#[cfg(windows)]
mod windows_impl {
    use windows::core::w;
    use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
    use windows::Win32::UI::Shell::{
        Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
        NOTIFYICONDATAW, NOTIFY_ICON_DATA_FLAGS,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, LoadIconW, SetForegroundWindow,
        TrackPopupMenu, IDI_APPLICATION, MF_CHECKED, MF_POPUP, MF_SEPARATOR, MF_STRING,
        TPM_BOTTOMALIGN, TPM_LEFTALIGN, WM_APP, WM_COMMAND, WM_LBUTTONDBLCLK, WM_RBUTTONUP,
    };

    pub const TRAY_CALLBACK: u32 = WM_APP + 7;
    pub const OPEN_MANAGER_COMMAND: usize = 0x7201;
    pub const TOGGLE_HOVER_COMMAND: usize = 0x7202;
    pub const TOGGLE_REST_COMMAND: usize = 0x7203;
    pub const EXIT_COMMAND: usize = 0x7204;
    pub const SET_SKIN_CLASSIC_COMMAND: usize = 0x7205;
    pub const SET_SKIN_FAIRCOPY_COMMAND: usize = 0x7206;
    pub const TOGGLE_AUTOSTART_COMMAND: usize = 0x7207;
    pub const SET_LANG_EN_COMMAND: usize = 0x7208;
    pub const SET_LANG_ZH_COMMAND: usize = 0x7209;

    /// The LingoSnap icon embedded in the resident (resource ID 1), sized for
    /// the tray at the current DPI.
    fn app_icon() -> Option<windows::Win32::UI::WindowsAndMessaging::HICON> {
        use windows::core::PCWSTR;
        use windows::Win32::Foundation::HINSTANCE;
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows::Win32::UI::WindowsAndMessaging::{
            GetSystemMetrics, LoadImageW, HICON, IMAGE_ICON, LR_DEFAULTCOLOR, SM_CXSMICON,
            SM_CYSMICON,
        };
        unsafe {
            let module = GetModuleHandleW(None).ok()?;
            let handle = LoadImageW(
                Some(HINSTANCE(module.0)),
                PCWSTR(std::ptr::without_provenance::<u16>(1)),
                IMAGE_ICON,
                GetSystemMetrics(SM_CXSMICON),
                GetSystemMetrics(SM_CYSMICON),
                LR_DEFAULTCOLOR,
            )
            .ok()?;
            Some(HICON(handle.0))
        }
    }

    #[derive(Debug)]
    pub struct TrayIcon {
        hwnd: HWND,
        id: u32,
    }

    impl TrayIcon {
        pub fn add(hwnd: HWND) -> windows::core::Result<Self> {
            let icon = Self { hwnd, id: 1 };
            let mut data = icon.data(NIF_MESSAGE | NIF_TIP | NIF_ICON);
            data.hIcon = match app_icon() {
                Some(icon) => icon,
                None => unsafe { LoadIconW(None, IDI_APPLICATION)? },
            };
            if !unsafe { Shell_NotifyIconW(NIM_ADD, &data).as_bool() } {
                return Err(windows::core::Error::from_win32());
            }
            Ok(icon)
        }

        fn data(&self, flags: NOTIFY_ICON_DATA_FLAGS) -> NOTIFYICONDATAW {
            let mut data = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: self.hwnd,
                uID: self.id,
                uFlags: flags,
                uCallbackMessage: TRAY_CALLBACK,
                ..Default::default()
            };
            let tip: Vec<u16> = "LingoSnap\0".encode_utf16().collect();
            let len = tip.len().min(data.szTip.len());
            data.szTip[..len].copy_from_slice(&tip[..len]);
            data
        }

        pub fn update_status(
            &self,
            hover_enabled: bool,
            rest_enabled: bool,
            language: lingosnap_core::config::UiLanguage,
        ) {
            let mut data = self.data(NIF_TIP);
            let text = match (language, rest_enabled, hover_enabled) {
                (lingosnap_core::config::UiLanguage::SimplifiedChinese, true, _) => {
                    "LingoSnap (休息模式)\0"
                }
                (lingosnap_core::config::UiLanguage::SimplifiedChinese, false, true) => {
                    "LingoSnap (悬停开启)\0"
                }
                (lingosnap_core::config::UiLanguage::SimplifiedChinese, false, false) => {
                    "LingoSnap\0"
                }
                (lingosnap_core::config::UiLanguage::English, true, _) => "LingoSnap (Rest mode)\0",
                (lingosnap_core::config::UiLanguage::English, false, true) => {
                    "LingoSnap (Hover on)\0"
                }
                (lingosnap_core::config::UiLanguage::English, false, false) => "LingoSnap\0",
            };
            let tip: Vec<u16> = text.encode_utf16().collect();
            let len = tip.len().min(data.szTip.len());
            data.szTip[..len].copy_from_slice(&tip[..len]);
            let _ = unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) };
        }

        pub fn remove(&self) {
            let data = self.data(NOTIFY_ICON_DATA_FLAGS(0));
            let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &data) };
        }
    }

    impl Drop for TrayIcon {
        fn drop(&mut self) {
            self.remove();
        }
    }

    pub fn handle_callback(
        hwnd: HWND,
        lparam: LPARAM,
        hover_enabled: bool,
        rest_enabled: bool,
        skin: lingosnap_core::config::PopupSkin,
        language: lingosnap_core::config::UiLanguage,
    ) {
        match lparam.0 as u32 {
            WM_LBUTTONDBLCLK => unsafe {
                windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                    Some(hwnd),
                    WM_COMMAND,
                    WPARAM(OPEN_MANAGER_COMMAND),
                    LPARAM(0),
                )
                .ok();
            },
            WM_RBUTTONUP => show_menu(hwnd, hover_enabled, rest_enabled, skin, language),
            _ => {}
        }
    }

    fn show_menu(
        hwnd: HWND,
        hover_enabled: bool,
        rest_enabled: bool,
        skin: lingosnap_core::config::PopupSkin,
        language: lingosnap_core::config::UiLanguage,
    ) {
        let menu = match unsafe { CreatePopupMenu() } {
            Ok(menu) => menu,
            Err(error) => {
                eprintln!("could not create tray menu: {error}");
                return;
            }
        };
        let hover_flags = if hover_enabled {
            MF_STRING | MF_CHECKED
        } else {
            MF_STRING
        };
        use lingosnap_core::config::{PopupSkin, UiLanguage};
        let classic_flags = if skin == PopupSkin::Classic {
            MF_STRING | MF_CHECKED
        } else {
            MF_STRING
        };
        let faircopy_flags = if skin == PopupSkin::FairCopy {
            MF_STRING | MF_CHECKED
        } else {
            MF_STRING
        };
        let autostart_flags = if crate::autostart::is_enabled() {
            MF_STRING | MF_CHECKED
        } else {
            MF_STRING
        };

        let lang_menu = match unsafe { CreatePopupMenu() } {
            Ok(m) => m,
            Err(error) => {
                eprintln!("could not create language submenu: {error}");
                let _ = unsafe { DestroyMenu(menu) };
                return;
            }
        };

        let lang_en_flags = if language == UiLanguage::English {
            MF_STRING | MF_CHECKED
        } else {
            MF_STRING
        };
        let lang_zh_flags = if language == UiLanguage::SimplifiedChinese {
            MF_STRING | MF_CHECKED
        } else {
            MF_STRING
        };

        let submenu_ok = unsafe {
            AppendMenuW(lang_menu, lang_en_flags, SET_LANG_EN_COMMAND, w!("English")).and_then(
                |_| {
                    AppendMenuW(
                        lang_menu,
                        lang_zh_flags,
                        SET_LANG_ZH_COMMAND,
                        w!("简体中文"),
                    )
                },
            )
        };
        if submenu_ok.is_err() {
            unsafe {
                let _ = DestroyMenu(lang_menu);
                let _ = DestroyMenu(menu);
            }
            return;
        }

        let is_zh = language == UiLanguage::SimplifiedChinese;
        let open_manager_text = if is_zh {
            w!("打开管理器")
        } else {
            w!("Open Manager")
        };
        let hover_text = if is_zh {
            w!("悬停翻译")
        } else {
            w!("Toggle Hover")
        };
        let rest_text = if is_zh {
            if rest_enabled {
                w!("退出休息模式")
            } else {
                w!("开启休息模式")
            }
        } else if rest_enabled {
            w!("Disable Rest Mode")
        } else {
            w!("Enable Rest Mode")
        };
        let classic_text = if is_zh {
            w!("界面: Classic")
        } else {
            w!("UI: Classic")
        };
        let faircopy_text = if is_zh {
            w!("界面: Fair Copy")
        } else {
            w!("UI: Fair Copy")
        };
        let autostart_text = if is_zh {
            w!("开机自启动")
        } else {
            w!("Start with Windows")
        };
        let exit_text = if is_zh { w!("退出") } else { w!("Exit") };

        let mut attached = false;
        let result = unsafe {
            AppendMenuW(menu, MF_STRING, OPEN_MANAGER_COMMAND, open_manager_text)
                .and_then(|_| AppendMenuW(menu, hover_flags, TOGGLE_HOVER_COMMAND, hover_text))
                .and_then(|_| AppendMenuW(menu, MF_STRING, TOGGLE_REST_COMMAND, rest_text))
                .and_then(|_| AppendMenuW(menu, MF_SEPARATOR, 0, None))
                .and_then(|_| {
                    let res =
                        AppendMenuW(menu, MF_POPUP, lang_menu.0 as usize, w!("Language / 语言"));
                    if res.is_ok() {
                        attached = true;
                    }
                    res
                })
                .and_then(|_| {
                    AppendMenuW(menu, classic_flags, SET_SKIN_CLASSIC_COMMAND, classic_text)
                })
                .and_then(|_| {
                    AppendMenuW(
                        menu,
                        faircopy_flags,
                        SET_SKIN_FAIRCOPY_COMMAND,
                        faircopy_text,
                    )
                })
                .and_then(|_| AppendMenuW(menu, MF_SEPARATOR, 0, None))
                .and_then(|_| {
                    AppendMenuW(
                        menu,
                        autostart_flags,
                        TOGGLE_AUTOSTART_COMMAND,
                        autostart_text,
                    )
                })
                .and_then(|_| AppendMenuW(menu, MF_STRING, EXIT_COMMAND, exit_text))
        };
        if result.is_ok() {
            let mut point = POINT::default();
            if unsafe { GetCursorPos(&mut point).is_ok() } {
                unsafe {
                    let _ = SetForegroundWindow(hwnd);
                    let _ = TrackPopupMenu(
                        menu,
                        TPM_LEFTALIGN | TPM_BOTTOMALIGN,
                        point.x,
                        point.y,
                        Some(0),
                        hwnd,
                        None,
                    );
                }
            }
        }
        if !attached {
            let _ = unsafe { DestroyMenu(lang_menu) };
        }
        unsafe {
            let _ = DestroyMenu(menu);
        }
    }
}

#[cfg(windows)]
pub use windows_impl::*;

#[cfg(not(windows))]
pub mod windows_impl {
    #[derive(Debug)]
    pub struct TrayIcon;

    impl TrayIcon {
        pub fn update_status(
            &self,
            _hover_enabled: bool,
            _rest_enabled: bool,
            _language: lingosnap_core::config::UiLanguage,
        ) {
        }
    }
}

#[cfg(not(windows))]
pub use windows_impl::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_command_ids_are_distinct() {
        let commands = [
            OPEN_MANAGER_COMMAND,
            TOGGLE_HOVER_COMMAND,
            TOGGLE_REST_COMMAND,
            EXIT_COMMAND,
            SET_SKIN_CLASSIC_COMMAND,
            SET_SKIN_FAIRCOPY_COMMAND,
            TOGGLE_AUTOSTART_COMMAND,
            SET_LANG_EN_COMMAND,
            SET_LANG_ZH_COMMAND,
        ];
        let mut unique = commands.to_vec();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), commands.len());
    }
}
