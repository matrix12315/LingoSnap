---
feature: ui-redesign
status: delivered
updated: 2026-03-14
branch: mimoUI
commits: a2d90a6..WORKING-TREE
---

# Native UI Redesign (Popup + Manager)

## Report

**What was built** — Replaced the bronze/espresso native chrome with the cool Raycast/Linear-class dark system from the HTML mockup. One shared `theme.rs` feeds popup and Manager. The result popup is 420×520 with a title bar (mark + name + Pin/Close), a profile rail of segmented pills (first pill active) plus a dark More… menu, one integrated Selection card (SELECTION + mono target + `ctx` context), a flex Result markdown card, and a footer where Copy is the only filled primary. Manager uses horizontal Settings | Prompts | History tabs, grouped Provider/Credentials/Defaults field cards, a Prompts ID dropdown with two flex editors (no pager), and a History list/detail split with a large Output well. English and Simplified Chinese cover the new labels.

**Verification** — `cargo test --workspace` PASS 256 (56+2+140+24+12+19+3). `cargo clippy --workspace --all-targets -- -D warnings` PASS. `cargo fmt --all -- --check` PASS. Optimized MSVC release builds of `selection-translate-manager` and `selection-translate-resident` PASS. Visual QA: seven inspected screenshot rounds (`windows/tmp/ui-redesign-popup/popup_preview.png`, `windows/tmp/ui-redesign-qa/round1`–`round7`). Resident behavior contracts (no-activate, pin, cascade, clipboard, Credential Manager keys) are unchanged.

**Journey log** — (1) Palette and geometry were one atomic switch; try to keep tokens in `theme.rs` only. (2) Owner-drawn pills needed `GWLP_USERDATA` slots — `GetDlgCtrlID` alone was unreliable for the active state. (3) `STATIC` labels painted black-on-black; field labels are painted by `paint_page` with the HWNDs kept hidden. (4) Windows accent-color caption bars ignore `DWMWA_USE_IMMERSIVE_DARK_MODE` here; the OS caption still shows above the custom title. (5) Seven rounds were required: empty profile rail on first show, active-pill first paint, then first-label text vanishing.

## [S1] Problem

The native Windows UI still carries a warm bronze/espresso palette and older geometry taken from an abandoned egui mockup (`WIDTH=660`, equal-width footer buttons, separate TARGET/CONTEXT wells, Manager left-rail nav). That surface reads as unfinished product chrome and does not match the approved HTML design system at `D:\Temp\selection-translate-redesign` (`index.html` + `DEVELOP_GUIDE.md`).

User-visible failures:

1. Popup is too wide (660 logical px) and visually heavy for a result-at-a-glance tool.
2. Selection and Context are two stacked wells instead of one peer Selection card with Target + `ctx` context.
3. Profile choice is a temporary strip that replaces Result; there is no stable profile rail / More… dark menu.
4. Footer actions are equal-width; mockup requires Copy as the only filled primary.
5. Manager uses a left nav and undifferentiated form dumps; mockup requires horizontal tabs and grouped field cards (Provider / Credentials / Defaults).
6. Prompts still has a pager (`1 of N` / Previous / Next / New) and page title chrome the mockup forbids.
7. Colors, type scale, radii, and scrollbars do not match the cool Raycast/Linear-class dark utility in the mockup.

## [S2] Design

### S2.1 Source of truth and overrides

- Pixel/layout source of truth: `D:\Temp\selection-translate-redesign\index.html`.
- Implementation contract: `D:\Temp\selection-translate-redesign\DEVELOP_GUIDE.md`.
- Where `DESIGN.md` (lexicon gold-rail widgets) conflicts with `DEVELOP_GUIDE.md` §4.1 Result card (**plain markdown surface, no custom lexicon widgets**), follow `DEVELOP_GUIDE.md` and `index.html`.
- Workspace override: use the existing linked worktree `D:\pythonProject\y2026\LingoSnap-mimoUI` (branch `mimoUI`). Do not nest another worktree. Design assets stay at `D:\Temp\selection-translate-redesign`; product code stays in this repo.
- Localization: keep English + Simplified Chinese. Mockup English strings are the EN source of truth; ZH strings follow existing manager localization tables and must cover every new/changed label.
- Non-goals for this pass: no Electron/WebView, no OCR architecture change, no prompt-contract/LLM schema change, no credential storage change, no history schema change.

### S2.2 Shared theme tokens

Define one theme module in `windows/crates/platform-windows/src/theme.rs` (exported for popup + manager). COLORREF remains `0x00BBGGRR`.

