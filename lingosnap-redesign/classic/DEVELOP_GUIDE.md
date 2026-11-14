# Selection Translate — PC UI Develop Guide

**Audience:** Implementation agents building the **native Windows desktop** UI (Win32 / WinUI / WPF / egui / etc.).  
**Source of truth for pixels:** `index.html` (same folder) — open it in a browser and match it.  
**Scope:** Visual + layout + interaction of every user-facing surface. Not LLM prompts (see `prompt-templates.md` if needed).

Do **not** invent a second visual language. If a detail is missing, match `index.html` first, then this document.

---

## 1. Product shape

| Surface | Role | Frequency |
|---|---|---|
| **Profile bar** | Choose interpretation mode | Every use |
| **Popup** | Selection + context + result after text is selected | 90% of the time |
| **Manager → Settings** | Provider, credentials, defaults | Rare setup |
| **Manager → Prompts** | Edit system prompt / user template per profile | Occasional |
| **Manager → History** | Search and reopen past results | Occasional |

- One **resident** process captures selection and shows the popup.
- One **Manager** window with **horizontal tabs**: Settings | Prompts | History.
- Never open a “marketing” shell or nested decorative frames around app chrome.

---

## 2. Design tokens (use these exact values)

### Color

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
| on-accent text | `#0B0E13` | Text on filled accent buttons/pills |

Do **not** use pure `#000` / `#FFF`, gradients on chrome, or purple–blue marketing gradients.

### Typography

| Role | Font stack | Size / weight |
|---|---|---|
| UI | `"Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif` | 12–13 body labels; 13 buttons; 26 page titles (Manager) |
| Mono | `"Cascadia Code", "Consolas", ui-monospace, monospace` | 12–13 for target, IDs, output, code, IPA |
| CJK | `"Microsoft YaHei UI", "PingFang SC", "Segoe UI", sans-serif` | Context, translation body |

- Field captions (SELECTION, RESULT, TARGET…): 10–11px, uppercase, letter-spacing ~0.05em, weight 600, color `muted`.
- Line-height: UI ~1.5; mono body ~1.5; context ~1.5.

### Spacing & shape

- Base unit **4px**. Common gaps: 6 / 8 / 10 / 12 / 16 / 20 / 24.
- Card radius: **10px** (popup Selection & Result, manager field groups).  
  Output/History well radius: **6px**.  
  Buttons / inputs: **8px**.  
  Profile pills: **999px** (full pill).
- Border: always `1px solid line`.
- Shadows: only the **floating popup** (e.g. `0 12px 40px rgba(0,0,0,.45)`). Manager is a normal top-level window — no drop shadow gimmicks.

### Scrollbars (mandatory — one style everywhere)

Every scrollable region (popup Selection, Result markdown, System prompt, User template, History list, Output):

| Part | Spec |
|---|---|
| Width / height | 10px |
| Track | transparent |
| Thumb | `line` `#2C3646`, fully rounded |
| Hover thumb | slightly lighter (`#3D4A5E` or similar) |

Never mix light system scrollbars with dark chrome. Never different scrollbar looks per panel.

---

## 3. Shared components

### 3.1 Button

| Variant | Background | Text | Border | Notes |
|---|---|---|---|---|
| **Primary** | `accent` | `void`, weight 600 | none | Copy, Save key, Save settings, Save prompt, Refresh |
| **Default** | `raised` | `ink` | `line` | Retry, Prompt, Previous, Next, New, Copy output |
| **Ghost** | transparent | `ink` | none or transparent | Pin, Close (popup foot) |
| **Danger** | `rgba(err, .08)` | `err` | `rgba(err, .35)` | Delete saved key, Delete selected |
| **Small** | same as parent | | | height 32, pad 12, font 12 |

- Height default **36px**, horizontal padding **14px**, radius **8px**.
- Focus-visible: 2px `accent` outline, 2px offset.
- **Only one filled primary per footer/row group** when possible (e.g. popup: only **Copy** is primary).

### 3.2 Input / field well

- Background `void` or `raised`, border `line`, radius 8px, min-height 36–40px, pad 8–12px.
- Mono for: endpoint, model, credential target, API key mask, ID, temperature, max tokens, system prompt, user template, history output, selection target line.
- UI font for: names, dropdown labels, manager language.
- Dropdown affordance: small `▼` on the right in `muted` (or native combo styled dark).

