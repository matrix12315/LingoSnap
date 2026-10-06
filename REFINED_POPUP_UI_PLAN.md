# Refined Popup UI Plan — WebView2 Companion Process

Status: DRAFT for approval. No implementation starts before this plan is
approved. Root Markdown per project change discipline.

## 1. Goal

A second, "delicate and elegant" translation popup rendered from the existing
HTML mockup (`selection-translate-redesign/index.html`) in a WebView2, offered
alongside the current native popup. The user picks a skin in the tray menu;
both skins remain supported indefinitely (explicit product decision: users opt
between performance and refinement).

Non-goals: no change to triggers, extraction paths, coordinator, provider
streaming, history, or the manager. No redesign of the manager in this plan.
No retirement of the native popup.

## 2. Architecture

```
┌────────────────────┐   named pipe, JSON lines   ┌──────────────────────┐
│ resident.exe       │◄──────────────────────────►│ popup-ui.exe         │
│ (native, <20 MiB)  │  events ↓ / commands ↑     │ (new, on demand)     │
│ tray, config,      │                            │  WebView2 host       │
│ hooks, pipeline,   │                            │  └─ assets/index.html│
│ popup registry     │                            │  one HWND per popup  │
└────────────────────┘                            └──────────────────────┘
```

- `windows/apps/popup-ui/` — new Rust binary using `webview2-com` (raw COM
  bindings, matching the codebase's `windows`-crate style; no wry/tao).
- HTML/CSS/JS assets under `windows/apps/popup-ui/assets/`, loaded via
  `SetVirtualHostNameToFolderMapping` (no `file://`).
- One popup window (frameless `WS_POPUP`, topmost, `WS_EX_NOACTIVATE |
  WS_EX_TOOLWINDOW`) per `popup_id`, mirroring the native popup registry.
- The WebView2 runtime itself spawns its own child processes; their memory is
  attributed to the popup-ui tree, never to the resident.

## 3. Invariants (what keeps this architecture healthy)

1. **The resident owns all popup state.** The UI process is a renderer: it
   displays what events say and reports user commands. It keeps no
   authoritative state beyond ephemeral view state.
2. **The pipe protocol is the only coupling.** Small, versioned, and
   documented in this file. Every future popup feature lands as a protocol
   addition here first.
3. **Both skins are first-class.** Every popup feature ships for both skins.
   A feature that only fits one skin must be called out in its PR and either
   ported or explicitly waived in this document's deviation log.

## 4. Protocol v1 (newline-delimited JSON over
`\\.\pipe\SelectionTranslatePopupUi`)

Handshake: popup-ui sends `{"protocol":1,"ready":true}` on connect; the
resident answers `{"protocol":1}`. Unknown fields are ignored (forward
compatibility). Unknown message types are dropped with a trace record.

Resident → popup-ui (events):

| Message | Fields | Purpose |
|---|---|---|
| `show_chooser` | `id, anchor{x,y}, profiles[], default_index` | present the profile rail |
| `show_loading` | `id, target, context?` | replace rail with result card + loading |
| `delta` | `id, text` | streamed provider output (append) |
| `completed` | `id, text` | terminal output (authoritative replace) |
| `local_error` | `id, message` | local failure surface |
| `set_pinned` | `id, pinned` | sync pin state |
| `close` | `id` | resident-side close/replacement/eviction |
| `shutdown` | — | kill switch before process exit |

popup-ui → resident (commands):

| Message | Fields | Purpose |
|---|---|---|
| `register_window` | `id, hwnd` | resident classifies clicks (inside/outside), clamps geometry |
| `select_profile` | `id, index` | equals native `POPUP_PROFILE_SELECTED` |
| `retry` / `show_prompt` / `toggle_pin` / `user_close` | `id` | equal native buttons |
| `copy` | `id` | copies that popup's **raw** output (resident holds the raw buffer) |
| `self_translate` | `id, text` | user selected/hovered text inside the completed output; the resident runs it through the normal pipeline (recursive-hover parity) |
| `ping` | — | liveness for the resident's keep-warm logic |

The result popup is a first-class surface, not a static view: every native
result-popup capability — Copy, Retry, Prompt, Pin, Close, streaming text,
markdown projection, text selection inside the output, and recursive
self-translation of a selected word — must work identically in the refined
skin. The UI displays rendered markdown but `copy` and `self_translate` act
on the raw text the resident already holds; the UI never becomes the source
of translation content.

Each event/command carries a per-`id` monotonically increasing `seq` so the UI
can drop out-of-order delivery; the resident keeps its existing
duplicate-suppression and supersede semantics unchanged.

