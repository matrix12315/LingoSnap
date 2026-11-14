//! "Start with Windows": a per-user `Run` registry value that launches the
//! resident at sign-in. The registry is the source of truth (no config key),
//! so the tray and both managers always agree.

use std::path::{Path, PathBuf};

/// Name of the value under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
pub const RUN_VALUE_NAME: &str = "LingoSnap";
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RESIDENT_EXE: &str = "lingosnap-resident.exe";

/// The command line stored in the registry: the resident path, quoted so a
/// path with spaces is launched correctly.
pub fn run_command(resident: &Path) -> String {
    format!("\"{}\"", resident.display())
}

/// The resident executable that sits next to the running executable (the
/// manager and the resident ship in the same folder).
pub fn sibling_resident_path() -> Option<PathBuf> {
    let current = std::env::current_exe().ok()?;
    let candidate = current.with_file_name(RESIDENT_EXE);
    candidate.is_file().then_some(candidate)
}

#[cfg(windows)]
mod windows_impl {
    use super::{run_command, RUN_KEY, RUN_VALUE_NAME};
    use std::path::Path;
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW,
        RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE,
        REG_OPTION_NON_VOLATILE, REG_SZ,
    };

    /// True when the `Run` value exists.
    pub fn is_enabled() -> bool {
        let key = HSTRING::from(RUN_KEY);
        let name = HSTRING::from(RUN_VALUE_NAME);
        let mut handle = HKEY::default();
        unsafe {
            if RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(key.as_ptr()),
                None,
                KEY_QUERY_VALUE,
                &mut handle,
            ) != ERROR_SUCCESS
            {
                return false;
            }
            let found = RegQueryValueExW(handle, PCWSTR(name.as_ptr()), None, None, None, None)
                == ERROR_SUCCESS;
            let _ = RegCloseKey(handle);
            found
        }
    }

    /// Add (pointing at `resident`) or remove the `Run` value.
    pub fn set_enabled(enabled: bool, resident: &Path) -> Result<(), String> {
        let key = HSTRING::from(RUN_KEY);
        let name = HSTRING::from(RUN_VALUE_NAME);
        let mut handle = HKEY::default();
        unsafe {
            let opened = RegCreateKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(key.as_ptr()),
                None,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                None,
                &mut handle,
                None,
            );
            if opened != ERROR_SUCCESS {
                return Err(format!("could not open the Run key (error {})", opened.0));
            }
            let result = if enabled {
                let bytes: Vec<u8> = run_command(resident)
                    .encode_utf16()
                    .chain(std::iter::once(0u16))
                    .flat_map(u16::to_le_bytes)
                    .collect();
                let status =
                    RegSetValueExW(handle, PCWSTR(name.as_ptr()), None, REG_SZ, Some(&bytes));
                if status == ERROR_SUCCESS {
                    Ok(())
                } else {
                    Err(format!(
                        "could not write the Run value (error {})",
                        status.0
                    ))
                }
            } else {
                let status = RegDeleteValueW(handle, PCWSTR(name.as_ptr()));
                if status == ERROR_SUCCESS || status == ERROR_FILE_NOT_FOUND {
                    Ok(())
                } else {
                    Err(format!(
                        "could not remove the Run value (error {})",
                        status.0
                    ))
                }
            };
            let _ = RegCloseKey(handle);
            result
        }
    }
}

#[cfg(windows)]
pub use windows_impl::{is_enabled, set_enabled};

#[cfg(not(windows))]
pub fn is_enabled() -> bool {
    false
}

#[cfg(not(windows))]
pub fn set_enabled(_enabled: bool, _resident: &Path) -> Result<(), String> {
    Err("start with Windows is only available on Windows".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes and removes the real per-user Run value, then restores it.
    /// Run on demand: `cargo test --package lingosnap-platform-windows autostart -- --ignored`.
    #[test]
    #[ignore = "touches HKCU Run"]
    fn registry_round_trip_restores_the_previous_state() {
        let before = is_enabled();
        let exe = Path::new(r"C:\Program Files\LingoSnap\lingosnap-resident.exe");
        set_enabled(true, exe).expect("enable");
        assert!(is_enabled());
        set_enabled(false, exe).expect("disable");
        assert!(!is_enabled());
        // Removing an absent value is not an error.
        set_enabled(false, exe).expect("disable again");
        if before {
            set_enabled(true, exe).expect("restore");
        }
    }

    #[test]
    fn run_command_quotes_the_path() {
        let command = run_command(Path::new(
            r"C:\Program Files\LingoSnap\lingosnap-resident.exe",
        ));
        assert_eq!(
            command,
            "\"C:\\Program Files\\LingoSnap\\lingosnap-resident.exe\""
        );
    }
}
