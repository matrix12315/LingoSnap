# Fix guide: history search, hover dismissal, and UI polish

This guide is for the engineer who will implement these tasks. Each task names the files, the
functions, the change, and how to check it. Read `AGENTS.md` first; its rules apply to every
task. Do the tasks in the order below, and make one commit per task.

## Rules for every task

- **Build and test.** Use the wrapper, which sets up MSVC:
  `.\windows\tmp\vscargo.ps1 <cargo args>`. Write `--package <name>`, not `-p`. A
  "vswhere.exe is not recognized" message is harmless.
  - Format: `.\windows\tmp\vscargo.ps1 fmt --all`
  - Lint: `.\windows\tmp\vscargo.ps1 clippy --workspace --all-targets '--' '-D' warnings`
  - Test: `.\windows\tmp\vscargo.ps1 test --workspace`
  - Package names: `selection-translate-manager`, `selection-translate-popup-ui`,
    `selection-translate-resident`, `selection-core`, `selection-storage`,
    `selection-platform-windows`, `selection-ui-protocol`.
- **Release package** after the last task:
  `.\windows\scripts\package-release.ps1 -OutputDirectory windows/dist/selection-translate-x64-rNN | Out-File windows\tmp\package-rNN.log`.
  The script always stops at a missing `Get-FileHash` after the build. Write
  `BUILD-INFO.txt` by hand in the same format as `windows/dist/selection-translate-x64-r62/BUILD-INFO.txt`.
- **Mockups are the app.** Files in `lingosnap-redesign/fair-copy/ui/` are compiled
  into `popup-ui.exe` with `include_str!`. A CSS or HTML change there changes the app. Keep
  the mockup pages working when opened directly in a browser.
- **Both UIs.** Every user-visible change applies to the classic UI (Win32) and the fair-copy
  UI (WebView2), unless the task says otherwise. Classic manager strings live in
  `ui_text()` and `status_text()` in `windows/apps/manager/src/main.rs`, with an English
  and a Simplified Chinese entry for every key.
- **Do not rename the config schema or message names.** This covers TOML keys
  (`profiles`, `[defaults] selection/hover`, `ui.manager_language`, `ui.popup_skin`,
  `hotkeys.cycle_profiles`, `hotkeys.selection_translate`), `mcmd`/`cmd` message names,
  JSON field names, and the history column `prompt_id`. `AppConfig` and `UiSettings` use
  `deny_unknown_fields`, so a renamed key breaks every existing config file. Product-name
  identifiers (`SelectionTranslate`, `selection-translate`, `SELECTION_TRANSLATE_`, the
  `selection-*` crates) are renamed only in Task R1, which runs last. Earlier tasks keep
  using the current names, so the commands above stay valid until R1.
- **Dependencies.** Do not add a crate. Ask the user first if a task seems to need one.
- **Commits.** Stage files by name (never `.idea/`), commit with a message that says what
  changed and why, and push to `glm`.
- **Stop and ask** if a step here contradicts the code you find.

## Decisions (confirmed by the user)

1. **Product name.** Every place uses **LingoSnap**: visible text (window titles, tray
   tooltip, error captions, README, docs, the Chinese UI) and internal names (data
   folder, credential target, window classes, pipes, mutexes, env vars, exe and crate
   names). Visible text changes in Task O4a; internal names change in Task R1. Moving an
   existing install's data is not code: it is listed under "Separate work" at the end.
2. **Hover dismissal.** The bug is only that a Hover popup closes when the user clicks
   **inside** it. Closing on a click outside the popup stays as it is today.
3. **Result colour design** (Task O1). Option B, described in that task.
4. **Dark theme** (Task O2). It is for fair-copy only. Implement the dark design the HTML
   mockups already have (open any mockup with `#theme=dark`). The fair-copy manager's
   existing **Theme** field chooses Light or Dark. **Do not touch the classic UI.** It is
   always dark, and it gets no theme setting.
5. **Profiles are chosen by name only** (Task O4c). An ID is an internal unique code
   (`custom-8`), meaningless to a user. Every picker, chooser and list a user picks a
   profile from shows **only the name**, never the ID. A name is required (blank names
   are already rejected) and must be unique, so that every entry can be told apart. The
   ID is assigned by the app and is not user-editable.

---

## B1. History search must match targets only

**Cause.** One SQL filter serves both managers, and it matches the output too.
`crates/storage/src/history.rs`, `query()`, about lines 209–213:

```rust
sql.push_str(" AND (instr(target, ?1) > 0 OR instr(output, ?2) > 0)");
values.push(Box::new(search.to_owned()));
values.push(Box::new(search.to_owned()));
```

**Change.**
1. Replace that block with one clause and one bound value:
   `" AND instr(target, ?1) > 0"`. The later parameters (`prompt_id`, `source`, `LIMIT`)
   number themselves from `values.len() + 1`, so they stay correct.
2. While there, make the match case-insensitive for ASCII:
   `instr(lower(target), lower(?1)) > 0`. SQLite's `lower()` only folds ASCII. That is
   enough, because CJK has no case.
3. Update the doc comment on `HistoryQuery` (about lines 32–34): "Search is over the
   target text only."
4. Placeholders:
   - Fair-copy: `manager.html`, `#page-history .hsearch input`, `placeholder="Search target or output"`
     becomes `"Search targets"`.
   - Classic: `TextKey::SearchTargetOutput` in `ui_text()`. Rename the key to
     `SearchTarget`, and update `ALL_TEXT_KEYS` and `apply_search_placeholder()`.
     English: `"Search targets"`. Chinese: `"搜索目标文本"`.
