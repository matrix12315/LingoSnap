# Fair Copy UI Plan (WebView2 companion)

Status: approved 2026-10-07. Implements the fair-copy mockup in
`selection-translate-redesign/fair-copy/ui/` as a second, tray-switchable UI.

## Decisions

- **Pixel-exact rendering.** The real mockup assets (`chooser.html`,
  `popup.html`, `popup-streaming.html`, `popup-error.html`, `manager.html`,
  `fair-copy.css`, `theme.js`, `liquid-lens.js`) are embedded in the UI
  executable and hosted in WebView2. No redesign, no native approximation.
- **All five surfaces** in this round: chooser, popup (result/streaming/error),
  and manager.
- **Memory contract.** The warmed idle resident stays below 20 MiB private
  working set (popup closed, manager stopped) — unchanged. The WebView2 UI
  process costs roughly 80–120 MiB while a popup or the manager is visible;
  this is accepted explicitly by the user, and the UI process is killed when
  closed/idle so nothing lingers.
- **Switching.** `ui.popup_skin = "classic" | "fair-copy"` in config.toml,
  exposed as a tray-menu radio pair. Switching to classic kills the UI process.

## Architecture

- **Companion process `popup-ui.exe`** (new `windows/apps/popup-ui/`, using
  `webview2-com`): one WebView2 window per visible surface, showing the
  embedded mockup page for that surface.
- **Protocol v1 is the only coupling.** JSON lines over the named pipe
  `\\.\pipe\SelectionTranslatePopupUi`. Handshake: the UI sends
  `{"protocol":1,"ready":true}`; each line carries `seq` and `id`.
  - Events (resident → UI): `show_chooser`, `show_loading`, `delta`,
    `completed`, `local_error`, `set_pinned`, `close`, `shutdown`.
  - Commands (UI → resident): `register_window`, `select_profile`, `retry`,
    `toggle_pin`, `user_close`, `copy`, `self_translate {id,text}`, `ping`.
- The resident owns all popup state, exactly as it does for the native popup;
  the UI process is stateless presentation.
- **Fallback.** Missing WebView2 runtime or pipe failure → classic native
  popup for that session. A translation is never blocked on the UI.
- **Self-translation parity.** The resident's mouse hook cannot see selection
  inside a WebView, so the UI reports selected fair-copy text via the
  `self_translate` command. Every popup feature (copy, retry, pin,
  self-translation, close) must work in both skins.

## Milestones

- M0 — this plan document.
- M1 — skeleton: `popup-ui.exe` shows `popup.html`/`chooser.html` with sample
  data over the pipe; `ui.popup_skin` config + tray radio; classic fallback.
- M2 — popup-family parity: streaming deltas + caret + seal, local error,
  copy/retry/pin/user-close, self-translate reporting, non-activating popup
  with correct z-order (chooser above source popup, Ctrl+C behavior matching
  the r25/r27 fixes).
- M3 — manager: WebView2 presentation of `manager.html` driven by the
  existing manager logic (Settings/Prompts/History unchanged underneath).
- M4 — lifecycle and packaging: lazy launch, keep-warm, kill after 600 s
  idle, kill on skin switch; asset embedding and `package-release.ps1`
  updates; memory verification; per-surface visual checks against the
  `.impeccable/review` captures; protocol and fallback tests.

## Deviation log

- The mockup Settings page shows four hot-key fields (Selection translate,
  Hover translate, Pause/resume, Open popup). These were implemented first in
  the classic native manager so both skins expose the same settings; the
  config keys live in `[hotkeys]` (`selection_translate`, `hover_translate`,
  `pause_resume`, `open_popup`), with an empty string meaning "off".
- "Open popup" re-presents the most recent completed popup and falls back to
  the manual selection trigger when none is reopenable.
- The companion starts **lazily** with the first popup request, so an unused
  resident pays zero companion memory and a missing WebView2 runtime costs
  one bounded handshake attempt. A failed start is remembered for the
  session; an explicit tray skin switch clears that memory and retries once.
- Known limitation: the very first popup after the companion process is
  started may arrive while the page is still navigating, so its content can
  be missed on screen (the request itself still completes normally). The
  keep-warm lifecycle (M4) is the place to add a ready-replay if needed.

## Implementation status (2026-10-07)

- Done: M0, M1, M2, M3, M4.
  - M3: the manager spawns `popup-ui --surface manager` on open (its own pipe
    `\\.\pipe\SelectionTranslateManagerUi`), hides the native window while the
    page is alive, pushes settings/prompts/history snapshots, and applies page
    commands (`save_settings`, `select_prompt`, `new_prompt`, `save_prompt`,
    `history_query`, `refresh_history`, `select_history`) through the same
    validation/save paths as the native controls. Closing the page restores
    the native manager via a child-exit poll timer.
  - M4 verified live (dev flags `--capture=path` / `--hold=ms` in popup-ui):
    PNG previews of the popup, chooser, and manager surfaces match the
    mockup design; memory while the popup is open measured at ~2.8 MB host +
    ~152 MB WebView2 processes (6 procs, first-run cache warm-up included),
    and zero `popup-ui`/new `msedgewebview2` processes remain after close;
    the resident's <20 MiB idle budget is untouched (lazy start, separate
    process).
- Known limitation: the very first popup after the companion process is
  started may arrive while the page is still navigating, so its content can
  be missed on screen (the request itself still completes normally).
