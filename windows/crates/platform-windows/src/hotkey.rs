//! Global hot-key registration for the resident shell.

#[cfg(windows)]
mod windows_impl {
    use selection_core::config::{HotkeyAction, HotkeySettings};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Input::KeyboardAndMouse::HOT_KEY_MODIFIERS;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN,
    };

    pub const TOGGLE_HOVER_ID: i32 = 0x7101;
    pub const TOGGLE_POPUP_ID: i32 = 0x7102;
    pub const CYCLE_PROFILES_ID: i32 = 0x7103;
    pub const PAUSE_RESUME_ID: i32 = 0x7104;
    pub const OPEN_POPUP_ID: i32 = 0x7105;

    fn action_id(action: HotkeyAction) -> i32 {
        match action {
            HotkeyAction::CycleProfiles => CYCLE_PROFILES_ID,
            HotkeyAction::SelectionTranslate => TOGGLE_POPUP_ID,
            HotkeyAction::HoverTranslate => TOGGLE_HOVER_ID,
            HotkeyAction::PauseResume => PAUSE_RESUME_ID,
            HotkeyAction::OpenPopup => OPEN_POPUP_ID,
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct ParsedHotkey {
        modifiers: HOT_KEY_MODIFIERS,
        virtual_key: u32,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum ParseHotkeyError {
        Empty,
        MissingModifier,
        DuplicateModifier,
        UnknownModifier,
        UnknownKey,
    }

    impl std::fmt::Display for ParseHotkeyError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            let message = match self {
                Self::Empty => "hotkey is empty",
                Self::MissingModifier => "hotkey must contain a modifier",
                Self::DuplicateModifier => "hotkey modifier is repeated",
                Self::UnknownModifier => "hotkey modifier is unknown",
                Self::UnknownKey => "hotkey key is unknown",
            };
            formatter.write_str(message)
        }
    }

    fn parse_hotkey(spec: &str) -> Result<ParsedHotkey, ParseHotkeyError> {
        let parts: Vec<_> = spec.split('+').map(str::trim).collect();
        if parts.is_empty() || parts.iter().any(|part| part.is_empty()) {
            return Err(ParseHotkeyError::Empty);
        }
        if parts.len() == 1 {
            return Err(ParseHotkeyError::MissingModifier);
        }
        let mut modifier_bits = 0_u32;
        for modifier in &parts[..parts.len() - 1] {
            let bit = match modifier.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => MOD_CONTROL.0,
                "alt" => MOD_ALT.0,
                "shift" => MOD_SHIFT.0,
                "win" | "windows" => MOD_WIN.0,
                _ => return Err(ParseHotkeyError::UnknownModifier),
            };
            if modifier_bits & bit != 0 {
                return Err(ParseHotkeyError::DuplicateModifier);
            }
            modifier_bits |= bit;
        }
        let key = parts.last().copied().ok_or(ParseHotkeyError::Empty)?;
        let virtual_key = parse_virtual_key(key).ok_or(ParseHotkeyError::UnknownKey)?;
        Ok(ParsedHotkey {
            modifiers: HOT_KEY_MODIFIERS(modifier_bits),
            virtual_key,
        })
    }

    fn parse_virtual_key(key: &str) -> Option<u32> {
        let normalized = key.trim().to_ascii_uppercase();
        match normalized.as_str() {
            "SPACE" | "SPACEBAR" => return Some(0x20),
            "TAB" => return Some(0x09),
            "ENTER" | "RETURN" => return Some(0x0D),
            _ => {}
        }
        let bytes = normalized.as_bytes();
        if bytes.len() == 1 && bytes[0].is_ascii_uppercase() {
            return Some(u32::from(bytes[0]));
        }
        if bytes.len() == 1 && bytes[0].is_ascii_digit() {
            return Some(u32::from(bytes[0]));
        }
        let suffix = normalized.strip_prefix('F')?;
        let number = suffix.parse::<u32>().ok()?;
        (1..=24).contains(&number).then_some(0x70 + number - 1)
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum HotkeyTransition {
        Unchanged,
        Replace {
            previous: Option<ParsedHotkey>,
            next: ParsedHotkey,
        },
        Remove,
    }

    fn plan_transition(
        previous: Option<ParsedHotkey>,
        requested: Option<&str>,
    ) -> Result<HotkeyTransition, ParseHotkeyError> {
        match requested {
            None => Ok(if previous.is_some() {
                HotkeyTransition::Remove
            } else {
                HotkeyTransition::Unchanged
            }),
            Some(spec) => {
                let next = parse_hotkey(spec)?;
                if previous == Some(next) {
                    Ok(HotkeyTransition::Unchanged)
                } else {
                    Ok(HotkeyTransition::Replace { previous, next })
                }
            }
        }
    }

    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    struct Slot {
        current: Option<ParsedHotkey>,
    }

    #[derive(Debug, Default)]
    pub struct Registrations {
        registered: Vec<i32>,
        pub conflicts: Vec<String>,
        slots: [Slot; ACTION_COUNT],
        manual_registered: bool,
    }

    const ACTION_COUNT: usize = 5;

    const ACTION_ORDER: [HotkeyAction; ACTION_COUNT] = [
        HotkeyAction::SelectionTranslate,
        HotkeyAction::HoverTranslate,
        HotkeyAction::PauseResume,
        HotkeyAction::OpenPopup,
        HotkeyAction::CycleProfiles,
    ];

    impl Registrations {
        pub fn register(hwnd: HWND, hotkeys: &HotkeySettings) -> Self {
            let mut registrations = Self::default();
            for action in ACTION_ORDER {
                registrations.apply(hwnd, action, hotkeys);
            }
            registrations
        }

        pub fn manual_available(&self) -> bool {
            self.manual_registered
        }

        pub fn reregister(&mut self, hwnd: HWND, hotkeys: &HotkeySettings) {
            for action in ACTION_ORDER {
                self.apply(hwnd, action, hotkeys);
            }
        }

        /// Reconcile one action against its configured spec, restoring the
        /// previous registration when the new spec cannot be registered.
        fn apply(&mut self, hwnd: HWND, action: HotkeyAction, hotkeys: &HotkeySettings) {
            let id = action_id(action);
            let slot = self.slots[slot_index(action)];
            let requested = hotkeys.spec(action);
            let plan = match plan_transition(slot.current, requested) {
                Ok(plan) => plan,
                Err(error) => {
                    if let Some(spec) = requested {
                        self.set_conflict(action, spec);
                        eprintln!("could not parse configured hotkey: {error}");
                    }
                    return;
                }
            };
            match plan {
                HotkeyTransition::Unchanged => {
                    self.clear_conflict(action);
                    if action == HotkeyAction::SelectionTranslate {
                        self.manual_registered = slot.current.is_some();
                    }
                }
                HotkeyTransition::Replace { next, .. } => {
                    self.clear_conflict(action);
                    self.unregister_action(hwnd, action);
                    let label = requested.unwrap_or_default();
                    if self.try_register(hwnd, id, next, label) {
                        self.slots[slot_index(action)] = Slot {
                            current: Some(next),
                        };
                        if action == HotkeyAction::SelectionTranslate {
                            self.manual_registered = true;
                        }
                        return;
                    }
                    // Roll back to the previous registration when possible.
                    if let Some(previous) = slot.current {
                        if self.try_register(hwnd, id, previous, label) {
                            self.slots[slot_index(action)] = slot;
                            return;
                        }
                    }
                    self.slots[slot_index(action)] = Slot::default();
                    if action == HotkeyAction::SelectionTranslate {
                        self.manual_registered = false;
                    }
                    if let Some(spec) = requested {
                        self.set_conflict(action, spec);
                    }
                }
                HotkeyTransition::Remove => {
                    self.clear_conflict(action);
                    self.unregister_action(hwnd, action);
                    self.slots[slot_index(action)] = Slot::default();
                    if action == HotkeyAction::SelectionTranslate {
                        self.manual_registered = false;
                    }
                }
            }
        }

        fn unregister_action(&mut self, hwnd: HWND, action: HotkeyAction) {
            let id = action_id(action);
            if self.slots[slot_index(action)].current.is_some() {
                let _ = unsafe { UnregisterHotKey(Some(hwnd), id) };
                self.registered.retain(|registered| *registered != id);
            }
        }

        fn clear_conflict(&mut self, action: HotkeyAction) {
            let label = conflict_label(action);
            if let Some(index) = self
                .conflicts
                .iter()
                .rposition(|entry| entry.starts_with(label))
            {
                self.conflicts.remove(index);
            }
        }

        fn set_conflict(&mut self, action: HotkeyAction, spec: &str) {
            let label = conflict_label(action);
            let entry = format!("{label}: {spec}");
            if self.conflicts.contains(&entry) {
                return;
            }
            self.conflicts.push(entry);
        }

        fn try_register(
            &mut self,
            hwnd: HWND,
            id: i32,
            parsed: ParsedHotkey,
            _label: &str,
        ) -> bool {
            // RegisterHotKey returns FALSE for an existing registration. Keep
            // the resident alive and report the conflict locally instead of panicking.
            if unsafe {
                RegisterHotKey(Some(hwnd), id, parsed.modifiers, parsed.virtual_key).is_ok()
            } {
                self.registered.push(id);
                true
            } else {
                eprintln!("LingoSnap could not register a configured hotkey");
                false
            }
        }

        pub fn unregister(&mut self, hwnd: HWND) {
            for id in self.registered.drain(..) {
                let _ = unsafe { UnregisterHotKey(Some(hwnd), id) };
            }
            self.slots = [Slot::default(); ACTION_COUNT];
            self.conflicts.clear();
            self.manual_registered = false;
        }
    }

    fn slot_index(action: HotkeyAction) -> usize {
        ACTION_ORDER
            .iter()
            .position(|candidate| *candidate == action)
            .expect("action is registered in ACTION_ORDER")
    }

    fn conflict_label(action: HotkeyAction) -> &'static str {
        match action {
            HotkeyAction::SelectionTranslate => "selection translate",
            HotkeyAction::HoverTranslate => "hover translate",
            HotkeyAction::PauseResume => "pause/resume",
            HotkeyAction::OpenPopup => "open popup",
            HotkeyAction::CycleProfiles => "cycle profiles",
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{
            parse_hotkey, plan_transition, HotkeyTransition, ParseHotkeyError, ParsedHotkey,
        };
        use selection_core::config::HotkeySettings;

        #[test]
        fn parses_default_cycle_hotkey() {
            let parsed = parse_hotkey("Ctrl+Alt+P").expect("default hotkey must parse");
            assert_eq!(parsed.virtual_key, u32::from(b'P'));
            assert_eq!(parsed.modifiers.0, super::MOD_CONTROL.0 | super::MOD_ALT.0);
        }

        #[test]
        fn parses_space_and_shift() {
            let parsed = parse_hotkey("Ctrl+Shift+Space").expect("open-popup default must parse");
            assert_eq!(parsed.virtual_key, 0x20);
            assert_eq!(
                parsed.modifiers.0,
                super::MOD_CONTROL.0 | super::MOD_SHIFT.0
            );
        }

        #[test]
        fn parses_function_key_case_insensitively() {
            let parsed = parse_hotkey("shift+win+f12").expect("function hotkey must parse");
            assert_eq!(parsed.virtual_key, 0x7b);
            assert_eq!(parsed.modifiers.0, super::MOD_SHIFT.0 | super::MOD_WIN.0);
        }

        #[test]
        fn rejects_ambiguous_or_unmodified_hotkeys() {
            assert_eq!(
                parse_hotkey("Ctrl+Ctrl+P"),
                Err(ParseHotkeyError::DuplicateModifier)
            );
            assert_eq!(parse_hotkey("P"), Err(ParseHotkeyError::MissingModifier));
            assert_eq!(parse_hotkey("Ctrl+"), Err(ParseHotkeyError::Empty));
            assert_eq!(
                parse_hotkey("Ctrl+MediaPlay"),
                Err(ParseHotkeyError::UnknownKey)
            );
        }

        #[test]
        fn rejects_unknown_modifiers() {
            assert_eq!(
                parse_hotkey("Meta+P"),
                Err(ParseHotkeyError::UnknownModifier)
            );
        }

        #[test]
        fn transition_planning_preserves_equal_registration() {
            let current = parse_hotkey("Ctrl+Alt+P").ok();
            assert_eq!(
                plan_transition(current, Some("Ctrl+Alt+P")),
                Ok(HotkeyTransition::Unchanged)
            );
        }

        #[test]
        fn transition_planning_replaces_registration() {
            let current = parse_hotkey("Ctrl+Alt+P").ok();
            let next = parse_hotkey("Shift+F12").expect("new hotkey must parse");
            assert_eq!(
                plan_transition(current, Some("Shift+F12")),
                Ok(HotkeyTransition::Replace {
                    previous: current,
                    next,
                })
            );
        }

        #[test]
        fn transition_planning_removes_and_adds() {
            let current = parse_hotkey("Ctrl+Alt+P").ok();
            assert_eq!(plan_transition(current, None), Ok(HotkeyTransition::Remove));
            let next = parse_hotkey("Ctrl+Shift+Space").expect("hotkey must parse");
            assert_eq!(
                plan_transition(None, Some("Ctrl+Shift+Space")),
                Ok(HotkeyTransition::Replace {
                    previous: None,
                    next
                })
            );
        }

        #[test]
        fn hotkey_specs_filter_empty_strings_as_off() {
            let settings = HotkeySettings::default();
            assert!(settings
                .spec(super::HotkeyAction::SelectionTranslate)
                .is_some());
            let off = HotkeySettings {
                selection_translate: Some(String::new()),
                hover_translate: None,
                ..HotkeySettings::default()
            };
            assert_eq!(off.spec(super::HotkeyAction::SelectionTranslate), None);
            assert_eq!(off.spec(super::HotkeyAction::HoverTranslate), None);
            assert_eq!(
                off.spec(super::HotkeyAction::PauseResume),
                Some("Ctrl+Shift+P")
            );
            assert_eq!(
                off.spec(super::HotkeyAction::CycleProfiles),
                Some("Ctrl+Alt+P")
            );
        }

        #[test]
        fn parsed_hotkey_is_copied_between_slots() {
            let parsed = parse_hotkey("Ctrl+Shift+T").expect("hotkey must parse");
            let slot = super::Slot {
                current: Some(parsed),
            };
            assert_eq!(
                slot.current,
                Some(ParsedHotkey {
                    modifiers: parsed.modifiers,
                    virtual_key: parsed.virtual_key
                })
            );
        }
    }
}

#[cfg(windows)]
pub use windows_impl::*;

#[cfg(not(windows))]
pub mod windows_impl {
    #[derive(Debug, Default)]
    pub struct Registrations {
        pub conflicts: Vec<String>,
    }
}

#[cfg(not(windows))]
pub use windows_impl::*;