5. The classic search runs only when Refresh is clicked. Leave that as it is.

**Tests.** `crates/storage/src/lib.rs`,
`search_filters_target_output_prompt_source_and_orders_dates` (about line 183) asserts
the old behaviour: it finds target `"beta"` by the word `"needle"` in its output. Rename
it to `search_filters_target_prompt_source_and_orders_dates`. Assert that `"needle"`
returns **zero** rows, that `"bet"` and `"BET"` return the `"beta"` row, and keep the
prompt, source and order assertions.

**Check.** Run `cargo test --package selection-storage`. Then open the fair-copy manager,
go to History, and type a word that appears only in an output: the list must be empty.
Type part of a target: that entry must appear.

---

## B2. A Hover popup closes when it is clicked inside

**Scope.** Only clicks **inside** a Hover popup are wrong: on its text, its buttons, its
header, its glass quote, or while dragging or resizing it. A click outside the popup
should still close it as it does today. Do not change `dismiss_unpinned_outside` or
`should_dismiss_for_outside_click`.

**Confirm which skin fails before changing code.** The resident records which rule closed
a popup with `runtime_trace::record`:
- `hover_popup_foreground_changed`: closed because the foreground window changed (the
  cause below);
- `popup_outside_click_dismiss`: recorded on every press outside all popups.

Find where `runtime_trace` writes its records, and reproduce the bug in both skins. The
analysis below explains the fair-copy case. If the classic skin also closes on an inside
click, the cause there is different: classic popups cannot be activated
(`WS_EX_NOACTIVATE`, and `WM_MOUSEACTIVATE` returns `MA_NOACTIVATE`). Trace it, then check
`contains_window_point` in `popup.rs` against the part of the popup that was clicked (for
example the selection card or a child control), and report what you find before fixing.

### The fair-copy cause

**Cause.** The resident closes an unpinned Hover popup when the foreground window changes
to a different top-level window: `handle_foreground_changed` →
`should_dismiss_hover_for_foreground` in `windows/crates/platform-windows/src/app.rs`
(about lines 1270–1321). It treats "the foreground is one of our popups" as safe, but it
only checks **classic** popup windows:

```rust
let foreground_is_popup = state.popups.iter().any(|entry| entry.popup.owns_window(foreground));
```

Fair-copy popups are windows of another process (`popup-ui.exe`), and they can be
activated: `create_surface` in `windows/apps/popup-ui/src/main.rs` (about lines 555–559)
creates them with `WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOREDIRECTIONBITMAP` and
without `WS_EX_NOACTIVATE`. Clicking one makes it the foreground window. The resident
then sees a different root window and closes the popup. Selection popups are not
affected, because that rule only applies to Hover.

**Change: do both steps.**
1. **Resident side** (the main fix). In `app.rs`, add a pure helper and use it:
   ```rust
   /// True when `hwnd`'s top-level window is a fair-copy companion window.
   fn is_companion_window(state: &ShellState, hwnd: HWND) -> bool
   ```
   It returns false when `state.fair_copy.is_none()`. Otherwise it takes
   `GetAncestor(hwnd, GA_ROOT)` and compares its class name (`GetClassNameW`) with
   `selection_ui_protocol::COMPANION_WINDOW_CLASS`. In `handle_foreground_changed`, set
   `foreground_is_popup` to true when either the classic check or this helper is true.
   That also stops the Hover candidate from being cancelled on that click (the
   `if !foreground_is_popup { … }` block above it). Also check the other caller of
   `hover_foreground_guard_is_current` (about line 2488) and pass the same combined value
   there.
2. **Companion side** (prevents the focus steal). In `popup-ui/src/main.rs`, in
   `surface_proc`, handle `WM_MOUSEACTIVATE` for popup surfaces (not
   `Surface::Manager`): return `LRESULT(MA_NOACTIVATE as isize)`. This matches the classic
   popup (`popup.rs`, `popup_wnd_proc`, `WM_MOUSEACTIVATE => MA_NOACTIVATE`).
   Do not add `WS_EX_NOACTIVATE` to the window style. Clicks still reach WebView2 with
   `MA_NOACTIVATE`, so after the change test that these all still work: Copy, Retry,
   Close, Pin, dragging by the header, edge resizing, selecting text in the result
   (self-translate), and Esc. If text selection or Esc stops working, keep only step 1
   and remove step 2.

**Tests.** Keep `hover_popup_foreground_guard_accepts_internal_or_same_root_only` and
`outside_click_dismisses_unpinned_only_when_no_popup_was_clicked` passing unchanged.
Split the class-name comparison into a pure function, for example
`fn is_companion_class(class: &str) -> bool`, and add a unit test for it.

**Check, in both skins.** Turn Hover on (tray → Toggle Hover). Hover a word in Notepad and
wait for the popup.
1. Click inside the popup: on its text, on Copy, on the header, and on the quote. Drag it
   and resize it. The popup stays every time.
2. Click outside the popup. It closes, as before.
3. Pin it, then click outside. It stays.
4. Repeat steps 1–2 with a Selection popup. Its behaviour is unchanged.

---

## O1. Expert result: unify the text colour

