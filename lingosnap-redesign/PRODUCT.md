# PRODUCT.md — Selection Translate (LingoSnap)

## What it is

A Windows resident linguistic utility: the user selects (or hovers) text anywhere
and a floating popup delivers a structured bilingual (Chinese ↔ English) analysis
powered by an LLM provider. Tray-resident, ultralight (< 20 MiB idle), privacy-
careful (keys in Windows Credential Manager, no screenshots retained, clipboard
preserved).

## Surfaces and frequency

| Surface | Job | Frequency |
|---|---|---|
| Profile bar | Choose how to interpret the selection (Translate / Expert / Program / Concise / More…) | Every use |
| Translation popup | Deliver analysis you can trust at a glance: Selection (target + context) → structured Result → Copy | 90% of the time |
| Manager · Settings | Provider endpoint, model, credentials, default profiles | Rare setup |
| Manager · Prompts | Edit per-profile system prompt and user template | Occasional |
| Manager · History | Reopen past results without holding the DB open | Occasional |

## Audience and scene

People reading foreign docs, code comments, papers, and UI strings on Windows —
developers and scholars working at a desk, mid-flow inside an IDE, browser, or
PDF reader. They want speed and trust, not marketing polish. The popup floats
over whatever they were reading; it must never steal focus.

## Result content model (product truth)

The Expert/linguist profile returns markdown with these sections: Translation,
Words (lemma + IPA + senses), Idioms and Grammar, Error check, Other Forms,
Reasoning. Other profiles return subsets. Streaming deltas arrive while the
popup is open.

## Hard constraints

- Resident idle < 20 MiB private working set (hard product requirement).
- No Electron / resident WebView without approved architecture; the refined
  popup is a separate WebView2 companion process (see REFINED_POPUP_UI_PLAN.md).
- Never send a remote request without a valid target; context alone is not a target.
- Popup never steals focus; click-outside dismisses; pinned popups persist.
- System fonts only in shipping UI (no font CDN at runtime).

## Design mandate for this redesign

Two design tracks coexist in this folder and are maintained in parallel,
mirroring the classic/refined dual-skin product decision
(REFINED_POPUP_UI_PLAN.md):

- **`classic/`** — the incumbent dark Raycast/Linear-class mockup
  (`index.html`), its design system (`DESIGN.md`) and implementation guide
  (`DEVELOP_GUIDE.md`).
- **`fair-copy/`** — the new "Fair Copy" paper world (`fair-copy.html` master
  sheet, per-UI pages under `ui/`, direction contract in `SURFACE_BRIEF.md`).

For fair-copy, the classic look is the anti-reference: replace its world,
preserve product truth, structure, and copy. Neither track replaces the other.