## 5. Skin switch and process lifecycle

- Config: `config.ui.popup_skin = "classic" | "refined"` (default
  `"classic"`). Saved through the manager or edited by hand; the existing
  config watcher applies it as a UI-only change (no pipeline cancel).
- Tray menu gains a radio pair: "Popup style · Classic / Refined (WebView2)".
- With `refined` selected, the resident lazily starts popup-ui on the first
  popup and keeps it warm. After `popup_ui_idle_timeout_secs` (config, default
  600) without any popup, the resident kills it; the next selection relaunches
  it invisibly. Switching to `classic` kills it immediately.
- If WebView2 runtime is missing or fails to initialize: the resident records
  a one-time local diagnostic and stays on the classic skin. The product never
  becomes unusable because of the refined skin.

## 6. Memory budget and verification

| State | Budget |
|---|---|
| Resident idle, either skin | unchanged, < 20 MiB private WS (hard rule) |
| popup-ui warm, no popup | ≤ 120 MiB private WS (whole process tree) |
| popup-ui after kill | 0 |
| popup-ui cold start → first frame | ≤ 300 ms (hidden pre-warm hides it) |

`windows/scripts/measure-memory.ps1` gains a `-IncludePopupUi` mode measuring
the resident and popup-ui separately, and the 20 MiB assertion stays
resident-only. Verification runs: skin=refined, popup closed, hover off,
manager closed, database closed.

## 7. Native interop details

- **Click classification:** popup-ui registers each popup HWND via
  `register_window`; the resident stores it in the popup registry entry
  (`PopupEntry` gains an external-window variant) so the low-level mouse hook
  keeps doing inside/outside dismissal and hover-replay checks.
- **Geometry:** the resident stays authoritative for anchoring/clamping and
  sends concrete pixel rects inside `show_*` events; popup-ui only applies
  them. Per-monitor DPI awareness is declared in popup-ui's manifest.
- **Focus:** the refined popup never calls `SetForegroundWindow`. Text
  selection/copy inside the WebView works via `SetFocus` on click, mirroring
  the native popup's focus-safe copy contract.
- **Streaming:** deltas are appended in the DOM; markdown rendering is a
  bundled JS renderer (equivalent of the native markdown projection). The
  output buffer cap (`MAX_OUTPUT_UTF16_UNITS`) is enforced resident-side.
- **Recursive self-translation:** the resident's mouse hook cannot see text
  inside a WebView, so the refined popup reports candidate text itself: a
  selection or hover-dwell inside the completed output card sends
  `self_translate {id, text}`. The resident feeds it through the same
  admission path as native self-hover (same gates, same priority, same
  duplicate suppression), producing a NEW popup per the normal rules. The
  JS bridge sends only the selected text — no positioning logic moves into
  the UI process.

## 8. Milestones

1. **M1 — Skeleton.** popup-ui creates one hidden-then-shown WebView2 popup
   from the adapted mockup assets; manual trigger shows it. Exit: the mockup
   is visible as a real topmost popup at the cursor.
2. **M2 — Chooser + streaming + result parity.** Protocol v1 implemented both
   sides; select → refined rail → pick → loading → streamed result; Copy
   copies the raw output; text selection inside the result works; outside
   click dismissal works via `register_window`. Exit: full flow against the
   resident without the native popup, with result-popup interactions
   (Copy/selection) verified.
3. **M3 — Lifecycle parity.** Pin, Retry, Prompt, More… overflow, pinned
   retention, eviction, recursive self-translation (`self_translate`),
   pinned suppression of automatic replacement, config + tray switch,
   keep-warm/kill, crash recovery (resident relaunches popup-ui on next
   popup). Exit: feature parity checklist in this document ticked.
4. **M4 — Hardening.** DPI/multi-monitor clamping, memory verification,
   SETUP/TROUBLESHOOTING docs, packaging with assets, `quick-pack.ps1` builds
   popup-ui too.

## 9. Risks

| Risk | Mitigation |
|---|---|
| IPC ordering races | per-`id` `seq`; resident remains sole state owner |
| WebView2 runtime absent | detect at init; local diagnostic; auto-fallback to classic |
| Focus stealing from the user's app | never `SetForegroundWindow`; focus only on click, mirroring native |
| Dual-UI maintenance drift | invariant 3 + deviation log in this document |
| Startup latency | hidden pre-warm; keep-warm window is configurable |

## 10. Deviation log

(empty — any popup feature intentionally skipping one skin is recorded here)