**Cause.** `renderMarkdown` in `windows/apps/popup-ui/src/bridge.rs` sets prose in the
first section as `.trans` (ink, near-black). Prose in later sections, and code, become
`.fineprint` (`--ink-soft`, grey). List items inherit ink. The Expert profile
(`linguist-analysis`, `crates/core/src/config.rs`) writes bullets under "Translation" and
prose under "Idioms and Grammar" and "Reasoning". So the same kind of text comes out black
or grey depending on where it falls. The History detail uses a second renderer,
`renderOutput` in `bridge_manager.rs`, which sets all prose grey, so the same result also
differs between the popup and History.

**Design (Decision 3: option B).**
- Option A, one colour: every paragraph, list item and table cell uses `--ink`. Simple,
  but loses all hierarchy.
- **Option B, one ink plus deliberate muted roles (default).** Content is always `--ink`.
  Grey is kept for two roles that carry meaning:
  1. **Field labels.** A bullet or paragraph that starts with a short label and a colon
     (`Chinese:`, `English:`, `Words:`, `Error check:`; at most 3 words before the colon)
     renders the label as `<span class="flabel">Chinese</span>` in `--ink-faint`, 12px UI
     font, followed by the value in ink.
  2. **Empty answers.** A paragraph, bullet value or whole section whose text is only
     `None` (case-insensitive, optionally followed by a full stop) renders as
     `<span class="none">None</span>` in `--ink-faint` italic. For example, an
     "Idioms and Grammar: None" section reads as visibly empty.

  Headings keep `--seal-deep`. Bullets keep `--seal`. Inline code keeps `.alt`.

**Change.**
1. `fair-copy.css`:
   - `.fineprint`: change `color` to `var(--ink)`. Search the mockup pages for
     `fineprint` and check that each sample still reads correctly.
   - `.manuscript blockquote, .houtput blockquote`: keep italic, change colour to
     `var(--ink)`.
   - Add `.flabel` and `.none` rules, and shared `.manuscript`/`.houtput` selectors for
     them.
2. `bridge.rs` `renderMarkdown` and `bridge_manager.rs` `renderOutput`: add the label and
   `None` detection in one small function. The two renderers are separate JS strings, so
   put the same function in both and keep them identical. Also make `renderOutput` treat
   the first section's prose as `.trans`, as `renderMarkdown` does, so History matches
   the popup.
3. Update one mockup (`popup.html`) to show a label bullet and a `None` section, so the
   design is visible in the mockup.
4. Classic popup (`windows/crates/platform-windows/src/popup.rs`, `render_markdown` /
   `set_rich_format`): it already uses one body colour. Only add the same two roles if it
   takes little code: a new `MarkdownStyle::Muted` coloured with an existing dimmer
   palette colour. Otherwise leave classic unchanged and say so in the commit message.

**Check.** Run an Expert translation of an English word in fair-copy, then open the same
entry in History. All content text is the same dark ink. The labels and `None` values are
muted. Both views look the same.

---

## O2. Fair-copy dark theme

**Scope.** Fair-copy only. Do not change anything in the classic popup or the classic
manager: they are always dark and get no theme setting. The target look is the dark
design the HTML mockups already have: open each page in
`lingosnap-redesign/fair-copy/ui/` with `#theme=dark` in a browser, and make the
app look the same. If a mockup itself shows a light leftover in dark mode (the list in
step 6), fix it in the mockup's CSS, which also fixes the app.

**What exists.** `fair-copy.css` already has a full `[data-theme="dark"]` palette (about
lines 46–75), and `bridge.rs` `APP_CSS` has dark shadows. `theme.js` sets
`document.documentElement.dataset.theme` and exposes `window.setTheme(t)`. The fair-copy
manager page already has a **Theme** field (`#uitheme`). Nothing in the app ever calls
`setTheme`, so the app is always light, and there is no theme setting in the config.

**Change.**
1. **Config** (`crates/core/src/config.rs`): add
   `pub enum UiTheme { #[default] #[serde(rename = "light")] Light, #[serde(rename = "dark")] Dark }`
   and `#[serde(default)] pub theme: UiTheme` in `UiSettings`. Export it from
   `crates/core/src/lib.rs` next to `UiLanguage`. Add a round-trip test, and a test that a
   config without `theme` loads as `Light`.
2. **Protocol** (`crates/ui-protocol/src/lib.rs`): add
   `UiEvent::SetAppearance { language: String, theme: String }`, with values `"en"` /
   `"zh-CN"` and `"light"` / `"dark"`. Task O3 uses `language` too, so add both fields
   now. Add a serde round-trip test like the existing ones.
3. **Resident** (`windows/crates/platform-windows/src/app.rs`): send `SetAppearance` from
   the current config right after the companion starts (where `ensure_companion` stores
   the new `FairCopyUi`), and again whenever the config is reloaded. Find the reload by
   searching for where `state.runtime.config` is replaced.
4. **Companion** (`windows/apps/popup-ui/src/main.rs`): store the last `SetAppearance` in a
   thread-local. Apply it to every popup window with a script
   (`window.setTheme && window.setTheme('dark')`, and `setLang` from O3), and also in
   `on_page_loaded`, so pooled and new windows match. Do not treat it as a popup event:
   `handle_event_line` must not assign a window for it.
