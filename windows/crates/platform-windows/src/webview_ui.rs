//! Resident-side bridge to the fair-copy WebView2 companion process
//! (`lingosnap-ui.exe`): named-pipe server plus child lifecycle. Protocol v1;
//! see FAIR_COPY_UI_PLAN.md.

use lingosnap_core::config::PopupSkin;
use lingosnap_ui_protocol::{
    Envelope, Handshake, UiEvent, MANAGER_PIPE_NAME, PIPE_NAME, PROTOCOL_VERSION,
};
use std::io::{BufRead, BufReader, Write};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

/// Resident window message carrying one boxed `UiCommand` JSON line in
/// lparam (ownership transfers to the window proc).
pub const FAIR_COPY_COMMAND_MESSAGE: u32 = WM_APP + 36;

/// How long the resident waits for the companion's ready handshake before
/// declaring the fair-copy UI unavailable for this session.
const HANDSHAKE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

#[derive(Debug)]
pub enum StartError {
    SpawnFailed(std::io::Error),
    /// The WebView2 runtime is missing or the companion never completed the
    /// handshake. The resident must fall back to the classic popup.
    Unavailable,
}

pub struct FairCopyUi {
    child: std::process::Child,
    pipe: std::fs::File,
    next_seq: u64,
}

/// Debug log next to the user's config: the resident's spawn/handshake
/// failures are otherwise invisible (a scheduled task has no console).
fn debug_log(message: &str) {
    let Some(base) = std::env::var_os("LOCALAPPDATA") else {
        return;
    };
    let path = std::path::PathBuf::from(base)
        .join("LingoSnap")
        .join("resident-debug.log");
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let _ = writeln!(file, "[{millis}] {message}");
    }
}