| Token | Hex | Use |
|---|---|---|
| `void` | `#0B0E13` | App chrome, editor bodies, list/scroll wells |
| `surface` | `#12171F` | Window title bars, tab bars, popup chrome |
| `raised` | `#1A2230` | Cards, inputs, Selection/Result panels, menus |
| `line` | `#2C3646` | 1px borders, dividers, scrollbar thumb |
| `ink` | `#F0F4FA` | Primary text |
| `muted` | `#8B97A8` | Labels, captions, context, hints |
| `accent` | `#8BACFF` | Active tab/pill, primary buttons, focus, `ctx` chip |
| `gold` | `#D4B56A` | IPA / code tokens / rare emphasis only |
| `ok` | `#4FD1A5` | Resident live, key present |
| `err` | `#F07178` | Delete key, Delete selected, errors |
| `on-accent` | `#0B0E13` | Text on filled accent buttons/pills |

Typography (DPI-scaled): UI `"Segoe UI"` / `"Segoe UI Variable Text"` 12–13 body, 13 buttons, 26 page titles; Mono `"Cascadia Code"` / `"Consolas"` 12–13; CJK `"Microsoft YaHei UI"` for context and translation body. Captions 10–11px uppercase letter-spacing ~0.05em weight 600 muted.

Spacing base 4px (6/8/10/12/16/20/24). Radii: cards 10, wells 6, buttons/inputs 8, pills 999. Borders `1px solid line`. Shadow only on floating popup.

Unified dark scrollbars on every scroller: 10px, transparent track, `line` rounded thumb, hover slightly lighter.

### S2.3 Popup

Geometry: default width **420** logical px (clamp 380–460), max body height **min(72–78vh, ~600)**, outer radius **12**. Replace current `WIDTH=660` / `HEIGHT=470` defaults.

Chrome stack top → bottom:

1. **Title bar** (`surface`, border-bottom `line`, pad 12×14): 28×28 accent mark (“文”), bold 13px “Selection Translate”, 11px muted subtitle (**active profile name** or “Resident”), icon buttons Pin + Close (28×28 ghost).
2. **Body** (pad 12×14, flex column, gap 10):
   - **Selection card** — one card integrating Target + Context (not two wells). `raised`, border `line`, radius 10, pad 8×12×10, max-height ~30% body, overflow auto. Caption `SELECTION`. Target mono 13 ink pre-wrap. Context row: `ctx` chip + muted CJK pre-wrap.
   - **Result card** — fills remaining height. Same chrome. Caption `RESULT`. Body is Markdown via existing RichEdit.
3. **Footer** (border-top `line`, pad 12×14, gap 8): **Copy** (primary, only filled) · Retry · Prompt (default) · Pin · Close (ghost).

**Amendment (guide §3.4 / §4.1, 2026-03-23):** **No profile bar in the popup.** Do not embed pills. Profile selection lives only on the **standalone profile bar** (own toolbar/chooser surface, mockup §1 / §3.4). Optionally show the active profile name in the title subtitle.

**Standalone profile bar** (own surface, not inside popup): pill container `surface` + `line`, radius 999, pad 4, gap 4. Pills pad 8×14 radius 999; active = accent fill + on-accent text; More… = raised + line, dark command menu. Shown for profile cycle (`Ctrl+Alt+P`) and explicit profile choice; a pill click re-requests with the same selection/context.

Behavior preserved: `WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW`, drag band, custom resize, outside-click dismiss (unpinned), pin, streaming coalesce timer, cascade popups, profile chooser/reuse rules, Esc/Close, clipboard-safe Copy of raw markdown.

Profile chooser when cycling/More…: dark raised menu/strip consistent with profile rail tokens; never replace Result with a light strip. If a compact chooser is still required for cascade popups, restyle it to the profile-pill language.

### S2.4 Manager

Window keeps OS frame. Content:

1. **Title bar** — `surface`, 14×14 accent mark + “Selection Translate — Manager”, native close/min/max as available.
2. **Horizontal tab bar** (replace left nav `NAV_WIDTH=176`): Settings | Prompts | History. Height ~45, `surface`, bottom border `line`. Active tab `rgba(accent,.14)` + accent text weight 600. Status chip on the far right: 8px `ok` dot + muted “Resident is running.” / “History refreshed.”
3. **Content area** — `void` background; page padding ~16–24. Fixed content height ~720 default. Editors/Output use flex leftover height (never min-height-only growth).
4. **No outer decorative board frames** inside the window.

**Settings** — stacked field groups (`raised`, radius 10):

- **Provider**: Endpoint, Model, Credential target (mono).
- **Credentials**: API key (masked muted), Save key (primary), Delete saved key (danger), status “Key present · value hidden” (`ok` + text), footnote keys in Windows Credential Manager never `config.toml`.
- **Defaults**: Selection profile, Hover profile dropdowns.
- Footer: Save settings (primary only).