### 3.3 Horizontal tabs (Manager)

- Bar: `surface`, bottom border `line`, height ~45px, horizontal padding 8–12.
- Items: pad 8×14, radius 8, 13px / 500, color `muted`.
- Active: background `rgba(accent, .14)`, color `accent`, weight 600.
- **No vertical left nav.**
- Status (optional) on the far right of the tab bar: 8px `ok` dot + 12px `muted` text (“Resident is running.” / “History refreshed.”).

### 3.4 Profile pills (**standalone profile bar only** — not inside the popup)

- Container: `surface`, border `line`, radius 999, pad 4, gap 4.
- Pill: pad 8×14, radius 999, 13px / 500, color `muted`.
- Active: fill `accent`, text `void`.
- Active Expert (optional lex treatment): fill `gold`, text `#1A1408`.
- **More…**: `raised` + border `line`, text `ink`. Opens a **dark** command menu (`raised`, radius 10, pad 6) — **never** a white menu.
- Menu item: pad 9–10×10–12, radius 6; hot/hover: `rgba(accent,.12)` + `accent` text.

**Placement rule:** The profile bar is its **own surface** (toolbar / chooser). It must **not** be duplicated inside the popup.

### 3.5 Field group (Manager Settings / History chrome)

- Background `raised`, border `line`, radius 10, padding 10–12.
- Caption (Provider, Credentials, Defaults, Search, Entries): same as field captions (uppercase muted 11px).
- Stacked labeled controls inside; group similar settings (do **not** dump every control in one undifferentiated form).

---

## 4. Surface specs

### 4.1 Popup (single card)

**Default size:** width **420px** (allow 380–460), max height ~78vh or 600px body.  
**Chrome stack (top → bottom):**

1. **Title bar** (`surface`, border-bottom `line`, pad 12×14)  
   - 28×28 mark (radius 8, `accent` fill, dark glyph e.g. “文”)  
   - Bold 13px title “Selection Translate”  
   - 11px `muted` subtitle (e.g. “Resident” or active profile name)  
   - Icon buttons right: Pin, Close (28×28, ghost, `muted`)

2. **Body** (pad 12×14, flex column, gap **10px**, overflow hidden — children scroll)  
   - **Selection card** (aligned with Result — same chrome)  
   - **Result card** (flex: 1)

3. **Footer** (border-top `line`, pad 12×14, gap 8)  
   - **Copy** (primary) · Retry · Prompt · Pin · Close  
   - Only Copy filled.

**No profile bar in the popup.** Profile selection is only the standalone bar (§1 / §3.4). Optionally show the active profile name in the title subtitle — do not embed pills.

#### Selection card (popup)

Must look like a **peer of Result**, not a tinier strip and not a different material:

| Prop | Value |
|---|---|
| Background | `raised` |
| Border | `1px solid line` |
| Radius | **10px** |
| Padding | `8px 12px 10px` |
| Max height | ~30% of body when content is long |
| Overflow | **auto** (unified scrollbar) |

Content — **two paragraphs only** (no `ctx` chip):

```
SELECTION                    ← caption

<target text>                ← paragraph 1: mono 13px ink; pre-wrap; break word
<context text>               ← paragraph 2: muted CJK 12–13px; pre-wrap; break word
```

- **Integrate Target + Context in this one card** (do not ship two separate boxes).
- Distinguish target vs context **only by paragraph + type/weight/color** — no chip, no second caption.
- Long target or context: wrap and scroll inside the card. Do **not** one-line ellipsis the whole context away.

#### Result card (popup)

| Prop | Value |
|---|---|
| Background | `raised` |
| Border | `1px solid line` |
| Radius | **10px** |
| Padding | `8px 12px 10px` |
| Flex | fills remaining body height |
| Inner body | scrollable markdown area |

- Caption row: `RESULT` (same caption style as SELECTION). Optional small muted badge for profile name — **no** “H2 format” / prompt-contract chrome.
- Body is **Markdown rendered text** (headings, paragraphs, lists, inline code).  
  - H2: 14px / 600, margins 14 top / 6 bottom.  
  - `code`: mono 12px, `gold`.  
  - Lists: disc or subtle markers; CJK-friendly metrics.  
- **Do not** invent custom lexicon components (gold rails, IPA rows as special widgets) unless product explicitly asks later. Keep one markdown surface.

---

### 4.2 Manager window

**Window chrome:**

