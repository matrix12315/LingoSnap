# Selection Translate — UI design tracks

Two UI design tracks live side by side in this folder and must continue to
exist at the same time — mirroring the tray-switchable dual-skin decision in
`REFINED_POPUP_UI_PLAN.md` (`popup_skin = "classic" | "refined"`).

## `classic/` — the incumbent dark UI

- `index.html` — the original redesign mockup (Raycast/Linear-class dark
  utility). Source of truth for its pixels.
- `DESIGN.md` — its design system (dark tokens, lexicon card, profile rail).
- `DEVELOP_GUIDE.md` — implementation guide for agents building the native
  Windows UI against `index.html`.

## `fair-copy/` — the new paper UI

- `ui/fair-copy.css` — **single source of truth for the design system** (tokens,
  cards, buttons, glass, manuscript, manager). One edit here updates the master
  sheet and every per-UI page; never duplicate these styles into a page.
- `fair-copy.html` — master sheet: every surface in one page for overview. It
  only links `ui/fair-copy.css` and `ui/liquid-lens.js`.
- `ui/` — one page per surface for full-size review: `chooser.html`,
  `popup.html`, `popup-streaming.html`, `popup-error.html`, `manager.html`
  (tabs via `#prompts` / `#history`). Each page links the shared CSS and adds
  only its own page-specific bits (caption, centering) in a tiny local
  `<style>`. `liquid-lens.js` generates the SVG displacement filter that gives
  the glass buttons their edge refraction.
- `SURFACE_BRIEF.md` — direction contract, candidate-world roll, and the log
  of applied user feedback.

## Shared

- `PRODUCT.md` — product truth for both tracks (surfaces, audience,
  constraints, result content model).

Open any HTML file directly in a browser; no build step, no network needed.