**Prompts** — no page title block, no `1 of N` pager, no Selection/Hover defaults here.

1. Compact meta row: **ID dropdown** (switch profiles), Name, Model override, Temperature, Max tokens, **Save prompt** (primary). Placeholders `{target}`, `{context}`, `{source}`; user template must contain `{target}`.
2. Two editors fill remaining height: System prompt | User template. Each: raised frame + caption + inner mono `void` well, flex fill, scroll.

**History**:

1. Search group: Search target/output · Refresh (primary) · Copy output; filters Prompt / Source / Order.
2. Entries group flex fill: list left ~280px | detail right. List rows: mono target, CJK muted preview, time · profile. Selected: `rgba(accent,.1)` + accent left edge. Detail: Selection card (compact, max-height ~36%) + large Output card (`void`, radius 6, mono 12, pre-wrap, flex fill).
3. Footer: `{n} entries` · Delete selected (danger). Hint: DB only loaded while tab open.

### S2.5 Accessibility and interaction

- Body ≥4.5:1 contrast; status never color-only; hit targets ≥32×32 (buttons 36 preferred); focus-visible 2px accent outline + 2px offset; reduced-motion respected.
- Keyboard: all controls focusable; Esc closes popup; tab order follows visual order.
- Existing behavior contracts stay intact (no-text → no request; keys in Credential Manager; no retained screenshots; resident <20 MiB idle).

### S2.6 Visual QA contract (required before “done”)

Because the user bar is “tested many times and the UI is the best”, delivery must include multiple inspected screenshot rounds, not a single smoke:

1. Build optimized MSVC resident + manager after each visual iteration.
2. Capture deterministic screenshots into `windows/tmp/ui-redesign-*/` (dev-only; not product retention): popup loading, popup completed (long selection + long context + long markdown), profile menu open, manager Settings / Prompts / History at 100% and at least one high DPI (125–150% or active).
3. Inspect each screenshot against `index.html` and `DEVELOP_GUIDE.md` anti-rules (§9). Fix real defects (clipping, wrong tokens, white menus, crowding Result, scrollbar clash, equal-width primary dump).
4. Iterate until: tokens match, hierarchy matches, long-content flex/scroll holds, and the result card remains the visual hero.
5. Run formatting, workspace tests, and strict Clippy after the final code state.

## [S3] Out of Scope

- LLM prompt templates, response schema, cache identity, provider client.
- Extraction pipeline (UIA / native / clipboard / OCR) and trigger logic.
- Credential storage backend, SQLite schema, history retention policy.
- Android / macOS, Electron/WebView, bundled OCR model.
- Changing product feature set beyond the mockup’s navigation/layout (e.g. new profiles product-wide).
- Committing design mockups into this repository (they remain at `D:\Temp\selection-translate-redesign`).

## Tasks

- [x] T1: Add shared theme tokens (`theme.rs`) matching S2.2 and wire popup + manager to them — acceptance: popup and manager color/font/radius constants come from one module; unit test asserts RGB values match the token table (covers: S2.2)
- [x] T2: Rebuild popup chrome to S2.3 (title bar, profile rail, Selection card with integrated Target+ctx, Result markdown card, footer Copy primary) — acceptance: layout tests + screenshots show 420-class width, one Selection card, Result flex fill, only Copy filled (covers: S2.3; depends: T1)
- [x] T3: Restyle profile rail + More… dark menu; remove light/white menu paths — acceptance: active pill accent/on-accent; More… menu uses raised/line; no white dropdown (covers: S2.3; depends: T2)
- [x] T4: Rebuild Manager to horizontal tabs + S2.4 Settings/Prompts/History layouts (no left nav, no Prompts pager) — acceptance: screenshots of all three tabs match mockup structure; ID dropdown switches profiles; Save actions use button variants correctly (covers: S2.4; depends: T1)
- [x] T5: Apply unified dark scrollbars, focus rings, captions, EN+ZH copy for every changed label — acceptance: all scrollers use the shared scrollbar helper; localization tables cover new strings; status text pairs color with words (covers: S2.2, S2.4, S2.5; depends: T2, T4)
- [x] T6: Visual QA loop — capture and inspect popup + manager screenshots across states/DPI, fix defects, repeat until mockup fidelity and polish bar met — acceptance: ≥3 inspected iteration rounds with residual notes empty or justified; anti-rule checklist clean (covers: S2.6; depends: T2, T3, T4, T5)
- [x] T7: Full verification — fmt, workspace tests, strict Clippy, optimized MSVC builds of resident + manager — acceptance: all commands pass; failures documented as PRE-EXISTING with id (covers: S2.6; depends: T6)