1. **Title bar** — `surface`, border-bottom `line`, pad 8×12  
   - 14×14 accent square (or app icon) + 12px “Selection Translate — Manager”  
   - Standard native close (and min/max if the OS frame provides them).

2. **Horizontal tab bar** (see 3.3) — Settings | Prompts | History.

3. **Content area** — `void` background; page padding ~16–20 / 20–24.  
   Fixed content height ~**720px** (or match a large default Manager size). Editors/Output use **flex layout** so they take leftover space — never only “make min-height larger.”

**Window borders:** the Manager **keeps its own window border**. Do **not** wrap pages in extra decorative outer containers on top of the window.

---

### 4.3 Settings tab

Groups (field groups, stacked):

**Provider**  
- Endpoint (mono)  
- Model (mono)  
- Credential target (mono)

**Credentials**  
- API key (masked, muted)  
- Actions: Save key (primary) · Delete saved key (danger)  
- Status: “Key present · value hidden” (`ok` on “Key present”)  
- Hint: keys in Windows Credential Manager; never in `config.toml`

**Defaults**  
- Selection profile (dropdown)  
- Hover profile (dropdown)

**Footer actions**  
- Save settings (primary)  
- No second long disclaimer under the button if it already lives in Credentials.

---

### 4.4 Prompts tab

**Layout order (top → bottom):**

1. **Meta row (compact)** — flex wrap, gap 10, items end-aligned  
   - **ID — dropdown** (switch profile; list all profile ids/names)  
   - Name  
   - Model override (optional empty)  
   - Temperature  
   - Max tokens  
   - **Save prompt** (primary)

   Placeholders: `{target}`, `{context}`, `{source}`; user template **must** contain `{target}`.

2. **Editors (fill remaining height)** — **two columns**, gap 10  
   - Left: **System prompt**  
   - Right: **User template**  
   - Each editor: `raised` frame (border `line`, radius 10), caption on top, inner mono well `void` (radius 8), pad 10–12, **fills column height**, scrolls.

**Do not put on this tab:**

- Page title “Prompts” / long subtitle  
- `1 of 6` / Previous / Next / New pager  
- Selection default / Hover default (those belong to **Settings**)

**Do:**

- **ID is a dropdown** so the user switches prompts quickly (no pager required).

---

### 4.5 History tab

**Layout (top → bottom):**

1. **Search group** (field group)  
   - Row: Search target/output (grow) · **Refresh** (primary) · Copy output (default)  
   - Filters row: Prompt (All prompts) · Source (All sources) · Order (Newest)

2. **Entries group** (field group, **flex fill**)  
   - Split: **list left (~280px)** | **detail right (flex)**  
   - List: `void` well, rows with target (mono), preview (CJK muted), time · profile (mono small). Selected row: `rgba(accent,.1)` + subtle accent left edge.  
   - Detail:  
     - **Selection card** (same language as popup Selection / Output card) — target + ctx; max-height ~36% of detail; scrolls.  
     - **Output** caption + **Output card** (`void`, border `line`, radius 6, mono 12, pad 8–10, **flex fill**, pre-wrap, scroll) — **this is the large area**.  
   - Hint under entries: DB only loaded while tab open.

3. **Footer actions**  
   - `104 entries` (or live count) · Delete selected (danger)

**Do not** render history as a raw `target | ## Translation…` dump.

---

## 5. Layout rules (space allocation)

| Region | Policy |
|---|---|
| Popup Selection | Compact peer of Result; scrolls if long (~30% max) |
| Popup Result | **Primary** — all remaining body height |
| Prompts System + User | **Primary** — remaining height after meta row |
| History Output | **Primary** — remaining height after Selection + chrome |
| Manager chrome (tabs, search) | Fixed compact height |

Never solve “too small” only by raising `min-height` on a box. Reallocate: shrink secondary chrome, let the important region **flex**.

---

## 6. Interaction notes

| Action | Behavior |
|---|---|
| Select text (resident) | Capture selection + context sentence → open popup with **default selection profile** |
| Change profile pill | Re-request with new profile; keep selection/context |
| Copy | Copy result/output text; brief ok state if possible |
| Retry | Re-run same job |
| Prompt | Jump to Prompts tab (or open Manager on that profile ID) |
| Pin | Keep popup open |
| Close / Esc | Close popup |
| Save settings / key / prompt | Persist; status text updates; keys → Credential Manager only |
| Prompts ID dropdown | Load that profile’s name, system prompt, user template, sampling |
| History row select | Fill Selection + Output; Refresh reloads list; Copy output copies raw output |
| Cycle profiles (hotkey) | Same as More… order; optional |

