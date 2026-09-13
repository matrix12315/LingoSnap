# Task 21 hand-off

Last updated: 2026-08-30, after the packaged forced-OCR gate.

## Current state

Task 21 is implemented and has passed the main source, package, manager, UIA Hover cascade, and
forced-OCR gates. The active Codex goal is **paused**, not complete, because the turn was
interrupted before the four-popup memory measurement and the final plan/evidence update.

No Selection Translate resident or manager process was running at hand-off time. No internet
access was used. Rust/Cargo/build output and temporary files stayed on `D:`.

The current portable package is:

`D:\pythonProject\2026\selectionTranslate\windows\dist\selection-translate-x64-20260830-r57`

Hashes:

- Resident: `1F72B778068BD0F67DB15CBB00EC659F9E2BF74D965333344FA00FAD67BF79B8`
- Manager: `8852B38E883AFCA38A79F9AE9A10B7C0788C1E44FCAAF719CA9747731826EBBF`

## Implemented behavior

### Popup registry and cascades

- Replaced singleton popup ownership with stable monotonic `PopupId` entries, bounded to four.
- Popup callbacks carry the popup ID in `LPARAM`; profile choice index remains in `WPARAM`.
- Provider deltas and completion are routed only to the active destination popup.
- Hover over **completed output only** creates a separate child popup; loading/input/buttons do not.
- Child-to-grandchild cascades work and retain distinct native HWNDs.
- Children are positioned beside the parent, preferring right and falling back left, then clamped.
- Normal reuse skips pinned popups; reanchoring no longer clears Pin.
- Capacity eviction is restricted to the oldest completed, unpinned entry that is neither the
  source nor active destination. An all-protected/all-pinned registry rejects the cascade locally.
- Child creation is staged. Existing popups are evicted only after native presentation and request
  admission succeed, so a failed child creation preserves the old surfaces and sends no request.
- Clicking outside closes unpinned popups. Clicking inside popup B retains B while closing other
  unpinned popups; pinned popups survive.
- Foreground-root lifetime handling remains active for Hover/cascade popups.
- Retry/Prompt now replay the exact admitted `TextContext` stored by their own popup. They do not
  re-extract an old coordinate after the source popup has closed.

Primary files:

- `windows/crates/platform-windows/src/app.rs`
- `windows/crates/platform-windows/src/popup.rs`

### Hover target cleanup

- Added a portable, idempotent Hover-only sanitizer.
- Removes zero-width formatting and boundary bullets/Markdown/quotes/brackets/punctuation.
- Preserves CJK, Unicode digits, decomposed combining marks, apostrophes, hyphens, underscores,
  paths, member access, namespace/arrows, `C++`, and `C#`.
- Rejects controls/newlines, punctuation-only, emoji-only, and ambiguous interior junk.
- Sanitizes each Hover extractor result before it can stop fallback, allowing OCR after bad UIA.
- Rechecks at `RequestGate`; empty/invalid/mismatched input makes zero provider requests.
- Context must contain the exact case-sensitive sanitized target and is reduced to its sentence.
- Selection and Manual retain their existing punctuation/multiline behavior.

Primary files:

- `crates/core/src/normalize.rs`
- `crates/core/src/request_gate.rs`
- `windows/crates/platform-windows/src/composition.rs`
- `crates/core/Cargo.toml`
- `Cargo.lock`

### Manager localization and UI

- Added optional `[ui] manager_language = "en" | "zh-CN"`; legacy configs default to English.
- Added complete typed English/Simplified Chinese catalogs for static and dynamic manager text.
- Language changes relabel live and persist atomically.
- Language-only save clones the last saved config and changes only `ui.manager_language`; unsaved
  provider/prompt edits are neither lost onscreen nor persisted.
- Language changes do not notify/restart the resident and never touch credentials.
- Settings, Prompts, and History remain exclusive DPI-aware page containers with no overlap.
- Updated the example configuration.

Primary files:

- `crates/core/src/config.rs`
- `crates/core/src/lib.rs`
- `windows/apps/manager/src/main.rs`
- `windows/config/config.example.toml`

## Verification already passed

### Source/package gates

