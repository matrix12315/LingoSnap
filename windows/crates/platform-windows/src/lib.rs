//! Windows-native platform integration.

pub mod app;
pub mod autostart;
pub mod capture;
pub mod clipboard;
pub mod composition;
pub mod config_reload;
pub mod credentials;
pub mod foreground;
pub mod hotkey;
pub mod mouse;
pub mod native_selection;
pub mod ocr;
pub mod popup;
pub mod runtime_trace;
pub mod shell_open;
pub mod tray;
pub mod uia;
pub mod webview_ui;

/// Marker retained for callers that only need to identify the platform crate.
pub const CRATE_NAME: &str = "lingosnap-platform-windows";