5. **Manager page.** `fair_copy_push_settings` (manager `main.rs`, about line 5570): add
   `"theme": "light" | "dark"`. In `bridge_manager.rs` `applySettings`, call
   `window.setTheme(cfg.theme)` and turn `#uitheme` into a `choice(...)` dropdown
   (Light/Dark, Chinese 浅色/深色) that posts `{mcmd:'save_theme', theme}`. Remove the
   mockup's own handler for `#uitheme` in `manager.html` (about lines 396–401), or guard
   it so it does not run in the app. In manager `main.rs`, handle `save_theme`: clone the
   config, set `ui.theme`, `save_config`, `notify_config_changed()`, then
   `fair_copy_push_all`. Follow the pattern of `save_language`, which calls
   `save_manager_language`.
6. **Hard-coded light colours.** Make each of these follow the theme, with a variable or a
   `[data-theme="dark"]` override:
   - `fair-copy.css`: the scrollbar colours `rgba(34,29,22,…)`; `::selection`;
     `.iconbtn` shadows; the `rgba(34,29,22,.05)` and `.16` shadows in `.btn`, `.rpill`
     and `.mnav button` (use `var(--inner-shade)` and `var(--edge)`); `.rmenu`
     `background: rgba(247,243,235,.45)`; and the `.card` paper-fibre SVG (add a dark
     variant with light fibres at low alpha).
   - `bridge.rs` `APP_CSS`: the `.rail, .rmenu` fibre background.
   - `bridge.rs` `setMode`: `seal.style.background = error ? '#8A8375' : ''` should use a
     CSS class, for example `.seal.idle`, with light and dark values.
   - `manager.html` page CSS: the History button shadows (`rgba(34,29,22,…)`,
     `rgba(15,12,8,…)`).
   - `liquid-lens.js`: check that it uses no fixed colours.
7. **Look.** Compare against the dark review screenshots in
   `lingosnap-redesign/.impeccable/review/` (`dark-popup.png`,
   `dark-manager.png`, `v48-chooser-dark.png`). Open each mockup with `#theme=dark` in a
   browser to check it.

**Check.** Switch the theme in the fair-copy manager. The manager changes at once. The
next popup and the chooser are dark, with no light rim, light fibre, light scrollbar or
light shadow anywhere. Switch back to light: everything is unchanged from today.

---

## O3. Fair-copy Chinese interface

**What exists.** The classic manager is localised through `ui_text()` and
`status_text()`. The language is `config.ui.manager_language`
(`UiLanguage::English` = `"en"`, `SimplifiedChinese` = `"zh-CN"`). The fair-copy manager
receives `"language"` in its settings snapshot but never uses it. Fair-copy popups have
no language at all. Native status messages pushed into the fair-copy manager are already
localised.

**Change.**
1. **String table.** Create `lingosnap-redesign/fair-copy/ui/strings.js`. It
   defines `window.LINGO_STRINGS = { en: {...}, 'zh-CN': {...} }`, a function
   `window.t(key)`, and `window.setLang(lang)`. `setLang` sets
   `document.documentElement.lang`, then fills the text of every element with
   `[data-i18n]`, the `title` of `[data-i18n-title]`, the `aria-label` of
   `[data-i18n-aria]`, and the `placeholder` of `[data-i18n-ph]`. Then it dispatches a
   `langchange` event.
2. **Inline it.** In `popup-ui/src/main.rs`, add `STRINGS_JS` to `mod assets` and inline
   it in `inlined_page()` the same way `theme.js` is inlined. Add
   `<script src="strings.js"></script>` before `theme.js` in every page, so the mockups
   also work in a browser.
3. **Tag the pages.** Add `data-i18n*` attributes to every visible string in
   `popup.html`, `popup-streaming.html`, `popup-error.html`, `chooser.html` and
   `manager.html`. Take the Chinese wording from the classic `ui_text()` entries where the
   same label exists, so both UIs read the same. Use the vocabulary from Task O4a.
4. **JS strings.** Replace the literals with `t('…')`:
   - `bridge.rs`: `'writing…'`, `'idle'`, `'Retry'`, `'Close'`,
     `'Something went wrong.'`, `'More…'`, `'Profile'`, `' Copied'`/`' Copy'`.
   - `bridge_manager.rs`: `'beneath More…'`, `Drag to reorder`, `Move up`, `Move down`,
     the History filter labels, `' entries'`, `Minimize`, `Close`.
   - Rebuild the dropdowns on `langchange`.
5. **Required first: stop wiring buttons by their English text.** `bridge.rs` `wire()`
   finds Retry and Close by `label === 'Retry'`, `label === 'Close'` and
   `aria-label === 'Close'`. Translated labels would silently break them. Give those
   buttons `data-act="retry"` / `data-act="close"` in the pages, and match on `data-act`.
6. **Deliver the language.** Popups use the `SetAppearance` event from O2. The manager
   calls `setLang(cfg.language)` in `applySettings`. After `save_language`, the snapshot
   is pushed again, so the page relabels at once.
7. **Popup status text.** `STATUS_READY` is read from the page's status span at startup.
   Make it a key instead, so it changes with the language.

**Check.** Set the language to 简体中文. The fair-copy manager, chooser, result card,
streaming card and error card show no English UI text. The profile names and model output
stay as they are. Retry, Close, Copy and Pin still work in both languages.

---

## O4. Terminology, Prompts tab, and help text

### O4a. One vocabulary

Use exactly these terms in all user-visible text: UI labels, tooltips, placeholders,
status and error messages, the tray, README.md, README.zh-CN.md and `docs/*.md`.