- `cargo fmt --all -- --check`
- Full offline workspace tests: **230 passed**
  - core 53
  - platform-interface 2
  - platform-windows 125
  - provider-openai 22
  - storage 12
  - manager 13
  - resident 3
- Strict workspace Clippy with all targets and `-D warnings`
- Optimized MSVC release packaging through `windows/scripts/package-release.ps1`

### Packaged manager language E2E

Passed against r57:

- live Chinese title/navigation and visual page relabeling;
- `manager_language = "zh-CN"` persisted;
- an unsaved endpoint remained visible but was absent from saved config;
- no manager overlap/blank-page regression.

Latest passing artifacts:

`windows/tmp/manager-language-20260830232928621`

The initial packaged run reported two false negatives because the harness expected painted labels
to exist as child-window text. `windows/tmp/manager-language-e2e.ps1` was corrected to use the
accessible title/navigation plus screenshots; the production UI was already visually correct.

### Packaged UIA Hover cascade E2E

Passed against r57 with a loopback provider:

- 500 ms dwell and no early request;
- exact target plus full containing sentence;
- loading output is not Hover-active;
- three requests produced root, child, and grandchild with three distinct HWNDs;
- outside click closed unpinned children and retained the pinned source;
- Pin state, clipboard, foreground, history, blank silence, disabled silence, and foreground
  cancellation all passed.

Evidence:

`windows/tmp/hover-sta-run-20260830233105476`

### Packaged forced-OCR Hover E2E

Passed against r57:

- UIA failure followed by OCR success;
- exact target and joined sentence preserved;
- far-column text excluded;
- one request and one history row;
- popup loading/completion, blank silence, disabled silence, clipboard and foreground preservation;
- no captured image retained on disk.

Evidence:

`windows/tmp/hover-ocr-run-20260830233150608/summary.json`

## Remaining work before marking the goal complete

1. Perform the required **four-visible-popup** resident memory measurement with the manager closed.
   The private working set must remain below 20 MiB. Do not weaken the bound; reduce the cap if it
   fails.
2. Preferably extend the packaged cascade harness from three to four visible popups during the
   measurement, then leave all four alive long enough for stable sampling.
3. Run or add the remaining packaged edge checks if time permits:
   - source closure during pending child extraction sends no request;
   - all-pinned/protected capacity rejection sends no request;
   - Retry/Prompt on a child uses that child's stored target after its parent closes;
   - manager `en -> zh-CN -> en` persistence after reopen.
   The core ownership/cap behaviors already have unit/state coverage, but these packaged checks are
   still stronger evidence requested by Task 21.
4. Update `IMPLEMENTATION_PLAN.md`:
   - change Task 21 from “implementation in progress” to complete only after the memory gate;
   - record r57, the 230 tests, both E2E artifact paths, hashes, and measured memory.
5. Re-run formatting if any source or tracked test harness changes, then do a final source review.
6. Start the final packaged r57 resident for the user only after all gates pass.
7. Mark the active goal complete only when no required work remains.

## D:-only command environment

Run Cargo from `D:\pythonProject\2026\selectionTranslate\windows` with:

```powershell
$env:RUSTUP_HOME = 'D:\DevTools\rustup'
$env:CARGO_HOME = 'D:\DevTools\cargo'
$env:CARGO_TARGET_DIR = 'D:\pythonProject\2026\selectionTranslate\windows\target'
$env:TEMP = 'D:\pythonProject\2026\selectionTranslate\windows\tmp'
$env:TMP = 'D:\pythonProject\2026\selectionTranslate\windows\tmp'
$env:CARGO_NET_OFFLINE = 'true'
```

MSVC environment:

`D:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat -arch=x64`

Cargo executable:

`D:\DevTools\cargo\bin\cargo.exe`

Do not install or place build/temp artifacts on `C:`. Do not use internet access. Do not delete
old packages or test artifacts without explicit user approval.

## Subagent review record

- Sanitizer audit: completed; fixed exact case-sensitive context matching; core 53/53 and strict
  core Clippy passed.
- Popup audit: completed; found Pin/reuse/eviction and Retry ownership issues; root integrated the
  architectural transaction, snapshot replay, and child placement fixes.
- Manager audit: completed; manager 13/13, strict manager Clippy, packaged-style GUI E2E, and visual
  inspection passed.