impl FairCopyUi {
    /// Create the pipe server, spawn the companion, verify its handshake, and
    /// pump commands back into the resident's message loop.
    pub fn start(surface: &str, hwnd: HWND) -> Result<Self, StartError> {
        // Each surface listens on its own pipe: the manager surface's
        // companion connects to the manager pipe, so it can never steal the
        // popup pipe instance a live companion or another process created.
        let pipe_name = if surface == "manager" {
            MANAGER_PIPE_NAME
        } else {
            PIPE_NAME
        };
        // Test harnesses run a second host beside the user's (the companion
        // reads the same variable, and inherits it).
        let override_name = std::env::var("LINGOSNAP_PIPE_NAME").ok();
        let pipe_name = override_name.as_deref().unwrap_or(pipe_name);
        debug_log(&format!("start: surface={surface} spawning companion"));
        let pipe = create_pipe_server(pipe_name).map_err(|error| {
            debug_log(&format!("start: pipe server failed: {error}"));
            StartError::Unavailable
        })?;
        let mut child = spawn_companion(surface).map_err(|error| {
            debug_log(&format!("start: spawn failed: {error}"));
            StartError::SpawnFailed(error)
        })?;
        // A background reader performs the blocking ConnectNamedPipe and the
        // handshake read so a stuck companion cannot block the resident; it
        // then keeps forwarding command lines into the message loop.
        let (tx, rx) = std::sync::mpsc::channel();
        let mut reader = pipe.try_clone().map_err(|_| StartError::Unavailable)?;
        // HWND is not Send; the thread only needs the raw handle value.
        let target = hwnd.0 as isize;
        std::thread::spawn(move || {
            let hwnd = HWND(target as *mut _);
            let result = (|| -> Option<Handshake> {
                connect_pipe(&mut reader).ok()?;
                let mut line = String::new();
                BufReader::new(&mut reader).read_line(&mut line).ok()?;
                let handshake: Handshake = serde_json::from_str(line.trim()).ok()?;
                Some(handshake)
            })();
            let _ = tx.send(result);
            if result.is_none() {
                return;
            }
            // Poll-based line reader. A blocking `lines()` on a synchronous
            // pipe handle would park the file object — a synchronous file
            // object allows only one outstanding I/O — and then every
            // `send()` from the resident's main thread would block forever,
            // freezing the whole resident on the first translation. Reads go
            // through PeekNamedPipe and only consume bytes already buffered.
            let mut pending: Vec<u8> = Vec::new();
            'read: while let Ok(chunk) = read_available_bytes(&reader) {
                if chunk.is_empty() {
                    std::thread::sleep(std::time::Duration::from_millis(25));
                    continue;
                }
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
                        PostMessageW(
                            Some(hwnd),
                            FAIR_COPY_COMMAND_MESSAGE,
                            WPARAM(0),
                            LPARAM(boxed as isize),
                        )
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
        });
        // The caller's UI thread blocks here, so the wait is bounded twice:
        // by the handshake deadline and by the companion process itself — a
        // child that dies (missing WebView2, broken page) ends the wait
        // immediately instead of burning the whole timeout.
        let deadline = std::time::Instant::now() + HANDSHAKE_TIMEOUT;
        let mut handshake_ok = false;
        loop {
            match rx.recv_timeout(std::time::Duration::from_millis(250)) {
                Ok(Some(handshake))
                    if handshake.protocol == PROTOCOL_VERSION && handshake.ready =>
                {
                    handshake_ok = true;
                    break;
                }
                Ok(_) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if matches!(child.try_wait(), Ok(Some(_)) | Err(_)) {
                        break;
                    }
                    if std::time::Instant::now() >= deadline {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        if !handshake_ok {
            debug_log("start: handshake failed/timeout");
            let _ = child.kill();
            let _ = child.wait();
            return Err(StartError::Unavailable);
        }
        debug_log("start: handshake ok");
        Ok(Self {
            child,
            pipe,
            next_seq: 1,
        })
    }

    pub fn send(&mut self, event: UiEvent) -> std::io::Result<()> {
        let envelope = Envelope::new(self.next_seq, event);
        self.next_seq += 1;
        let mut line = serde_json::to_string(&envelope)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        line.push('\n');
        self.pipe.write_all(line.as_bytes())?;
        self.pipe.flush()
    }

    /// Send one opaque JSON line — the manager data bridge uses payloads the
    /// protocol crate does not model.
    pub fn send_raw(&mut self, line: &str) -> std::io::Result<()> {
        self.pipe.write_all(line.as_bytes())?;
        self.pipe.write_all(b"\n")?;
        self.pipe.flush()
    }

    /// True once the companion process has exited (the page was closed).
    pub fn child_exited(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)) | Err(_))
    }

    pub fn shutdown(&mut self) {
        let _ = self.send(UiEvent::Shutdown);
        let _ = self.child.wait();
    }
}