| Concept | English | Chinese |
|---|---|---|
| A saved set of settings (name, ID, system prompt, user template, model, temperature, max tokens) | **profile** | **配置档** |
| The tab that edits them | **Profiles** | **配置档** |
| The system instruction text | **system prompt** | **系统提示词** |
| The per-request text with `{target}` | **user template** | **用户模板** |
| The app's config file | **settings** / `config.toml` | **设置** / 配置文件 |
| The product | **LingoSnap** (Decision 1) | **LingoSnap** |

Never use "prompt" alone for a profile, and never use bare 配置 for a profile.

Known places to change (search again; this list is not complete):
- **Classic manager** `ui_text()`:
  - Tab `Prompts` → `Profiles` / `提示词` → `配置档`.
  - `Save prompt` → `Save profile` / `保存配置档`.
  - `All prompts` → `All profiles` / `全部配置档`.
  - History column `Prompt` → `Profile` / `配置档`.
  - `划词配置` / `悬停配置` → `划词配置档` / `悬停配置档`.
  - `默认配置` → `默认配置档`.
  - `PromptsSubtitle` → "Create and tune reusable profiles" / "创建和调整可复用的配置档".
  - `WindowTitle` and `Brand` → LingoSnap.
- **Classic manager** `status_text()`: "no prompt profile", "Cannot save prompt",
  "New prompt is unsaved…", "Unsaved new prompt discarded…", "Prompt is invalid", and
  their Chinese entries.
- **Classic manager** hard-coded labels in control creation (about lines 1762–1860):
  `"ID"`, `"Name"`, `"Model override"`, `"Temperature"`, `"Max output tokens"`, `"New"`,
  `"Save prompt"`, the hint. Make them use `ui_text` keys, so they are localised too.
- **Seed data:** `new_prompt()` seeds the name `"New prompt"` → `"New profile"`.
- **Fair-copy** `manager.html`: nav `Prompts`, h2 `Prompts`, `＋ New prompt`, `Save prompt`,
  "Pick an existing prompt…", `All prompts`, titlebar "Selection Translate — Manager".
- **Fair-copy** `bridge_manager.rs`: `'All prompts'`.
- **Classic popup** `popup.rs`: the action button `"Prompt"`, which opens the chooser,
  becomes `"Profile"`; window titles "Selection Translate".
- **Elsewhere:** `app.rs` messages "Selection Translate could not start…" and
  "Prompt configuration is invalid" → "Profile configuration is invalid"; `tray.rs`
  tooltips; `popup-ui/src/main.rs` window titles; README.md / README.zh-CN.md
  ("prompt profile", 提示词 used for a profile); `docs/HOTKEYS.md`
  ("Cycle the prompt profile").