Keyboard: all controls focusable; visible accent focus ring; dialog Esc closes popup.

---

## 7. Content / copy (English UI)

Use these strings unless product localizes later:

- Window: `Selection Translate — Manager`
- Tabs: `Settings` · `Prompts` · `History`
- Status: `Resident is running.` · `History refreshed.`
- Popup title: `Selection Translate`
- Captions: `SELECTION` · `RESULT`
- Popup actions: `Copy` · `Retry` · `Prompt` · `Pin` · `Close`
- Settings: `Endpoint` · `Model` · `Credential target` · `API key` · `Save key` · `Delete saved key` · `Key present` · `value hidden` · `Selection profile` · `Hover profile` · `Save settings` · credential-manager footnote
- Prompts: `ID` · `Name` · `Model override` · `Temperature` · `Max tokens` · `Save prompt` · system/user captions `SYSTEM PROMPT` / `USER TEMPLATE` · placeholder hint
- History: `Search target/output` · `Refresh` · `Copy output` · `All prompts` · `All sources` · `Newest` · `Selection` · `Output` · `Delete selected` · `{n} entries` · DB footnote

Voice: short, factual, verb-first buttons. No marketing filler. Errors state what failed and how to fix.

---

## 8. Accessibility

- Body text ≥ 4.5:1 on its background; large titles ≥ 3:1.
- Status never color-only: pair `ok`/`err` with text.
- Hit targets ≥ 32×32 (prefer 36 height for buttons).
- Focus-visible ring on every interactive control.
- Respect reduced-motion if you animate pill/selection fades.
- Chinese/English mixed text: use the CJK stack for context and translation body.

---

## 9. Explicit anti-rules (do not ship)

1. White dropdown / menu under dark chrome.  
2. Vertical left tab list for Manager (use horizontal tabs).  
3. Prompts page title block, `1 of 6` pager, or Selection/Hover defaults on Prompts.  
4. Separate Target box + Context box in the popup (they are **one** Selection card).  
5. Target/Context as huge multi-line wells that crowd Result/Output.  
5b. `ctx` chips or extra labels between target and context (use two paragraphs only).  
6. Result as custom multi-widget “lexicon” instead of markdown (unless redesign is re-approved).  
7. Prompt-format labels (`## Translation` as UI chrome, “H2 format” badges).  
8. History list of raw markdown one-liners.  
9. Outer decorative board frames inside the Manager window.  
10. Multiple filled primaries in one footer.  
11. Different scrollbar styles per panel.  
12. Storing API keys in `config.toml` or UI-only insecure storage.  
13. **Duplicate profile pills inside the popup** (profile bar is standalone only).

---

## 10. Implementation checklist for the agent

- [ ] Define color/type/spacing tokens once in code (theme constants).  
- [ ] Implement shared Button / Input / FieldGroup / Tabs / Pills / Scrollbar.  
- [ ] Popup: Selection + Result peer cards; **no profile bar**; Result markdown; footer Copy primary.  
- [ ] Standalone profile bar (§1) is the only place with Translate/Expert/Program/Concise/More… pills.  
- [ ] Manager: title bar + horizontal tabs + 3 pages.  
- [ ] Settings: three field groups + Save settings.  
- [ ] Prompts: meta row (ID **dropdown**) + two large editors; no pager/defaults.  
- [ ] History: search chrome + list/detail; Selection compact; Output flex large.  
- [ ] Unified dark scrollbars on every scroller.  
- [ ] Wire real config fields (endpoint, model, credential_target, profiles, defaults, history).  
- [ ] Visual QA against `index.html` at 100% DPI and 125–150% DPI.  
- [ ] Verify long selection + long context + long output scroll without breaking flex.

---

## 11. File map (this folder)

| File | Purpose |
|---|---|
| `index.html` | Pixel/layout reference mockups (open in browser) |
| `DESIGN.md` | Design system rationale + decision trace |
| `prompt-templates.md` | LLM prompt format contracts (if implementing profiles) |
| `DEVELOP_GUIDE.md` | This document — implement UI to match |

When mockup and guide disagree, fix the mockup or update this guide in the same change set so they stay in sync.
