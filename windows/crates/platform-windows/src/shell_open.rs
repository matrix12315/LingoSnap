//! Open a web page in the user's default browser.

/// True for `https://` and `http://` URLs only. Anything else (`file:`,
/// `javascript:`, a program path) is never handed to the shell.
pub fn is_web_url(url: &str) -> bool {
    let url = url.trim().to_ascii_lowercase();
    (url.starts_with("https://") || url.starts_with("http://"))
        && !url.contains(char::is_whitespace)
}

/// Open `url` with the default browser.
#[cfg(windows)]
pub fn open_url(url: &str) -> Result<(), String> {
    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let url = url.trim();
    if !is_web_url(url) {
        return Err("only http and https pages can be opened".to_owned());
    }
    let target = HSTRING::from(url);
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(target.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    // ShellExecute reports success with a value greater than 32.
    if result.0 as isize > 32 {
        Ok(())
    } else {
        Err(format!(
            "the browser could not be started (code {})",
            result.0 as isize
        ))
    }
}

#[cfg(not(windows))]
pub fn open_url(_url: &str) -> Result<(), String> {
    Err("opening pages is only available on Windows".to_owned())
}

#[cfg(test)]
mod tests {
    use super::is_web_url;

    #[test]
    fn only_plain_web_urls_are_accepted() {
        assert!(is_web_url("https://platform.openai.com/api-keys"));
        assert!(is_web_url("  HTTP://example.test/a?b=c "));
        assert!(!is_web_url("file:///C:/Windows/System32/calc.exe"));
        assert!(!is_web_url("javascript:alert(1)"));
        assert!(!is_web_url("calc.exe"));
        assert!(!is_web_url("https://a.test/ b"));
    }
}
