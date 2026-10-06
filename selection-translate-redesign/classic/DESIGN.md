# Selection Translate — Design System

## Identity

Product UI Designer — the 90% surface is the **result popup after a selection**; everything else exists to configure or recall that moment.

## Grounding

**Assumption:** Selection Translate is a Windows resident linguistic utility (select text → floating analysis). Treat the redesign as a **Raycast / Linear-class dark utility** with **DeepL-grade content hierarchy** for the result card — not a generic HTML form in corporate navy.

**Defer:** Live prompt bodies, real API field labels, and History sample rows are placeholders shaped like real data; product copy can be swapped without breaking the system.

---

## 1. Objective

Replace the current UI (raw navy form chrome, chunky profile buttons, inconsistent dropdown, unreadable result dump) with one coherent dark system that:

1. Makes the **selected text + context + result** the visual hero.
2. Distinguishes **profiles** as modes, not random buttons.
3. Turns Settings from a long form into **grouped, scannable cards**.
4. Feels native to a **Windows power-user tool**, not a web dashboard.

---

## 2. Product Context

| Surface | Job | Frequency |
|---|---|---|
| Profile bar | Choose how to interpret the selection | Every use |
| Translation popup | Deliver analysis you can trust at a glance | 90% of the time |
| Settings | Provider, credentials, defaults | Rare setup |
| Prompts | Edit profile prompt templates | Occasional |
| History | Reopen past results | Occasional |

**Audience:** People reading foreign docs, code comments, papers, UI strings on Windows — they want speed and clarity, not marketing polish.

---

## 3. Visual Foundations

### Palette

| Token | Hex | Role |
|---|---|---|
| `--void` | `#0B0E13` | App chrome / backdrop |
| `--surface` | `#12171F` | Panels, manager shell |
| `--raised` | `#1A2230` | Cards, inputs, menus |
| `--lex` | `#1E2634` | Result “manuscript” body (slightly warmer raised) |
| `--line` | `#2C3646` | Hairline borders |
| `--ink` | `#F0F4FA` | Primary text |
| `--muted` | `#8B97A8` | Labels, captions, context |
| `--accent` | `#8BACFF` | Interactive primary, active profile |
| `--lex-gold` | `#D4B56A` | Linguistic emphasis: IPA, Expert, section marks |
| `--ok` | `#4FD1A5` | Copy success, key saved, resident live |
| `--warn` | `#F0A35E` | Warnings |
| `--err` | `#F07178` | Delete key, errors |

No purple-blue gradients. No pure `#000` or pure `#FFF`.

### Typography

| Role | Stack | Scale / weight |
|---|---|---|
| UI | `"Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif` | 12 / 13 / 14 / 16 / 20 / 28 |
| Mono / IPA / code | `"Cascadia Code", "Consolas", ui-monospace, monospace` | 12 / 13 |
| CJK | `"Microsoft YaHei UI", "PingFang SC", "Segoe UI", sans-serif` | inherits |

- UI labels: 12px, `letter-spacing: 0.04em`, uppercase only for **eyebrow** row labels (`TARGET`, `CONTEXT`, `RESULT`).
- Body/result: 14–15px, line-height 1.55.
- Display (page titles in Manager): 28px / 600, not 40px marketing.

### Layout & density

- Base unit: **4px**. Rhythm: 8 / 12 / 16 / 24 / 32.
- Popup width: **440px**; max height **min(72vh, 640px)**; radius **12px**.
- Manager: **220px** left nav + fluid content, max content width **720px**, horizontal padding 32.
- Cards: radius **10px**, border `1px solid var(--line)`, no heavy shadows (use `0 12px 40px rgba(0,0,0,.45)` only on floating popup).
- Density: compact — this is a utility, not a landing page.

### Signature elements

1. **Lexicon card** — result body sits on `--lex` with a thin left rail in `--lex-gold`; IPA in mono + gold.
2. **Profile rail** — segmented pills; active = accent fill + dark text OR accent underline glow; inactive = raised surface, muted ink.
3. **Command menu** for “More…” — same dark raised surface as popup (never light-on-dark clash).

---

## 4. Accessibility

- Body text contrast ≥ 4.5:1 on `--surface` / `--lex`; large titles ≥ 3:1.
- Focus-visible: `outline: 2px solid var(--accent); outline-offset: 2px` — never `outline: none`.
- Hit targets ≥ 32×32 for toolbar icons; popup actions ≥ 36px height.
- Status never color-only: include text (“Key present”, “Resident running”).
- `prefers-reduced-motion: reduce` disables pill slide and result fade.

---

## 5. Voice & Tone

- Labels are **controls**, not descriptions: “Save settings”, “Save key”, “Delete key”.
- Status is factual: “Key present · value hidden”, “Resident is running”.
- Result structure speaks linguist: `Translation`, `Words`, `Idioms & Grammar`, `Error check` — not bullet soup.
- No marketing filler on any screen.

---

## 6. Implementation Practices

- CSS variables only for color/type/spacing; no hardcoded hex in components.
- One visual language across Popup / Manager / menus.
- Mockups as static HTML for design review; production can map 1:1 to the same tokens.
- Prefer system fonts (Windows product); no external font CDN required.

---

## 7. Anti-Patterns

