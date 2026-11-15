# Fair-Copy Popup Input Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement in-card manual text input for the Fair Copy UI (`popup-input.html` mockup), allowing users to invoke a single unified card via `Ctrl+Shift+Space`, type/paste text with full IME support, choose a profile, and view streamed results in place (excluding the Classic UI).

**Architecture:** Extend `ui-protocol` with `UiEvent::ShowInput` and `UiCommand::SubmitInput`. In the companion (`popup-ui`), activate the window and instruct the page via `bridge.js` to render an editable `<textarea>` and embedded profile rail inside `.sourcepanel`. On profile selection or Enter, post `SubmitInput` back to the resident, which runs the standard LLM stream directly into that same card.

**Tech Stack:** Rust (Win32, WebView2 COM bindings, named pipes, Serde), HTML/CSS/JavaScript (Fair Copy paper theme).

---

## File Structure

- **Modified:**
  - `crates/ui-protocol/src/lib.rs`: Add `UiEvent::ShowInput` and `UiCommand::SubmitInput`.
  - `lingosnap-redesign/fair-copy/ui/fair-copy.css`: Add styles for `.srcinput`, embedded `.sourcepanel .railwrap`, and `.need` shake animation.
  - `windows/apps/popup-ui/src/bridge.rs`: Add `show_input` handler to `window.LINGO.apply`, render textarea and embedded pills, wire submission.
  - `windows/apps/popup-ui/src/main.rs`: Handle `UiEvent::ShowInput` with window activation (`SetForegroundWindow`, `SW_SHOW`) and route `SubmitInput` back to the pipe.
  - `windows/crates/platform-windows/src/app.rs`: Wire `hotkey::OPEN_POPUP_ID` when Fair Copy skin is active to send `UiEvent::ShowInput`, and handle `UiCommand::SubmitInput`.

---

### Task 1: Update UI Protocol with Input Event and Command

**Files:**
- Modify: `crates/ui-protocol/src/lib.rs`
- Test: `crates/ui-protocol/src/lib.rs` (unit tests)

- [ ] **Step 1: Write failing unit test in `crates/ui-protocol/src/lib.rs`**

```rust
#[test]
fn show_input_and_submit_input_round_trip() {
    let event = UiEvent::ShowInput {
        id: 42,
        profiles: vec!["Translate".into(), "Expert".into()],
        anchor: Anchor { x: 100, y: 200 },
    };
    let serialized_event = serde_json::to_string(&event).unwrap();
    let deserialized_event: UiEvent = serde_json::from_str(&serialized_event).unwrap();
    assert_eq!(event, deserialized_event);

    let command = UiCommand::SubmitInput {
        id: 42,
        text: "hello world".into(),
        index: 1,
    };
    let serialized_command = serde_json::to_string(&command).unwrap();
    let deserialized_command: UiCommand = serde_json::from_str(&serialized_command).unwrap();
    assert_eq!(command, deserialized_command);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lingosnap-ui-protocol`
Expected: Compile error (`ShowInput` and `SubmitInput` not found).

- [ ] **Step 3: Add `ShowInput` to `UiEvent` and `SubmitInput` to `UiCommand`**

