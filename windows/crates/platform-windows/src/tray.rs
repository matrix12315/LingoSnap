//! Notification-area icon and its small resident command menu.

#[cfg(windows)]
mod windows_impl {
    use super::super::popup::normalize_opacity;
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
    use windows::Win32::UI::Shell::{
        Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
        NOTIFYICONDATAW, NOTIFY_ICON_DATA_FLAGS,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, LoadIconW, SetForegroundWindow,
        TrackPopupMenu, IDI_APPLICATION, MF_CHECKED, MF_POPUP, MF_STRING, TPM_BOTTOMALIGN,
        TPM_LEFTALIGN, WM_APP, WM_COMMAND, WM_LBUTTONDBLCLK, WM_RBUTTONUP,
    };

    pub const TRAY_CALLBACK: u32 = WM_APP + 7;
    pub const OPEN_MANAGER_COMMAND: usize = 0x7201;
    pub const TOGGLE_HOVER_COMMAND: usize = 0x7202;
    pub const TOGGLE_REST_COMMAND: usize = 0x7203;
    pub const EXIT_COMMAND: usize = 0x7204;
    /// Opacity menu commands: `OPACITY_COMMAND_BASE + index` for each preset.
    pub const OPACITY_COMMAND_BASE: usize = 0x7300;
    /// Preset opacities offered on the tray submenu, highest first.
    pub const OPACITY_PRESETS: &[(usize, f32)] = &[
        (OPACITY_COMMAND_BASE, 1.0),
        (OPACITY_COMMAND_BASE + 1, 0.9),
        (OPACITY_COMMAND_BASE + 2, 0.8),
        (OPACITY_COMMAND_BASE + 3, 0.7),
        (OPACITY_COMMAND_BASE + 4, 0.6),
        (OPACITY_COMMAND_BASE + 5, 0.5),
        (OPACITY_COMMAND_BASE + 6, 0.4),
        (OPACITY_COMMAND_BASE + 7, 0.3),
        (OPACITY_COMMAND_BASE + 8, 0.2),
        (OPACITY_COMMAND_BASE + 9, 0.1),
    ];

    pub fn opacity_command_value(command: usize) -> Option<f32> {
        OPACITY_PRESETS
            .iter()
            .find(|(id, _)| *id == command)
            .map(|(_, value)| *value)
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
            data.hIcon = unsafe { LoadIconW(None, IDI_APPLICATION)? };
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
            let tip: Vec<u16> = "Selection Translate\0".encode_utf16().collect();
            let len = tip.len().min(data.szTip.len());
            data.szTip[..len].copy_from_slice(&tip[..len]);
            data
        }

        pub fn update_status(&self, hover_enabled: bool, rest_enabled: bool) {
            let mut data = self.data(NIF_TIP);
            let text = if rest_enabled {
                "Selection Translate (Rest mode)\0"
            } else if hover_enabled {
                "Selection Translate (Hover on)\0"
            } else {
                "Selection Translate\0"
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
        popup_opacity: f32,
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
            WM_RBUTTONUP => show_menu(hwnd, hover_enabled, rest_enabled, popup_opacity),
            _ => {}
        }
    }

    fn opacity_label(opacity: f32) -> [u16; 8] {
        // "100%\0" through "10%\0" fit in a fixed buffer for AppendMenuW.
        let percent = (normalize_opacity(opacity) * 100.0).round() as i32;
        let text = format!("{percent}%");
        let mut buffer = [0u16; 8];
        let units: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let len = units.len().min(buffer.len());
        buffer[..len].copy_from_slice(&units[..len]);
        buffer
    }

    fn show_menu(hwnd: HWND, hover_enabled: bool, rest_enabled: bool, popup_opacity: f32) {
        let menu = match unsafe { CreatePopupMenu() } {
            Ok(menu) => menu,
            Err(error) => {
                eprintln!("could not create tray menu: {error}");
                return;
            }
        };
        let opacity_menu = match unsafe { CreatePopupMenu() } {
            Ok(menu) => menu,
            Err(error) => {
                eprintln!("could not create tray opacity menu: {error}");
                unsafe {
                    let _ = DestroyMenu(menu);
                }
                return;
            }
        };
        let hover_flags = if hover_enabled {
            MF_STRING | MF_CHECKED
        } else {
            MF_STRING
        };
        let current_opacity = normalize_opacity(popup_opacity);
        let result = unsafe {
            AppendMenuW(menu, MF_STRING, OPEN_MANAGER_COMMAND, w!("Open Manager"))
                .and_then(|_| {
                    AppendMenuW(menu, hover_flags, TOGGLE_HOVER_COMMAND, w!("Toggle Hover"))
                })
                .and_then(|_| {
                    AppendMenuW(
                        menu,
                        MF_STRING,
                        TOGGLE_REST_COMMAND,
                        if rest_enabled {
                            w!("Disable Rest Mode")
                        } else {
                            w!("Enable Rest Mode")
                        },
                    )
                })
                .and_then(|_| {
                    for (command, value) in OPACITY_PRESETS {
                        let labels = opacity_label(*value);
                        let flags = if (current_opacity - *value).abs() < 0.05 {
                            MF_STRING | MF_CHECKED
                        } else {
                            MF_STRING
                        };
                        AppendMenuW(opacity_menu, flags, *command, PCWSTR(labels.as_ptr()))?;
                    }
                    AppendMenuW(
                        menu,
                        MF_POPUP | MF_STRING,
                        opacity_menu.0 as usize,
                        w!("Popup Opacity"),
                    )
                })
                .and_then(|_| AppendMenuW(menu, MF_STRING, EXIT_COMMAND, w!("Exit")))
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
        // Destroying the root menu also destroys attached submenus.
        unsafe {
            let _ = DestroyMenu(menu);
        }
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn opacity_commands_map_to_presets() {
            assert_eq!(
                super::opacity_command_value(super::OPACITY_COMMAND_BASE),
                Some(1.0)
            );
            assert_eq!(
                super::opacity_command_value(super::OPACITY_COMMAND_BASE + 5),
                Some(0.5)
            );
            assert_eq!(super::opacity_command_value(0x71ff), None);
            assert_eq!(super::opacity_label(1.0)[0], '1' as u16);
            assert_eq!(super::opacity_label(0.5)[0], '5' as u16);
        }
    }
}

#[cfg(windows)]
pub use windows_impl::*;

#[cfg(not(windows))]
pub mod windows_impl {
    #[derive(Debug)]
    pub struct TrayIcon;
}

#[cfg(not(windows))]
pub use windows_impl::*;