- ❌ Corporate navy form with left-label / right-field dump for all settings.
- ❌ White dropdown under a dark toolbar.
- ❌ Chunky equal-width web buttons for Expert/Program/Concise.
- ❌ Result as raw markdown with “English: None” noise.
- ❌ Emoji decoration, gradient heroes, generic icon-card grids.
- ❌ Every action as a filled primary button (Copy can be primary; Close stays quiet).

---

## 8. Decision-Making (when in doubt)

1. Prefer **scannability of the result** over chrome decoration.
2. Prefer **grouped settings cards** over one long form.
3. Prefer **mode language** (profiles) over button soup.
4. Prefer **quiet Manager, expressive popup** — spend visual weight on the 90% surface.
5. If a control’s meaning isn’t obvious in 2 seconds, the label is wrong — not the user.

---

## 9. Workflow

1. Approve this token system + structure.
2. Implement in product (or continue HTML → code handoff).
3. QA against §4 and the popup golden path (select → profile → result → copy).
4. Extend tokens before inventing one-off styles.

---

## Page map (structure)

### A. Profile bar
Horizontal segmented control. Order: **Expert · Program · Concise · More…**  
Active profile gets accent treatment + optional gold “linguist” chip when Expert.  
More… opens a dark command menu: Contextual, Word, Wiki (+ future profiles).

### B. Translation popup
Top chrome: product mark + name + resident hint + pin/close.  
Body stack:
1. **TARGET** — selected text, mono-leaning, selectable.
2. **CONTEXT** — muted surrounding sentence(s), collapsible.
3. **RESULT / Lexicon card** — structured sections (Translation, Words with IPA, Idioms & Grammar, Error check).
Footer actions: **Copy** (primary) · Retry · Prompt · Pin · Close (ghost).

### C. Settings (Manager)
Nav: Settings · Prompts · History + resident status.  
Content groups (cards, not one dump):
1. **Provider** — endpoint, model, credential target.
2. **Credentials** — API key field, Save key, Delete key, vault note.
3. **Defaults** — Selection profile, Hover profile.
4. Footer note: keys in Windows Credential Manager, never `config.toml`.

### D. Prompts
List of profiles with short description; select → editor (system prompt template) with Save / Restore default.

### E. History
Timeline/list of past (target snippet, profile, time); click → reopen popup content read-only.

---

## Decision Trace

```json
[
  {
    "decision": "Raycast/Linear dark utility + DeepL result hierarchy",
    "reason": "Product is a resident Windows tool; current navy form UI reads as unfinished web admin, not a companion app",
    "alternatives": ["Keep navy corporate", "Light DeepL-clone", "Expressive gradient AI app"],
    "tradeoff": "Less 'friendly SaaS' on first glance; tighter fit for power users"
  },
  {
    "decision": "Accent #8BACFF instead of #3B82F6 corporate blue",
    "reason": "Keeps interactive affordance but softens out of default Bootstrap/AI navy",
    "alternatives": ["#3B82F6", "acid green", "pure gold accent"],
    "tradeoff": "Slightly lower 'instant button' loudness; higher brand distinctiveness"
  },
  {
    "decision": "Lexicon card + gold rail for result",
    "reason": "The analysis body is the product; needs a manuscript metaphor distinct from chrome",
    "alternatives": ["Same surface as inputs", "white paper card", "code block only"],
    "tradeoff": "Extra token (`--lex`); pays off in scanability"
  },
  {
    "decision": "Profile rail as segmented pills, More… as dark command menu",
    "reason": "Fixes the ugly light dropdown under dark chrome; profiles are modes not CTAs",
    "alternatives": ["Native select dropdown", "sidebar profile list", "keep white menu"],
    "tradeoff": "Custom menu needs correct z-index/focus management in implementation"
  },
  {
    "decision": "Settings as grouped cards (Provider / Credentials / Defaults)",
    "reason": "Screenshot form is one undifferentiated dump; cards encode task frequency",
    "alternatives": ["Single long form", "wizard", "JSON editor"],
    "tradeoff": "Slightly more vertical space; much faster scan"
  },
  {
    "decision": "Convention mode for Manager, restrained personality only on popup",
    "reason": "frontend-design: settings must be findable; expressive budget belongs to result",
    "alternatives": ["Full expressive everywhere", "fully mute utility"],
    "tradeoff": "Manager may feel 'quiet'; correct for rare-use config"
  },
  {
    "decision": "Segoe UI + Cascadia Code system stacks, no web font CDN",
    "reason": "Windows product; sandbox/offline mockups must not depend on Google Fonts",
    "alternatives": ["Inter via CDN", "IBM Plex", "custom brand font"],
    "tradeoff": "Less unique letterforms; zero FOIT and native OS feel"
  },
  {
    "decision": "Copy = only filled primary in popup footer",
    "reason": "Matches actual intent of the 90% surface; avoids U6 every-button-primary",
    "alternatives": ["All filled", "all ghost", "Pin primary"],
    "tradeoff": "Retry/Prompt need clear ghost styling so they remain findable"
  }
]
```

---

## Anti-slop self-check

**clean** — no gradient hero, no emoji bullets, no icon-card feature grid, no “seamless unlock” copy, no all-filled-button footer. Settings use convention-mode hierarchy; signature spend is limited to the lexicon card + profile rail.