In `crates/ui-protocol/src/lib.rs`:
```rust
pub enum UiEvent {
    // ...
    ShowInput {
        id: u64,
        profiles: Vec<String>,
        anchor: Anchor,
    },
    // ...
}

pub enum UiCommand {
    // ...
    SubmitInput {
        id: u64,
        text: String,
        index: usize,
    },
    // ...
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p lingosnap-ui-protocol`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ui-protocol/src/lib.rs
git commit -m "Add ShowInput event and SubmitInput command to ui-protocol"
```

---

### Task 2: Integrate `popup-input.html` CSS into `fair-copy.css`

**Files:**
- Modify: `lingosnap-redesign/fair-copy/ui/fair-copy.css`

- [ ] **Step 1: Append input and embedded rail styles to `fair-copy.css`**

Add styles extracted from `popup-input.html`:
```css
/* In-card manual query input and embedded profile rail */
.sourcepanel .railwrap.embedded {
  position: static;
  display: block;
  width: auto;
  margin: 10px 0 0;
  padding: 0;
}
.sourcepanel .railwrap.embedded .rail {
  background: none;
  box-shadow: none;
  flex-wrap: wrap;
}
.srcinput {
  display: block;
  width: 100%;
  margin: 0;
  padding: 0;
  border: 0;
  outline: 0;
  background: transparent;
  resize: none;
  overflow-y: auto;
  max-height: 132px;
  font: inherit;
  line-height: inherit;
  letter-spacing: inherit;
  color: inherit;
}
.srcinput::placeholder {
  color: inherit;
  opacity: .45;
}
.sourcepanel.need {
  outline: 1.5px solid transparent;
  outline-offset: 3px;
  animation: need .9s ease;
}
@keyframes need {
  30%, 70% { outline-color: var(--btn-primary-bg, #2b2721); }
}
```

- [ ] **Step 2: Commit**

```bash
git add lingosnap-redesign/fair-copy/ui/fair-copy.css
git commit -m "Add in-card input and embedded rail styles to fair-copy.css"
```

---

### Task 3: Support `show_input` in WebView2 Bridge and Page Handler

**Files:**
- Modify: `windows/apps/popup-ui/src/bridge.rs`
- Modify: `windows/apps/popup-ui/src/main.rs`

- [ ] **Step 1: Add `show_input` handling in `bridge.rs`**

Inside `window.LINGO.apply`:
- When `p.type === 'show_input'`:
  - Set mode to `'input'`.
  - Transform `.sourcepanel .target` to contain `<textarea class="srcinput" id="src" rows="2" placeholder="Type or paste text…" aria-label="Source text"></textarea>`.
  - Build the embedded `#chooser` inside `.sourcepanel`.
  - Wire pill clicks and Enter key on textarea to post `{ cmd: 'submit_input', id: p.id, text: src.value.trim(), index: index }`.
  - If text is empty on submission, trigger the `.need` nudge animation and re-focus textarea.
  - Automatically call `src.focus()`.
- When subsequent `show_loading` / `delta` arrives:
  - Transition smoothly: set `.target` text to submitted input, remove textarea and chooser rail, and proceed with standard streaming manuscript.

- [ ] **Step 2: Update `popup-ui/src/main.rs` to handle `UiEvent::ShowInput` and `submit_input`**

In `windows/apps/popup-ui/src/main.rs`:
- In `handle_pipe_line`:
  - When event is `UiEvent::ShowInput { id, anchor, .. }`:
    - Assign window: `let Some(win) = assign(*id) else { return };`
    - Set anchor at position: `set_anchor(&win, *anchor, false);`
    - Mark card and pending show.
    - Call `show_input_window(&win)`: unlike passive `SW_SHOWNOACTIVATE`, activate the window with `ShowWindow(win.hwnd, SW_SHOW)` and `SetForegroundWindow(win.hwnd)` so the user can immediately type without clicking.
    - Forward event to page via `apply_page_event(&win, &event)`.
- In web message handler (`handle_web_message`):
  - When `cmd == "submit_input"`:
    - Parse `text` and `index`.
    - Post `UiCommand::SubmitInput { id: win.popup_id.get(), text, index }` back to the resident over the named pipe.

- [ ] **Step 3: Run `cargo check -p lingosnap-ui` to verify compilation**

Run: `cargo check -p lingosnap-ui`
Expected: Clean compilation with 0 errors.

- [ ] **Step 4: Commit**

```bash
git add windows/apps/popup-ui/src/bridge.rs windows/apps/popup-ui/src/main.rs
git commit -m "Implement ShowInput handling and submit_input routing in popup-ui"
```

---

### Task 4: Connect `Ctrl+Shift+Space` and `SubmitInput` in Resident Core

**Files:**
- Modify: `windows/crates/platform-windows/src/app.rs`

- [ ] **Step 1: Wire `OPEN_POPUP_ID` in `app.rs` for Fair Copy**

In `windows/crates/platform-windows/src/app.rs`:
- On `hotkey::OPEN_POPUP_ID`:
  - If `companion_presents(state)` (Fair Copy is active):
    - Retrieve cursor anchor.
    - Allocate new popup admission via `ensure_popup(hwnd, state, anchor, None, None, true)`.
    - Ensure companion running via `ensure_companion(hwnd, state)`.
    - Query ordered profile names:
      ```rust
      let ordered = profile_chooser_order(&state.runtime.config.profiles);
      let profile_names: Vec<String> = ordered
          .iter()
          .map(|&idx| state.runtime.config.profiles[idx].name.clone())
          .collect();
      ```
    - Send `UiEvent::ShowInput`:
      ```rust
      send_fair_copy(state, UiEvent::ShowInput {
          id: popup_id as u64,
          profiles: profile_names,
          anchor: fair_copy_anchor(anchor),
      });
      ```
  - If Classic UI is active: do nothing (explicitly excluded as requested).

- [ ] **Step 2: Handle `UiCommand::SubmitInput` in `handle_fair_copy_command`**

In `handle_fair_copy_command` in `windows/crates/platform-windows/src/app.rs`:
```rust
UiCommand::SubmitInput { id, text, index } => {
    let ordered = profile_chooser_order(&state.runtime.config.profiles);
    if let Some(&profile_idx) = ordered.get(index) {
        let profile_id = state.runtime.config.profiles[profile_idx].id.clone();
        let spec = RequestSpec {
            trigger: TriggerKind::Selection,
            process_id: std::process::id(),
            source_root_window: 0,
            foreground_guard_root_window: 0,
            pointer: None,
            selection_rect: None,
            prompt_id: profile_id,
            bypass_duplicate_suppression: true,
            source_popup_id: None,
            destination_popup_id: (id != 0).then_some(id as popup::PopupId),
        };
        let pending = PendingAttempt {
            attempt: 0,
            spec,
            config_generation: state.config_generation,
        };
        let context = TextContext::new(text, lingosnap_core::ExtractionSource::Clipboard);
        admit_extracted_request(hwnd, state, pending, context, state.popup_anchor);
    }
}
```

- [ ] **Step 3: Run workspace tests to verify**

Run: `cargo test --workspace --locked`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add windows/crates/platform-windows/src/app.rs
git commit -m "Wire OPEN_POPUP_ID to ShowInput and admit SubmitInput in resident"
```

---

### Task 5: End-to-End Verification & Release Packaging

- [ ] **Step 1: Check formatting and linter**

Run: `cargo fmt --all -- --check`
Run: `cargo clippy --workspace --all-targets --locked -- -D warnings`

- [ ] **Step 2: Build release package `lingosnap-x64-r73`**

Run: `.\windows\scripts\package-release.ps1 -OutputDirectory windows/dist/lingosnap-x64-r73`

- [ ] **Step 3: Verify the packaged binaries and test manual query flow**

Verify:
1. Launch `lingosnap-resident.exe`.
2. Press `Ctrl+Shift+Space`: a Fair Copy popup card opens with an auto-focused textarea and profile pills.
3. Type text (supports English & Chinese IME).
4. Click a profile pill (e.g., "语言学家" or "IELTS"): the card smoothly transitions to loading and streams the result.
5. In Classic skin, verify `Ctrl+Shift+Space` does not trigger (as requested).