impl Drop for FairCopyUi {
    fn drop(&mut self) {
        // Never let the companion linger behind the resident. Cancelling I/O
        // on the server handle also unblocks the reader thread if it is still
        // parked in ConnectNamedPipe; otherwise the pipe instance would stay
        // registered forever and every later start would fail with
        // "all pipe instances are busy".
        use std::os::windows::io::AsRawHandle;
        unsafe {
            let _ = windows::Win32::System::IO::CancelIoEx(
                windows::Win32::Foundation::HANDLE(self.pipe.as_raw_handle() as _),
                None,
            );
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn create_pipe_server(pipe_name: &str) -> std::io::Result<std::fs::File> {
    use windows::Win32::Security::SECURITY_ATTRIBUTES;
    let wide: Vec<u16> = pipe_name.encode_utf16().chain(Some(0)).collect();
    // The server handle must NOT be inheritable: the companion is spawned
    // with handle inheritance on, and an inherited copy of the server handle
    // would keep the pipe "connected" even after this process dies, leaving
    // an orphaned companion (and a phantom pipe instance) behind.
    let security = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: std::ptr::null_mut(),
        bInheritHandle: false.into(),
    };
    let handle = unsafe {
        windows::Win32::System::Pipes::CreateNamedPipeW(
            windows::core::PCWSTR(wide.as_ptr()),
            windows::Win32::Storage::FileSystem::PIPE_ACCESS_DUPLEX,
            windows::Win32::System::Pipes::PIPE_TYPE_BYTE
                | windows::Win32::System::Pipes::PIPE_READMODE_BYTE
                | windows::Win32::System::Pipes::PIPE_WAIT,
            1,
            64 * 1024,
            64 * 1024,
            0,
            Some(&security),
        )
    };
    if handle.is_invalid() {
        return Err(std::io::Error::last_os_error());
    }
    use std::os::windows::io::FromRawHandle;
    Ok(unsafe { std::fs::File::from_raw_handle(handle.0 as _) })
}

/// Pull every byte currently buffered in the pipe without ever parking in a
/// blocking ReadFile. A synchronous file object allows only one outstanding
/// I/O, so a parked read on the reader thread would block every `send()` from
/// the resident's main thread. `Ok(vec![])` means no data yet; `Err` means
/// the peer hung up or the pipe broke.
fn read_available_bytes(pipe: &std::fs::File) -> std::io::Result<Vec<u8>> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Storage::FileSystem::ReadFile;
    use windows::Win32::System::Pipes::PeekNamedPipe;
    let handle = windows::Win32::Foundation::HANDLE(pipe.as_raw_handle() as _);
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

fn connect_pipe(pipe: &mut std::fs::File) -> std::io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    let result = unsafe {
        windows::Win32::System::Pipes::ConnectNamedPipe(
            windows::Win32::Foundation::HANDLE(pipe.as_raw_handle() as _),
            None,
        )
    };
    // ERROR_PIPE_CONNECTED means the client connected before the call.
    match result {
        Ok(()) => Ok(()),
        Err(error) if error.code() == windows::core::HRESULT(0x8007_0219_u32 as i32) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn spawn_companion(surface: &str) -> std::io::Result<std::process::Child> {
    let exe = std::env::current_exe()?
        .parent()
        .ok_or_else(|| std::io::Error::other("no executable directory"))?
        .join("lingosnap-ui.exe");
    // Dev diagnostics: keep the child's stderr in a file next to the exe —
    // a loader failure or early abort is otherwise completely invisible.
    let stderr_path = exe
        .parent()
        .map(|dir| dir.join("lingosnap-ui-stderr.log"))
        .ok_or_else(|| std::io::Error::other("no executable directory"))?;
    let stderr = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(stderr_path)?;
    std::process::Command::new(exe)
        .arg("--surface")
        .arg(surface)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::from(stderr))
        .spawn()
}

/// Start the fair-copy companion for the configured skin. Any failure is
/// reported as `None` so the resident keeps serving the classic popup.
pub fn start_for_skin(skin: PopupSkin, hwnd: HWND) -> Option<FairCopyUi> {
    match skin {
        PopupSkin::Classic => None,
        PopupSkin::FairCopy => match FairCopyUi::start("popup", hwnd) {
            Ok(ui) => Some(ui),
            Err(error) => {
                eprintln!("fair-copy UI unavailable, falling back to classic: {error:?}");
                None
            }
        },
    }
}

/// User-initiated copy from the fair-copy page. Unlike automatic handling
/// this is an explicit action, so overwriting the clipboard is expected.
pub fn copy_text_to_clipboard(text: &str) -> bool {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::GMEM_MOVEABLE;
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock};
    use windows::Win32::System::Ole::CF_UNICODETEXT;

    unsafe {
        if OpenClipboard(None).is_err() {
            return false;
        }
        let opened = true;
        let mut ok = false;
        if EmptyClipboard().is_ok() {
            let bytes: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
            let size = bytes.len() * std::mem::size_of::<u16>();
            if let Ok(handle) = GlobalAlloc(GMEM_MOVEABLE, size) {
                let locked = GlobalLock(handle);
                if !locked.is_null() {
                    std::ptr::copy_nonoverlapping(bytes.as_ptr(), locked as *mut u16, bytes.len());
                    let _ = GlobalUnlock(handle);
                    if SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(handle.0))).is_ok() {
                        ok = true;
                    }
                }
            }
        }
        let _ = CloseClipboard();
        let _ = opened;
        ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_skin_never_starts_a_companion() {
        assert!(start_for_skin(PopupSkin::Classic, HWND::default()).is_none());
    }
}