- **Keep as they are:** the config schema and message names (see "Rules for every
  task"), the `TextKey` and `StatusEvent` *names* (they are code), and the feature name
  "Selection translate" for the hot key and `HotkeyAction::SelectionTranslate` (it names
  the selection feature, not the product). The Chinese hot key label 划词翻译 also stays;
  only the Chinese *brand* 划词翻译 (`WindowTitle`, `Brand`) becomes LingoSnap. Internal
  product identifiers are renamed in Task R1, not here.
- **Tests:** the manager tests check that no English text leaks into the Chinese table,
  and that diagnostics hide private terms ("prompt" is in the forbidden list of
  `visible_diagnostics_are_fixed_and_privacy_safe` in `app.rs`). Run them, and do not
  weaken them.

### O4b. Delete a profile

Neither manager can delete a profile today.

1. **Pure logic.** Add this to `windows/apps/manager/src/main.rs`, next to `apply_prompt`:
   ```rust
   enum DeleteBlocked { LastProfile, SelectionDefault, HoverDefault }
   fn config_without_profile(config: &AppConfig, id: &str) -> Result<AppConfig, DeleteBlocked>
   ```
   Block the delete when it is the only profile, or when its ID is
   `config.defaults.selection` or `config.defaults.hover`. The user changes the default in
   Settings first. Otherwise remove the profile from `profiles`. The order is the order of
   that `Vec`, so nothing else needs fixing. Unit-test every branch.
2. **Shared action.** Add `fn delete_profile(owner: HWND, state: &mut ManagerState)`. It:
   1. ignores an unsaved draft (deleting a draft = `discard_draft`);
   2. asks for confirmation with `MessageBoxW` and
      `MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2`, following `delete_history`;
   3. calls `config_without_profile`, `validate()` and `save_config()`, then
      `notify_config_changed()`;
   4. clamps `state.profile_index`, then calls `refresh_prompt_form`,
      `populate_profile_selectors` and `populate_history_filters`.

   Add `StatusEvent` entries, in English and Chinese, for the confirmation, success, each
   blocked reason (it must say which setting to change), and failure. Old history rows
   keep their `prompt_id`; that is fine.
3. **Classic UI.** Add a `Delete profile` button (new `ID_DELETE_PROMPT`, slot next to
   Save) to the Profiles tab layout. It uses danger styling if the classic UI has one.
4. **Fair-copy UI.** In `manager.html`, add
   `<button class="btn danger sm" id="deletePromptBtn">Delete profile</button>` to
   `#page-prompts .actions`. In `bridge_manager.rs` `wire()`, post
   `{mcmd:'delete_prompt', id: promptId}`. Disable the button for a draft. In manager
   `main.rs`, handle `delete_prompt`: select the profile by ID, call `delete_profile`
   with the page window as owner (use the same `FindWindowW(SelectionTranslateManagerUi)`
   lookup as `delete_history`), then `fair_copy_push_all` and `fair_copy_push_status`.

### O4c. Find a profile by its name

**Goal.** A profile ID (`custom-8`, `contextual-zh-en`) is an internal unique code. Users
pick a profile by its **name**, so every place a user picks or reads a profile shows the
name only, never the ID.

1. **Names are required and unique.** Blank names are already rejected:
   `PromptConfig::validation_error` in `crates/core/src/prompt.rs` returns
   `PromptValidationError::EmptyName`. Add a uniqueness check to the managers' save path,
   not to `AppConfig::validate`, so an existing `config.toml` that happens to contain two
   equal names still loads. Compare names trimmed and case-insensitively. Add the check
   to `save_prompt` and `fair_copy_save_prompt` (manager `main.rs`), with a new
   `StatusEvent::DuplicateProfileName` in English ("Another profile is already named
   “{name}”. Choose a different name.") and Chinese ("已有名为“{name}”的配置档，请换一个名称。").
   The default profiles in `config.rs` already have unique names. Unit-test the check as a
   pure function.
2. **The ID is assigned by the app.** `new_prompt()` already creates `custom-N`. Make sure
   N gives an ID no existing profile uses (today it is `len + 1`, which can collide after a
   delete). The user can no longer type or change an ID:
   - **Classic.** `h.profile_id` is an editable combo (`CBS_DROPDOWN`) that is both the
     picker and the ID editor, and `prompt_from_form` reads the ID from its text. Change
     it to `CBS_DROPDOWNLIST`. `prompt_from_form` takes the ID from `draft_prompt` or from
     `state.config.profiles[state.profile_index]`, not from the control.
   - **Fair-copy.** `#pid` is already a hidden input. Keep posting the ID from it.
   - `apply_prompt` keeps its rename handling; with read-only IDs it simply never sees a
     changed ID.
3. **Show names only.** Add one helper per UI and use it everywhere listed below:
   classic `profile_display_name(profile) -> &str`, and fair-copy `profileName(p)` in
   `bridge_manager.rs` (replace the existing `profileLabel`, which gives "name — id").
   - **Profiles tab picker.** Fair-copy: `applyPrompts` fills `#pidMenu` and `#pidLabel`
     with `id`; use the name (for an unsaved draft, `selected.name`). Keep `data-v` and the
     posted `id`. Classic: `refresh_profile_dropdown` uses `profile_option_label()`; use the
     name. Change the label `ID＊`/`ID` to `Profile`/`配置档`.
   - **Settings defaults** (Selection profile, Hover profile): classic
     `populate_profile_selectors` and fair-copy `applySettings` use "name — id"; use the
     name.
   - **Settings order list**: fair-copy `buildOrder` and the classic order list show a
     muted ID next to the name; remove the ID.
   - **History "All profiles" filter**: fair-copy `buildHistoryFilters` lists `p.id`;
     classic `populate_history_filters` uses `"{id} — {name}"`. Use the name.
   - **History list and detail**: the profile chip shows `entry.prompt`, which is the ID.
     Show the profile's current name. If the ID no longer exists (the profile was deleted),
     show "Deleted profile" / "已删除的配置档".
   - **Popup choosers.** Check the fair-copy rail (`buildRail` in `bridge.rs`) and the
     classic chooser (`popup.rs`). They must show names; confirm that no ID appears.
   - **Status messages** that name a profile (for example the delete confirmation in
     O4b) use the name.
4. **Where the ID may still appear.** Only `config.toml`, and log or trace output.

**Check.** In both managers, open every picker and list named above. No ID is visible
anywhere. Create a new profile, give it the name of an existing one, and save: the save is
refused with the duplicate-name message. Rename it and save: it appears by name in the
Settings defaults, the order list, the History filter and the popup chooser.

### O4d. Help text for Model override, Temperature, Max tokens

**Cause.** In fair-copy the only help is a placeholder. A placeholder disappears once the
field has a value, and is cut off at the 92px column width. Also,
`#page-prompts .field .fhint { display: none; }` in `manager.html` hides every hint on the
page. The classic manager has no help text for these fields.

**Check this first.** Read `crates/provider-openai/src/client.rs`. Find what the request
sends when `model`, `temperature` or `max_output_tokens` is `None`: the provider model
from Settings, or the field left out of the request. Make the "empty" wording below match
what you find.

**Text** (adjust the "empty" part to what you found):

| Field | English | Chinese |
|---|---|---|
| Model override | Leave empty to use the model set in Settings. | 留空则使用设置中的模型。 |
| Temperature | 0–2. Lower gives steadier answers, higher gives more varied ones. Empty uses the provider default. | 0–2。越低越稳定，越高越多样。留空使用服务商默认值。 |
| Max tokens | The longest reply allowed, in tokens. Empty uses the provider default. | 回复的最大长度（令牌数）。留空使用服务商默认值。 |

**Change.**
- Fair-copy: show these as a `.fhint` line under each field. Change the hide rule to hide
  only the picker's old hint, for example `#page-prompts #pidbox ~ .fhint`. Keep short
  placeholders (`e.g. gpt-4o-mini`, `0.2`, `512`).
- Classic: add a hint label under each of the three fields, from new `TextKey` entries in
  English and Chinese. Rename the label "Max output tokens" to "Max tokens" in both UIs.

**Check.** In both managers, every hint is fully visible at the default window size, with
values filled in, in English and Chinese.

---

## O5. App icon

**What exists.** There is no icon resource and no `build.rs`. The tray uses the generic
`LoadIconW(None, IDI_APPLICATION)` (`windows/crates/platform-windows/src/tray.rs`, about
line 35). No window class sets `hIcon`. The design source is the vermilion "译" seal in the
fair-copy pages (`.seal` in `fair-copy.css`: `--seal #B23A25`, paper-coloured glyph,
radius 4px at 27px).

**Change. No new crates (see the dependency rule).**
1. **Artwork.** Add `windows/scripts/make-icon.ps1`. It draws the seal with
   `System.Drawing`: a `#B23A25` rounded square, with the glyph 译 in white in
   "Microsoft YaHei" bold, about 62% of the size. It renders 16, 20, 24, 32, 40, 48, 64
   and 256 px PNGs and packs them into one `.ico` by writing the ICO header and directory
   by hand. That is about 40 lines; the 256 px entry is stored as PNG. Save the result as
   `windows/assets/lingosnap.ico` and commit the `.ico`, so builds do not need the script.
   Below 24 px the glyph is unreadable; use a solid seal with a white bar for 16 and 20 px.
   Ask the user to approve the artwork before wiring it in.
2. **Structure.** `windows/assets/` is a new directory. Add one line for it to the
   Structure list in `AGENTS.md` ("Windows icons and other resources") before adding
   files, as AGENTS.md requires.
3. **Resource.** Add `windows/assets/app.rc` containing `1 ICON "lingosnap.ico"`. In each
   of `windows/apps/resident`, `windows/apps/manager` and `windows/apps/popup-ui`, add a
   `build.rs` that:
   1. runs `rc.exe /nologo /fo $OUT_DIR\app.res <path to app.rc>`;
   2. prints `cargo:rustc-link-arg-bins=$OUT_DIR\app.res`;
   3. prints `cargo:rerun-if-changed` for the `.rc` and `.ico`.

   `rc.exe` is on `PATH` inside the `vscargo.ps1` (VsDevCmd) shell, which is the
   documented way to build. If `rc.exe` is missing, fail with a clear message. The resident
   shows the icon in Explorer and Task Manager; the other two need it for their windows.
4. **Runtime.**
   - Tray (`tray.rs`): load icon 1 from the resident's own module with
     `LoadImageW(GetModuleHandleW(None), MAKEINTRESOURCE(1), IMAGE_ICON, GetSystemMetrics(SM_CXSMICON), GetSystemMetrics(SM_CYSMICON), LR_DEFAULTCOLOR)`.
     Fall back to `IDI_APPLICATION` if that fails.
   - Classic manager (`windows/apps/manager/src/main.rs`, window created about line 1140):
     send `WM_SETICON` with `ICON_BIG` (32 px) and `ICON_SMALL` (16 px) after
     `CreateWindowExW`.
   - Fair-copy manager window (`popup-ui/src/main.rs` `create_surface`, `Surface::Manager`
     branch): the same two `WM_SETICON` calls. Its taskbar button uses them.
   - Popups are tool windows and need no icon.

**Check.** Explorer shows the seal on all three exes. The tray icon is sharp at 100 % and
200 % scale. Both manager windows show the icon in the taskbar and in Alt+Tab.

---

## R1. Rename the product to LingoSnap everywhere in code

Do this task last, after every other task is committed, so it does not conflict with
them. It is mechanical but wide. Use `git grep -n -I` for each pattern below, change every
hit that names the product, and keep feature names (see "Keep" below). Build and test after
each step, and make one commit per step.

**Step 1: runtime identifiers** (no file renames). Change the strings, not the Rust
identifiers around them:

| Today | New |
|---|---|
| `%LOCALAPPDATA%\SelectionTranslate\` (`config.rs` `default_config_path`, `storage/src/db.rs`, `storage/src/lib.rs`, `popup-ui/src/main.rs` `user_data_dir` and log path) | `%LOCALAPPDATA%\LingoSnap\` |
| `SelectionTranslateWebview` temp fallback | `LingoSnapWebview` |
| `DEFAULT_CREDENTIAL_TARGET = "SelectionTranslate/OpenAI"` (`config.rs`), and the samples in `manager.html`, `config.example.toml`, the classic mockup | `LingoSnap/OpenAI` |
| Pipe names and window classes in `crates/ui-protocol/src/lib.rs` (`PIPE_NAME`, `MANAGER_PIPE_NAME`, `COMPANION_WINDOW_CLASS`, `MANAGER_WINDOW_CLASS`) | `LingoSnapPopupUi`, `LingoSnapManagerUi` |
| Class names in `popup-ui/src/main.rs` (`CLASS_NAME`, `MANAGER_CLASS_NAME`, `HOST_CLASS_NAME`), `manager/src/main.rs` (`CLASS_NAME`, `PAGE_CLASS`, the `w!("SelectionTranslate…")` lookups, the `SelectionTranslateManagerInstance` default), `app.rs` (`SelectionTranslateResident`), `popup.rs` (`SelectionTranslatePopup`, `SelectionTranslateMenuPanel`, also in its tests) | the same names with `LingoSnap` |
| Mutexes `Local\SelectionTranslate.Resident`, `Local\SelectionTranslate.ResidentManagerLaunch` (`app.rs`) | `Local\LingoSnap.…` |
| User agent `SelectionTranslate/0.1` (`provider-openai/src/client.rs`) | `LingoSnap/0.1` |
| Env vars `SELECTION_TRANSLATE_OPENAI_API_KEY`, `…_BASE_URL`, `…_MODEL` (`resident/src/main.rs`, `provider-openai/src/client.rs`) | `LINGOSNAP_OPENAI_API_KEY`, `…_BASE_URL`, `…_MODEL`. `OPENAI_API_KEY` stays: it is the standard name |

Make every window class and pipe name come from one constant in `ui-protocol` where two
processes must agree on it, instead of repeating string literals. Update the docs that
quote these values: README.md, README.zh-CN.md, `docs/SETUP.md`,
`docs/TROUBLESHOOTING.md`, `HANDOFF.md`, `IMPLEMENTATION_PLAN.md`,
`FAIR_COPY_UI_PLAN.md`, `windows/config/config.example.toml`.

**Step 2: exe names.** Rename the binaries: `selection-translate-resident` →
`lingosnap-resident`, `selection-translate-manager` → `lingosnap-manager`, and `popup-ui`
→ `lingosnap-ui`. Set the `[[bin]] name` or package name in each
`windows/apps/*/Cargo.toml`. Update every place that launches or names them:
`webview_ui.rs` (`join("popup-ui.exe")`), the resident's manager launch, the manager's
resident start, `windows/scripts/*.ps1` (`package-release.ps1` copies the exes by name and
writes them into `BUILD-INFO.txt`; its default output folder becomes
`windows/dist/lingosnap-x64`), `measure-memory.ps1` usage in docs, and `AGENTS.md` /
`docs/VERIFICATION.md` commands.

**Step 3: crate names.** Rename the packages `selection-core`, `selection-storage`,
`selection-platform-interface`, `selection-platform-windows`, `selection-provider-openai`
and `selection-ui-protocol` to `lingosnap-core`, `lingosnap-storage`, … in their
`Cargo.toml` files and in every dependency entry that points at them. Rename the Rust
paths accordingly (`selection_core::` → `lingosnap_core::`, about 200 uses in 26 files).
Cargo rewrites `Cargo.lock` on the next build; no new dependency is added. Directory names
(`crates/core`, `windows/apps/manager`, …) stay.

**Step 4: the design folder.** `git mv lingosnap-redesign lingosnap-redesign`,
then fix every path that names it: the `include_str!` macro in `popup-ui/src/main.rs`,
`popup.rs`, `.gitignore`, `FAIR_COPY_UI_PLAN.md`, `IMPLEMENTATION_PLAN.md`, `AGENTS.md`,
and this guide.

**Step 5: project rules.** Update `AGENTS.md`: the title "Selection Translate Project
Rules", the exe names, and the package names in the commands. `AGENTS.md` requires this
before structure or conventions change, so make this edit in the same commit as the step
it describes.

**Keep.** Feature names that are not the product: the hot key "Selection translate",
`HotkeyAction::SelectionTranslate`, the TOML key `hotkeys.selection_translate`, and the
Chinese hot key label 划词翻译. The config schema and message names (see "Rules for every
task").

**Check.** `git grep -n -I -i -e "selection.translate" -e "selectiontranslate"` returns
only the "Keep" items. A clean start with an empty `%LOCALAPPDATA%\LingoSnap\` works: the
resident starts, the tray menu opens the manager in both skins, a fair-copy popup appears,
and a translation is saved to history. Releases are now built to
`windows/dist/lingosnap-x64-rNN`.

---

## Finish

1. Run fmt, clippy and the tests (see "Rules for every task"). After R1 the package names
   are `lingosnap-*`.
2. Run the release package to the next `rNN`, and write `BUILD-INFO.txt`.
3. Re-check the warmed idle memory rule in `AGENTS.md`: the resident must stay below
   20 MiB private working set
   (`windows/scripts/measure-memory.ps1 -ProcessName lingosnap-resident`). Task O5 adds an
   icon to the resident; nothing else should touch its memory.
4. Report, for each task, what changed, how it was checked, and anything left undone.

---

## Separate work: moving an existing install to LingoSnap (not code)

After R1, the app looks for its data under the new names, so an existing install starts
with default settings and no saved key. The app does not migrate anything. These are the
manual steps for the user, to write up as user-facing notes in the release README:

1. Quit the old resident from the tray, and close the old manager.
2. Move `%LOCALAPPDATA%\SelectionTranslate\config.toml` and the history database files
   (`history.sqlite3` and any `-wal`/`-shm` next to it) to `%LOCALAPPDATA%\LingoSnap\`.
   The `webview` folder does not need moving; delete it.
3. **API key.** A moved `config.toml` still says
   `credential_target = "SelectionTranslate/OpenAI"`, and the key is still stored under
   that name, so it keeps working. To switch to the new name, open the manager, set
   Credential target to `LingoSnap/OpenAI`, paste the key, and choose Save key. Then
   delete the old entry `SelectionTranslate/OpenAI` in Windows Credential Manager.
4. **Env vars.** If `SELECTION_TRANSLATE_OPENAI_API_KEY`, `…_BASE_URL` or `…_MODEL` are
   set, set the `LINGOSNAP_…` names instead and remove the old ones.
5. Replace any shortcut or startup entry that points at `selection-translate-resident.exe`
   with one for `lingosnap-resident.exe`, then delete the old release folders.
