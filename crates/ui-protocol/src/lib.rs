//! Protocol v1 between the resident shell and the WebView2 fair-copy UI
//! process. JSON lines over the named pipe; see FAIR_COPY_UI_PLAN.md.
//!
//! The resident owns all UI state; the UI process is stateless presentation.
//! Every line carries a monotonic `seq` next to the tagged payload.

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;

/// Named pipe the UI process connects to.
pub const PIPE_NAME: &str = r"\\.\pipe\SelectionTranslatePopupUi";

/// Named pipe for the manager's fair-copy view. The manager keeps its own
/// data bridge, separate from the resident's popup state.
pub const MANAGER_PIPE_NAME: &str = r"\\.\pipe\SelectionTranslateManagerUi";
/// Window class of the companion's top-level window; the resident checks
/// clicks against it so choosing a profile is not dismissed as an outside
/// click by the resident's own mouse hook.
pub const COMPANION_WINDOW_CLASS: &str = "SelectionTranslatePopupUi";
/// Window class of the manager's fair-copy page. It differs from the popup
/// companion's class so the resident's outside-click check never mistakes the
/// manager page for the popup, and a second manager launch can find it.
pub const MANAGER_WINDOW_CLASS: &str = "SelectionTranslateManagerUi";

/// First line the UI process sends after connecting.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Handshake {
    pub protocol: u32,
    pub ready: bool,
}

impl Handshake {
    pub fn ready() -> Self {
        Self {
            protocol: PROTOCOL_VERSION,
            ready: true,
        }
    }
}

/// Which mockup surface a window presents.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    Chooser,
    Popup,
    Streaming,
    Error,
    Manager,
}

/// A screen position for anchoring a popup surface (physical pixels).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Anchor {
    pub x: i32,
    pub y: i32,
}

/// Resident → UI events.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UiEvent {
    ShowChooser {
        id: u64,
        profiles: Vec<String>,
        highlight: Option<usize>,
        anchor: Anchor,
        /// True when the chooser was raised from inside a result card
        /// (in-card selection).
        #[serde(default)]
        keeps_card: bool,
        /// The popup whose text was selected (self-translation). Every
        /// popup is its own window; the result card that replaces this
        /// chooser is placed beside that source card, like the classic
        /// cascade, and the source stays on screen.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<u64>,
    },
    ShowLoading {
        id: u64,
        target: String,
        context: Option<String>,
        anchor: Anchor,
    },
    Delta {
        id: u64,
        text: String,
    },
    Completed {
        id: u64,
        text: String,
    },
    LocalError {
        id: u64,
        message: String,
        /// Set when the error is the first thing presented for a request
        /// (no loading card came before it), so the card is placed at the
        /// selection instead of wherever the previous card was.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        anchor: Option<Anchor>,
    },
    SetPinned {
        id: u64,
        pinned: bool,
    },
    Close {
        id: u64,
    },
    Shutdown,
}

/// UI → resident commands.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UiCommand {
    RegisterWindow {
        id: u64,
        surface: Surface,
    },
    SelectProfile {
        id: u64,
        index: usize,
    },
    Retry {
        id: u64,
    },
    TogglePin {
        id: u64,
        pinned: bool,
    },
    UserClose {
        id: u64,
    },
    Copy {
        id: u64,
        text: String,
    },
    SelfTranslate {
        id: u64,
        text: String,
        /// Screen position of the selection inside the source card, where
        /// the chooser is presented (physical pixels).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        anchor: Option<Anchor>,
    },
    /// The page finished loading; the host may replay any event that was
    /// sent while the page was still navigating.
    PageReady,
    Ping,
}

/// One framed line of the pipe protocol: a sequence number plus payload.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Envelope<T> {
    pub seq: u64,
    #[serde(flatten)]
    pub payload: T,
}

impl<T> Envelope<T> {
    pub fn new(seq: u64, payload: T) -> Self {
        Self { seq, payload }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handshake_round_trips() {
        let line = serde_json::to_string(&Handshake::ready()).expect("serialize handshake");
        assert_eq!(line, r#"{"protocol":1,"ready":true}"#);
        let parsed: Handshake = serde_json::from_str(&line).expect("parse handshake");
        assert_eq!(parsed, Handshake::ready());
    }

    #[test]
    fn events_and_commands_round_trip_with_seq() {
        let event = Envelope::new(
            7,
            UiEvent::ShowChooser {
                id: 3,
                profiles: vec!["Translate".into(), "Expert".into()],
                highlight: Some(0),
                anchor: Anchor { x: 100, y: 200 },
                keeps_card: false,
                source: None,
            },
        );
        let line = serde_json::to_string(&event).expect("serialize event");
        assert!(line.contains(r#""seq":7"#));
        assert!(line.contains(r#""type":"show_chooser""#));
        let parsed: Envelope<UiEvent> = serde_json::from_str(&line).expect("parse event");
        assert_eq!(parsed, event);

        let command = Envelope::new(
            9,
            UiCommand::SelfTranslate {
                id: 3,
                text: "hola".into(),
                anchor: Some(Anchor { x: 4, y: 5 }),
            },
        );
        let line = serde_json::to_string(&command).expect("serialize command");
        let parsed: Envelope<UiCommand> = serde_json::from_str(&line).expect("parse command");
        assert_eq!(parsed, command);
        assert!(line.contains(r#""type":"self_translate""#));
    }

    #[test]
    fn shutdown_and_ping_round_trip() {
        let line = serde_json::to_string(&UiEvent::Shutdown).expect("serialize shutdown");
        assert_eq!(line, r#"{"type":"shutdown"}"#);
        let line = serde_json::to_string(&UiCommand::Ping).expect("serialize ping");
        assert_eq!(line, r#"{"type":"ping"}"#);
        let line = serde_json::to_string(&UiCommand::PageReady).expect("serialize page_ready");
        assert_eq!(line, r#"{"type":"page_ready"}"#);
    }
}
