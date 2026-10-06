# Selection Translate — Task-by-Task Coding Plan

## 0. Plan contract

Implement tasks in order. Do not start a task until its dependency and exit gate are complete. Each task must end by running its listed verification. If a gate fails, stop, report the evidence, and do not hide the failure with a workaround.

Current repository state (2026-08-30):

- Tasks 1–2 are complete; the native Windows workspace and D:-only generated-file layout exist.
- Tasks 3–11 are implemented in the worktree. Selection, UIA Hover, forced-OCR Hover, and the packaged manager visual/memory gates pass; the remaining broader Task 12 gates are Manual and the application matrix.
- SQLite schema version 1 was explicitly approved by the user on 2026-08-19 and is implemented in `crates/storage` with a resident background writer and on-demand manager History view.
- Task 13 is implemented in source and covered by automated tests; its packaged interactive exit gate remains open.
- The r56 candidate completes Tasks 19–20: Hover waits 500 ms, keeps an admitted result across pointer movement, cancels pending work and dismisses an unpinned result on a foreground-root change, uses UIA-first/OCR-fallback exact word plus sentence extraction, and supports completed text in its own popup without translating transient output or losing Pin. The manager is now a dark, DPI-aware native left-rail interface with three exclusive pages. Packaged UIA/OCR E2E, 210 workspace tests, strict Clippy, active-250%-DPI visual inspection, and the sub-20-MiB resident gate pass.
- Task 21 is approved and in implementation. It restores outside-click dismissal, replaces self-popup result replacement with a bounded stable-ID popup cascade, adds Hover-only Unicode token cleanup with zero-request rejection, and adds persistent English/Simplified Chinese manager localization.
- The r21 candidate adds a universal concise-Markdown response contract after the no-text/request gate and renders completed Markdown through the native system `RICHEDIT50W` control. Streaming remains immediate plain text, Copy preserves the raw Markdown, and closing the popup unloads `msftedit.dll`. The packaging gate passes formatting, 166 workspace tests (including a real hidden RichEdit formatting test), strict Clippy, and optimized MSVC builds. The final pointer-driven r21 check was cancelled before starting because its confirmation dialog was not acknowledged; it is not counted as passed.
- The r22 candidate disables Qwen thinking mode on the wire with `enable_thinking: false`, selected only when the configured model name begins with `qwen`; other OpenAI-compatible models receive no nonstandard field. Historical runtime traces showed provider-start-to-first-delta latency of roughly 5.9–18.8 seconds with completion usually following almost immediately, identifying provider-side thinking as the delay rather than extraction or popup rendering. The r22 packaging gate passes formatting, 167 workspace tests, strict Clippy, and optimized MSVC builds. Live configured-provider verification then observed first visible deltas at 0.645–1.043 seconds, 6–16 popup delta updates per response, and completion at 0.994–1.157 seconds; evidence is `windows/tmp/selection-r22-thinking-off-trace.log`.
- The r23 candidate automatically dismisses an unpinned result popup when the existing global mouse hook observes a left-button press outside the resident process. Clicking popup controls does not dismiss it, Pin overrides automatic dismissal, and outside-click dismissal cancels any unfinished provider stream. This preserves the popup's required `WS_EX_NOACTIVATE` behavior instead of relying on keyboard focus that the popup intentionally never owns. The r23 packaging gate passes formatting, 168 workspace tests, strict Clippy, and optimized MSVC builds.
- The r24 candidate added a fixed, labeled source pane above the independent streamed-result pane. The current display contract supersedes its original target-only behavior: the pane shows the admitted containing sentence when available and falls back to the normalized target only when sentence retrieval fails. It remains selectable and bounded to 4,096 characters, and never exposes prompts, provider configuration, or credentials.
- The r25 candidate removes extraction-source metadata from the three default LLM user templates and explicitly directs the model to answer only Target/Word/Subject while using Context solely for disambiguation. This prevents `Source: UiaSelection` from being echoed as the translation. `{source}` remains supported for intentional custom profiles. The active local templates were updated without touching credential storage, and restarting r25 cleared stale in-memory cached outputs. The r25 packaging gate passes formatting, 169 workspace tests, strict Clippy, and optimized MSVC builds.
- The r26 candidate enforces a central context invariant in RequestGate: optional context is admitted only when it contains the normalized target, then reduced to the single sentence containing that target. Unrelated or over-expanded context is dropped before prompt rendering, caching, provider access, and history persistence. The selected target itself is unchanged. The r26 packaging gate passes formatting, 170 workspace tests, strict Clippy, and optimized MSVC builds.
- The r27 candidate adds three built-in, switchable profiles—`linguist-analysis`, `code-specialist`, and `concise-explanation`—to both product defaults and the active local configuration. User-supplied `{selection}` placeholders were mapped to the supported `{target}` contract; prompts use the admitted one-sentence `{context}` only for disambiguation. Existing selection/hover defaults and credential storage are unchanged. The r27 packaging gate passes formatting, 170 workspace tests, strict Clippy, and optimized MSVC builds.
- The r28 candidate implements Task 14: a valid Selection opens a native chooser containing only configured profile names, and a provider request becomes possible only after one name is clicked. Dismissal retains no pending target and sends no new request. Hover and Manual remain direct. The packaging gate passes formatting, 172 workspace tests, strict Clippy, and optimized MSVC builds. The packaged interactive chooser check remains for the user.
- The r29 candidate changes that chooser to one compact horizontal row centered eight logical pixels above the pointer. It is 52 logical pixels high, sizes itself from the longest profile name, compresses to the monitor work area without wrapping, and restores the normal result-popup geometry after a choice. The packaging gate again passes formatting, 172 workspace tests, strict Clippy, and optimized MSVC builds.
- The r30 candidate adds session-only Rest mode through the tray menu. Rest starts disabled; enabling it cancels active work, closes the popup, clears stale mouse state, and blocks Selection, Hover, Manual, profile-cycle, Retry, Prompt, and chooser actions while leaving Manager/config refresh and Exit available. The packaging gate passes formatting, 173 workspace tests, strict Clippy, and optimized MSVC builds.
- The r31 candidate bounds the chooser to the first three one-word profile labels plus `More…`. The folded native menu contains one-word labels for all remaining profiles and maps each choice back to its unchanged configured profile ID. With the active configuration the inline labels are `Contextual`, `Word`, and `Wiki`; overflow contains `Expert`, `Program`, and `Concise`. The packaging gate passes formatting, 174 workspace tests, strict Clippy, and optimized MSVC builds.
- The r32 candidate independently sizes every inline chooser button to its own label instead of distributing equal longest-label widths. At 100% scaling the button widths are approximately 100/52/52/60 pixels, with a 38-pixel strip height, four-pixel margins, and four-pixel gaps; the natural total width is about 284 pixels. The packaging gate passes formatting, 174 workspace tests, strict Clippy, and optimized MSVC builds.
- The r33 candidate guarantees best-effort sentence context after first-valid-target extraction. UIA and native Edit paths keep their existing sentence derivation; when Selection succeeds without a sentence and has selection geometry, bounded Windows OCR runs once as context enrichment. It may attach only a sentence containing the exact selected target and cannot replace the target or its extraction source. The packaging gate passes formatting, 176 workspace tests, strict Clippy, and optimized MSVC builds.
- The r34 candidate applies the sentence-context contract to the popup's fixed Input pane: when RequestGate admits a containing sentence, the pane displays that complete sentence; only a genuine context-extraction failure falls back to the selected word. The selected word remains the unchanged `{target}` used by prompt profiles. The packaging gate passes formatting, 177 workspace tests, strict Clippy, and optimized MSVC builds; packaged resident SHA-256 is `8A7522BCCA77B0C9E432CD8EF020E0E0F64396E98BBD79C267ED104BB6B9337D`.
- The r35 candidate always places the user's three profiles—`linguist-analysis`, `code-specialist`, and `concise-explanation`—in the chooser's three inline positions when present. Their compact labels are `Expert`, `Program`, and `Concise`; all other configured profiles remain under `More…` in configuration order. The ordering is reapplied during live configuration refresh so button indexes and profile IDs remain synchronized. The packaging gate passes formatting, 179 workspace tests, strict Clippy, and optimized MSVC builds; packaged resident SHA-256 is `5C8F1642006868AB267C017A74A69077D9C9EA736C2736F5FE6BEB5A92730EBE`.
- The r36 candidate keeps RichEdit redraw disabled while synchronizing each response delta, resetting the caret, and restoring the reader's first visible line; only the final stable state is painted. Streaming remains raw Markdown and completed output still receives Markdown formatting, but neither phase selects the response end or forces a scroll to the bottom. The packaging gate passes formatting, 180 workspace tests (including a native RichEdit first-line regression), strict Clippy, and optimized MSVC builds; packaged resident SHA-256 is `37AAC7FB0D1A1931754C561512207030F2E403D489D0D2FEB155618351D4ADA3`.
- The r37 candidate implements the unified dark popup surface. The parent, borderless input, RichEdit/fallback output, permanent actions, and inline profile choices share one dark palette; buttons use native owner drawing and the RichEdit background is configured before presentation. One process-lifetime class brush prevents per-paint GDI leaks, and input/result foreground colors remain distinct. The packaging gate passes formatting, 181 workspace tests, strict Clippy, and optimized MSVC builds; packaged resident SHA-256 is `98D1BA5CD6AB1217279631E59B23A9BEFCCF7A1B6309C20CC50206B793E4F124`. The packaged visual check remains pending.
- The r38 candidate fixes r37's RichEdit foreground regression by applying `CFM_COLOR` with the popup's light foreground across every synchronized streaming/completed response before Markdown-specific spans. A native RichEdit test verifies the exact character color alongside bold formatting. The packaging gate passes formatting, 181 workspace tests, strict Clippy, and optimized MSVC builds; packaged resident SHA-256 is `E54DDBEB530F473707510C1E9469A4546B94B2C94380DE0255180079F4DE8D38`.
- The r39 candidate renders the accumulated Markdown projection synchronously for every streaming delta while retaining raw Markdown for Copy/cache/history. It also adds a DPI-scaled native top drag band, preserves the dragged anchor, uses the suggested cross-monitor DPI rectangle, and clamps the final position to the monitor work area. Loading and local errors remain literal. The packaging gate passes formatting, 183 workspace tests, strict Clippy, and optimized MSVC builds; packaged resident SHA-256 is `3E0CEFBDE78104E6418E3E7A2695B00A5BAD16202D16D92E2AB1D469AE489299`.
- The r40 candidate fixes r39's unreliable caption hit-test drag path. The expanded popup now reserves a 24-logical-pixel empty top band and explicitly starts the native move loop from client mouse-down; the chooser and all text/action controls are excluded. Routing is verified at 100%, 150%, and 200% DPI. The packaging gate passes formatting, 183 workspace tests, strict Clippy, and optimized MSVC builds; packaged resident SHA-256 is `AB154A4AA7FE3D635B207895E31A34880AD17FF2BE8E3B760CDB440A75F5127B`.
- The r41 candidate removes streaming-induced drag lag by continuing to accumulate the latest raw provider output while the native move loop is active, but deferring the expensive full RichEdit Markdown projection until `WM_EXITSIZEMOVE`. Loading, streaming, completion, local errors, and cached text all use the same render gate, and the final state is flushed exactly once after the mouse is released. Popup state is snapshotted before synchronous Win32 control updates to avoid reentrant mutable access. The packaging gate passes formatting, 185 workspace tests, strict Clippy, and optimized MSVC builds; packaged resident SHA-256 is `1D487EE6C1C6AC958A2F2B0BE70D79069ACF462979F6B596F7EC455387480243`.
- The r42 candidate fixes global pointer starvation during live Markdown rendering. Provider deltas update the lossless raw state immediately but coalesce full Markdown/RichEdit projection behind one 40-millisecond one-shot timer; repeated output-buffer length scans and forced synchronous `UpdateWindow` painting are removed. The `WH_MOUSE_LL` callback now runs on a dedicated message-loop thread, posts only bounded pointer metadata to the resident, and cannot be blocked by popup rendering on the UI thread. Startup acknowledgement, shutdown, timer/move races, stale timer messages, and terminal-state preservation are handled explicitly. The packaging gate passes formatting, 186 workspace tests, strict Clippy, and optimized MSVC builds; packaged resident SHA-256 is `97FB2B758AB4231639AD5336EB6DDBBEF575A0988447FE7EBDCEE92CA7C11580`.
- The r43 candidate removes the duplicated `Translation` label from the linguist profile at its source. The built-in, packaged example, and active local profile now require exactly four ordered level-2 Markdown headings—`Translation`, `Idioms and Grammar`, `Other Forms`, and `Reasoning`—each exactly once; `Translation:` fields, alternate headings, introductions, conclusions, and repeated headings are forbidden, and inapplicable fields use `None`. The output cap is reduced from 900 to 700 tokens. The packaging gate passes formatting, 187 workspace tests, strict Clippy, and optimized MSVC builds; packaged resident SHA-256 is `00A477A8C97D8E3AE20CE74C8FBABBF6BACD5D5ADAC328BE4BBAEA3B4D654911`.
- The r44 candidate completes the source-level Hover/OCR correctness pass. Significant movement, clicks, and disabling Hover invalidate pending/active Hover-only work; UIA now preserves the exact pointed target and target rectangle even when sentence derivation fails; one bounded OCR enrichment may attach only context for the same unchanged target. OCR uses exact pointer containment, actual target geometry, transitive wrapped-line grouping with far-column exclusion, the runtime `OcrEngine::MaxImageDimension`, bounded capture allocations, and a contract-correct deselected GDI bitmap before `GetDIBits`. Screenshots remain memory-only. The packaging gate passes formatting, 196 workspace tests, strict Clippy, and optimized MSVC builds; packaged resident SHA-256 is `D746B069B6691B2D74DA22ABBF4D701306492090E36F3B3FADDFF63C7410DF59`. Packaged Hover/UIA and forced-OCR interactive gates remain pending.
- The r45 candidate makes the linguist response schema deterministic instead of relying only on model compliance. At terminal completion, only `linguist-analysis` is normalized to exactly four ordered headings—`Translation`, `Idioms and Grammar`, `Other Forms`, and `Reasoning`. Duplicate canonical headings are removed, preamble is retained under `Translation`, and empty or missing sections receive `None`; other profiles remain byte-for-byte unchanged. The normalized value is used consistently by the popup, cache, and history, including legacy cache hits. The packaging gate passes formatting, 200 workspace tests, strict Clippy, and optimized MSVC builds; packaged resident SHA-256 is `22F3013F4DEF63FC3AC73F37EFCC2FFEC861694A92E1BCD81B36C3C493BCB34A`.
- The r46 candidate closes a remaining model-deviation case in r45: heading-only variants such as `# Translation`, `### Translation:`, and `**Translation**` are canonicalized, while forbidden value-bearing labels such as `Translation: 翻译` and `- Translation: 翻译` lose only the redundant label and retain their value. Ordinary prose containing those words is preserved. The packaging gate passes formatting, 202 workspace tests, strict Clippy, and optimized MSVC builds; packaged resident SHA-256 is `48243E342EA8E9CE5D94CE83431B8E3DC743C2BD3D665D3F23E67A3D512C81CF`.
- The r51 candidate closes the packaged Hover/OCR gate. Hover now starts at the exact pointed UIA element, verifies the captured native `GA_ROOT`, proves raw-view membership in the captured UIA root through a bounded walk, and searches at most 16 raw ancestors for the nearest `TextPattern` provider. Exact word-box containment and a second native-root check prevent snapped/out-of-window text admission. The full package gate passes formatting, 202 workspace tests, strict Clippy, and optimized MSVC builds; packaged resident SHA-256 is `FF95CD83985A06A445BDB5293C84669ED1FFDC6CB0A00253734CA7BA61A0DE25`. Packaged UIA Hover evidence is `windows/tmp/hover-sta-run-20260830054221269`: one `UiaPoint` request contained exact target `hovered` plus its full sentence, the popup loaded/completed, OCR was not used, jitter/blank/disabled probes sent no request, and foreground/clipboard/history checks passed. Packaged forced-OCR evidence is `windows/tmp/hover-ocr-run-20260830054255338/summary.json`: UIA failure then OCR success, exact target and joined sentence, far-column exclusion, no retained image, one request/history row, popup completion, blank/disabled silence, and foreground/clipboard preservation all passed. A fresh r51 Hover-off idle run produced 59 samples at 0% average CPU, 1.579 MiB average and 1.625 MiB peak private working set (`windows/tmp/memory-selection-translate-resident.csv`); the stricter same-process post-success five-minute gate remains open.
- The r54 candidate completes the refined native popup task. The result window now uses a rounded dark native surface, cool-blue state accent, DPI-scaled Segoe UI hierarchy, unclipped INPUT/RESULT labels, a dedicated sentence-context area, rounded owner-drawn action pills, and explicit RichEdit font/Markdown formatting while retaining its no-activate, drag, streaming, selection, and dismissal behavior. The deterministic packaged Selection/loopback test passed and its inspected screenshot is `windows/tmp/markdown-popup-run-20260830104915795/popup-visual.png`. The package gate passed formatting, 202 workspace tests, strict Clippy, and optimized MSVC builds. The packaged resident is 1,778,688 bytes with SHA-256 `F9C5814CE63C16810786DCDA9E36BC977224217275712617081291891C6BAF86`. A fresh Hover-off idle run produced 59 samples at 0% average CPU, 2.353 MiB average and 2.375 MiB peak private working set (`windows/tmp/memory-popup-r54.csv`).
- The redesign candidate realizes the approved `selection-translate-redesign` HTML mockup (Raycast/Linear-class dark utility) across both surfaces. The popup adopts the mockup tokens verbatim — surface `#12171F` chrome, raised `#1A2230` Selection/Result cards with hairline `#2C3646` borders, ink `#F0F4FA`, muted `#8B97A8` eyebrows, accent `#8BACFF` mark/primary action, and gold `#D4B56A` mono inline code at 12px — with a 440-logical-pixel frame, integrated Selection card (mono target over muted context), header pin/close chrome icons duplicating the footer actions, a full-header drag band, and left-aligned per-label footer widths. The profile chooser becomes the mockup's standalone pill rail (fully rounded strip, default profile highlighted in accent, More… opening a dark owner-drawn command menu instead of a native light dropdown), and `show_profile_choices` now receives the default-profile index to highlight. The manager replaces its left rail with the mockup's horizontal tab bar (accent-tinted active tab, ok-green live status dot at the right), groups Settings into Provider/Credentials/Defaults cards, moves the Prompts page to the mockup's meta-row-plus-side-by-side-editors layout (ID becomes an editable dropdown that loads the selected profile, replacing Previous/Next; the duplicated global-defaults fields move to Settings only), and rebuilds History as a Search card over an Entries split whose list uses owner-drawn three-line rows (target, muted preview, `HH:MM · profile` with the profile in accent) beside void Selection/Output wells. Workspace gate: formatting, 249 tests, strict Clippy, optimized MSVC builds; inspected captures are `windows/tmp/popup_preview.png` and `windows/tmp/manager_{settings,prompts,history}.png`.
- The IELTS candidate adds a seventh built-in prompt profile, `ielts-writing-instructor` (`IELTS writing instructor`), to product defaults, the packaged example configuration, and the active local configuration. The profile treats the admitted target as one sentence from a candidate's IELTS Writing response, refines it into a natural, accurate, band-7+ academic sentence while fully preserving its original meaning, and returns exactly two ordered level-2 headings—`Refined Sentence` and `Refining Reasons`—with one item per change (original wording, refined wording, reason). Selection/Hover defaults, the chooser rail layout, and the response normalizer are unchanged, so the profile is reached through `More…`, Ctrl+Alt+P cycling, or the manager defaults. The packaging gate passes formatting, 254 workspace tests, strict Clippy, and optimized MSVC builds.
- The IELTS follow-up fixes a `None`-refinement deviation observed on the first live use: the prompt's trailing `When nothing was changed, write None` line was applied by the model to both sections, so an already-correct input returned `None` under `Refined Sentence`. The prompt now forbids `None` in `Refined Sentence`, requires an already-correct input to be echoed unchanged, and scopes `None` to `Refining Reasons` only when the refined sentence is identical to the input; the schema test pins that phrasing. The live local configuration had also drifted (profile id renamed to a display-string and `{target}{context}` jammed into one template line); it was restored to the canonical id and template. The packaging gate passes formatting, 254 workspace tests, strict Clippy, and optimized MSVC builds.
- A second live `None` with the scoped prompt (history evidence: fresh request, canonical prompt id, `served_from_cache = 0`) showed the model applied the None escape hatch globally and turned an instruction-like selection into meta-commentary. The prompt was redesigned to remove the word `None` entirely, require the bare refined sentence with no commentary, refine non-IELTS and already-correct input unchanged, and give an affirmative no-change explanation; the schema test now forbids `None` anywhere in the prompt. Output was verified against the configured live provider before packaging. The packaging gate passes formatting, 254 workspace tests, strict Clippy, and optimized MSVC builds.
- The r13 source passes offline workspace formatting, all workspace tests, strict Clippy, and optimized release builds. Its Selection path is verified through extraction, one mock-provider request, loading/completed popup, clipboard/foreground preservation, and one history row.
- The current Selection-only candidate (resident SHA-256 `C6D62CF0C8DB5783B7DB2B7D93922024C99DCE6E1DDFF1C1BD44E4B073A9B642`) passes 85 platform tests, strict platform Clippy, and an optimized resident build. An automated Chrome drag-selection passed UIA extraction, valid-text/request admission, and popup creation. A subsequent real user selection passed extraction, the configured provider completed, and the live popup was externally verified visible, topmost, non-activating, and responsive. Evidence: `windows/tmp/chrome-selection-auto-20260826141823919/runtime-trace.log` and `windows/tmp/selection-final-live-trace.log`.
- Rust/Cargo, Visual Studio Community 2026, and the Windows SDK are installed, but Cargo and `cl.exe` are not available on a normal PowerShell PATH. Use the explicit D: Rust tool paths and an x64 `VsDevCmd.bat` environment for verification and builds.
- No public release, signing, deployment, or Git history mutation is part of this plan.

## 1. Fixed product decisions

- Target Windows 10 22H2 and Windows 11 x64 for v0.1.
- Use native Rust/Win32. Do not use Electron, WebView2, `tokio`, `reqwest`, a bundled OCR model, or a permanent polling loop.
- Resident acceptance target: below 20 MiB Task Manager private working set after one warm-up request and 60 seconds idle, with Hover Mode off, popup closed, SQLite closed, and manager process stopped.
- Standard operation always includes mouse-selection and manual-hotkey triggers.
- Hover Mode is manually enabled per session and always starts disabled.
- Windows system OCR is enabled as the last fallback; it is not a separate mode.
- Stop extraction as soon as one extractor returns valid target text.
- Hover accepts only a word box that contains the pointer; blank space never snaps to a nearby
  word. If UI Automation finds a valid pointed word without sentence context, one bounded OCR
  enrichment may attach only a sentence containing that unchanged word and may not replace it.
- Never send a remote request when target text is absent, invalid, oversized, stale, or cancelled.
- LLM behavior is controlled by prompt profiles. There is no independent language selector.
- Store the latest 1,000 completed results. The History/Prompts/Settings UI runs only on demand.
- macOS is a later adapter implementation. Do not create macOS or Android code in v0.1.

## 2. Required repository structure

Task 2 creates this structure:

```text
selectionTranslate/
├─ AGENTS.md
├─ IMPLEMENTATION_PLAN.md
├─ Cargo.toml
├─ Cargo.lock
├─ rust-toolchain.toml
├─ crates/
│  ├─ core/
│  ├─ platform-interface/
│  ├─ provider-openai/
│  └─ storage/
├─ windows/
│  ├─ crates/
│  │  └─ platform-windows/
│  ├─ apps/
│  │  ├─ resident/
│  │  └─ manager/
│  ├─ scripts/
│  │  └─ measure-memory.ps1
│  ├─ target/             # Generated; not source.
│  └─ tmp/                # Windows-only scratch/downloads.
└─ android/                 # Reserved; leave empty.
```

Portable crates remain at the repository root so a future macOS adapter can reuse them without depending on a Windows-named directory. Installed Rust, MSVC, Visual Studio, and the Windows SDK are external development prerequisites and are not copied into the repository.

Package names:

- `selection-core`
- `selection-platform-interface`
- `selection-platform-windows`
- `selection-provider-openai`
- `selection-storage`
- `selection-translate-resident`
- `selection-translate-manager`

Dependency direction:

```text
core <- platform-interface <- platform-windows
core <- provider-openai
core <- storage
resident -> core + platform-interface + platform-windows + provider-openai + storage
manager  -> core + platform-windows + storage
```

No shared crate may depend on `platform-windows`.

## 3. Coding tasks

### Task 1 — Toolchain verification (complete)

**Goal:** make the Windows Rust build possible without violating approval or internet rules.

Verified installation layout: target `x86_64-pc-windows-msvc`; `RUSTUP_HOME=D:\DevTools\rustup`; `CARGO_HOME=D:\DevTools\cargo`; Visual Studio Community 2026 `18.9.1` at `D:\Program Files\Microsoft Visual Studio\18\Community`; MSVC tools `14.51.36231` and compiler `19.51.36256`; Windows SDK `10.0.26100.0` at `D:\Windows Kits\10`. Windows task downloads and temporary files belong under `D:\pythonProject\2026\selectionTranslate\windows\tmp`. Small unavoidable Microsoft installer/registry components may remain on C:, but Rust and the main Visual Studio workload remain on D:.

Normal PowerShell PATH does not contain Cargo or `cl.exe`. Initialize the x64 MSVC environment and call the D: Rust tools explicitly:

```powershell
cmd /c 'call "D:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 && "D:\DevTools\cargo\bin\rustc.exe" --version && "D:\DevTools\cargo\bin\cargo.exe" --version && "D:\DevTools\cargo\bin\rustup.exe" show active-toolchain && cl.exe 2>&1'
```

For Cargo commands in later tasks, use `D:\DevTools\cargo\bin\cargo.exe` from the same `VsDevCmd.bat -arch=x64` shell. Verify the SDK and installed MSVC versions from the Visual Studio environment before building.

Exit gate:

- Task 1 is complete: explicit D: `rustc`, `cargo`, and `rustup` work; the active toolchain has the `x86_64-pc-windows-msvc` target; `VsDevCmd.bat` initializes x64 `cl.exe`; MSVC and Windows SDK versions are present.

### Task 2 — Bootstrap the workspace (complete)

**Files:** root `Cargo.toml`, `rust-toolchain.toml`, crate manifests, minimal `lib.rs` and `main.rs` files, then `AGENTS.md`.

Actions:

1. Update `AGENTS.md` first with the concrete verification commands above.
2. Create the workspace and packages from Section 2.
3. Pin stable Rust in `rust-toolchain.toml` with the MSVC x64 target.
4. Set release defaults: `lto = true`, `codegen-units = 1`, `opt-level = "z"`, `panic = "abort"`, and stripped symbols.
5. Add only required dependencies. Use feature-scoped `windows`, `serde`, `serde_json`, `toml`, and `rusqlite` with bundled SQLite. Do not add an async runtime.
6. Generate and retain `Cargo.lock`.

Verification:

```powershell
cmd /c 'call "D:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 && "D:\DevTools\cargo\bin\cargo.exe" fmt --all -- --check'
cmd /c 'call "D:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 && "D:\DevTools\cargo\bin\cargo.exe" check --workspace'
cmd /c 'call "D:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 && "D:\DevTools\cargo\bin\cargo.exe" test --workspace'
```

Exit gate: all packages compile as empty shells and dependency direction matches Section 2.

Completed with the Rust/Cargo homes and dependency cache on `D:`. The workspace resolves and passes formatting, check, test, and Clippy verification with the explicit D: toolchain. No application feature code is counted as part of this task.

### Task 3 — Build the native feasibility shell (implemented; final manual verification pending)

**Files:** `windows/crates/platform-windows/src/app.rs`, `tray.rs`, `hotkey.rs`, `popup.rs`; `windows/apps/resident/src/main.rs`; `windows/scripts/measure-memory.ps1`.

Actions:

1. Initialize COM and WinRT on the correct threads.
2. Create a hidden message-only Win32 window and event-driven message loop.
3. Add a notification-area icon with Open Manager, Toggle Hover, and Exit commands.
4. Register fixed `Ctrl+Alt+T` and `Ctrl+Alt+H` plus the validated `hotkeys.cycle_profiles` chord with `RegisterHotKey`; report conflicts locally without crashing.
5. Create a borderless, non-activating popup containing static sample text; clamp it to the current monitor and destroy it on dismissal.
6. Implement graceful cleanup of hotkeys, icon, windows, COM, and WinRT.
7. Implement `measure-memory.ps1` to record process private working set and average CPU every second, without changing system configuration.

Verification:

```powershell
cmd /c 'call "D:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 && "D:\DevTools\cargo\bin\cargo.exe" test -p selection-platform-windows'
cmd /c 'call "D:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 && "D:\DevTools\cargo\bin\cargo.exe" build --release -p selection-translate-resident'
powershell -ExecutionPolicy Bypass -File .\windows\scripts\measure-memory.ps1 -ProcessName selection-translate-resident -DurationSeconds 300
```

Exit gate:

- Tray, hotkeys, popup, and exit lifecycle work.
- Warmed idle private working set is below 20 MiB and average CPU is at most 0.1%.
- If the shell alone fails either budget after profiling, stop before building product features.

### Task 4 — Define core contracts and the no-request gate (complete)

**Files:** `crates/core/src/{text,job,normalize,request_gate,cache}.rs`; `crates/platform-interface/src/lib.rs`.

Required public types:

```rust
enum TriggerKind { Selection, Manual, Hover }
enum ExtractionSource { UiaSelection, UiaPoint, Clipboard, Ocr }

struct TextContext {
    target: String,
    context: Option<String>,
    source: ExtractionSource,
    screen_rect: Option<ScreenRect>,
}

struct JobInput {
    id: u64,
    trigger: TriggerKind,
    text: TextContext,
    prompt_id: String,
}

struct PreparedRequest {
    job_id: u64,
    target: String,
    context: Option<String>,
    prompt_id: String,
}
```

Required interfaces:

- `TextExtractor::extract(trigger, pointer, selection_rect) -> ExtractionResult`
- `TranslationProvider::stream(prepared, cancellation, sink) -> ProviderResult`
- `HistoryStore::insert_completed(entry)`
- `PopupSink::show_loading/update/finish/show_local_error/dismiss`

Request gate rules:

1. Trim Unicode whitespace.
2. Remove U+200B, U+2060, and U+FEFF.
3. Reject empty targets and targets over 4,000 Unicode scalar values.
4. Reject cancelled or stale job IDs.
5. Reject missing/invalid prompts or provider configuration.
6. Construct `PreparedRequest` only after every check succeeds.

Verification:

- Unit-test missing, empty, whitespace-only, zero-width-only, context-only, oversized, cancelled, and stale inputs.
- Use a fake provider counter and prove all rejected inputs result in exactly zero provider calls.
- Run `D:\DevTools\cargo\bin\cargo.exe test -p selection-core -p selection-platform-interface` from the x64 `VsDevCmd.bat` environment.

Exit gate: no code outside the request gate can construct `PreparedRequest`.

Completed with private `PreparedRequest` construction, Unicode/zero-width normalization, stale and cancellation checks, prompt/provider admission checks, portable platform traits, and tests proving rejected jobs cannot reach a provider call.

### Task 5 — Implement input triggers and job priority (implemented; final manual verification pending)

**Files:** `windows/crates/platform-windows/src/mouse.rs`; `crates/core/src/coordinator.rs`; resident wiring.

Actions:

1. Install `WH_MOUSE_LL`; record left-button down/up coordinates and mouse movement events.
2. On every left-button release, wait 80 ms using a one-shot window timer, then emit a Selection trigger. This covers drag and double-click selection without polling.
3. Record the drag rectangle for OCR, expanding zero-size double-click rectangles around the pointer.
4. When Hover Mode is enabled, reset a 500 ms one-shot timer on movement beyond 4 px and emit Hover only when it expires.
5. Start Hover disabled and do not persist its enabled state.
6. Implement priority `manual > selection > hover`. A new higher-priority candidate cancels
   lower-priority extraction immediately. Replacement of an already presented/provider job is
   transactional: cancel it only after the new candidate has valid text, passes admission, and has
   a result surface, so an empty click cannot destroy a valid result.
7. Fingerprint successful text using process ID, normalized target, context, and source rectangle; suppress an unchanged automatic result for 10 minutes.
8. Significant pointer/process/root movement, a new click, or disabling Hover invalidates pending
   Hover extraction immediately. Disabling Hover also cancels an unfinished Hover provider/popup
   without cancelling Selection or Manual work.

Verification:

- Unit-test priority, cancellation, timer reset, duplicate suppression, and stale completion handling.
- Manually verify single click, drag selection, double click, rapid pointer movement, and hotkey conflict behavior.

Exit gate: triggers create local jobs only; no network code is connected yet.

### Task 6 — Implement UI Automation extraction (implemented; packaged WPF Hover verified, application matrix pending)

**Files:** `windows/crates/platform-windows/src/uia/{mod,selection,point}.rs`; local sentence segmentation in `crates/core/src/sentence.rs`.

Actions:

1. Create `CUIAutomation8` on a dedicated COM MTA worker.
2. For Selection and Manual, inspect the focused/foreground element, request `TextPattern` or `TextPattern2`, and call `GetSelection`.
3. Accept only non-empty selected ranges associated with the active control.
4. For Hover, start at `ElementFromPoint` and search a bounded nearest-ancestor chain for the
   first element exposing `TextPattern`; composite WPF/Chromium descendants commonly expose the
   pattern on their editor ancestor. Use that provider's `RangeFromPoint`, duplicate the range,
   and expand one copy to `TextUnit_Word`. Revalidate the native `GA_ROOT` under the unchanged
   pointer before accepting the range, and never accept content outside the captured root/process
   boundary.
5. UI Automation has no Sentence text unit. Expand another copy to Paragraph, falling back to Line, cap retrieved text, then run local sentence-boundary detection around the word.
6. Return structured failure reasons—unsupported pattern, empty range, permission denial, stale element—so the coordinator can continue to the next extractor.

Verification:

- Unit-test sentence boundaries for English, Chinese punctuation, abbreviations, quotes, and missing punctuation.
- Manually test Notepad, Chromium, Firefox, Office, VS Code, a terminal, and a PDF reader.
- Confirm failed UIA extraction produces no provider call and advances to fallback.

Exit gate: selected text and hover word/context work in at least Notepad plus one Chromium browser; unsupported applications fail safely.

### Task 7 — Implement clipboard and OCR fallbacks (implemented; packaged forced-OCR Hover verified, manual clipboard/application matrix pending)

**Files:** `windows/crates/platform-windows/src/native_selection.rs`, `clipboard.rs`, `capture.rs`, and `ocr.rs`.

Clipboard actions:

1. Use a dedicated OLE STA worker.
2. Snapshot each supported HGLOBAL-backed clipboard format into independent bounded memory before invoking Copy; do not retain the live clipboard `IDataObject`.
3. Capture both the source process and its `GA_ROOT` top-level window at trigger time.
   Bind each automatic Copy attempt to that root window. Immediately before `SendInput`,
   require the foreground `GA_ROOT` window to match; Manual captures the foreground root
   when its hotkey fires. PID remains diagnostic/secondary identity because Chromium and
   Electron accessibility elements can legitimately run in a renderer process.
4. Give clipboard snapshotting and post-injection Copy response separate bounded deadlines;
   snapshot time must not consume the application's response budget. Invoke Copy with
   `SendInput`, wait for clipboard change, and read
   `CF_UNICODETEXT`. If the first attempt produces no clipboard change, permit exactly one
   retry only while the original clipboard sequence and owner are unchanged and the same
   source process is still foreground.
5. Restore the bounded snapshot through a short-lived clipboard owner in a finally-style cleanup path; fail closed for unsupported or over-budget formats.
6. For automatic Selection, try a bounded native Edit/RichEdit adapter after
   UI Automation. Resolve the control from the mouse-up point, require its
   process ID to match the trigger, read the selected range and bounded full
   control text, and derive sentence context without touching the clipboard.
7. Use the bounded, fail-closed clipboard snapshot/restore fallback for Manual and
   automatic Selection after UI Automation and native-control extraction fail, but only for a drag or
   double-click candidate with a non-empty selection rectangle. A plain click
   and Hover never invoke Copy.
   Selection must preserve the original clipboard and must not advance to the
   provider when copied text is empty or invalid.

OCR actions:

1. Capture with Win32 into an in-memory bitmap and convert for `Windows.Media.Ocr`.
   Detach the `DataWriter` buffer before any operation that drains it, verify its exact
   `width * height * 4` byte length, and create a BGRA8 `SoftwareBitmap` with ignored alpha.
2. Use the drag rectangle plus 16 px for Selection. Use the last selection rectangle or a DPI-aware 960×320 logical-pixel pointer crop for Manual. Hover uses a wider DPI-aware 1536×384 logical-pixel pointer crop so a word near either end of a normal sentence can still yield complete context; capture allocation and the Windows OCR runtime dimension remain hard bounds.
3. For Hover, choose only an OCR word box containing the pointer and derive sentence context locally
   from the target line plus geometrically contiguous wrapped lines. Never snap blank space to a
   nearby word. When UI Automation already found a valid word, OCR may enrich only its context and
   must not replace the target.
4. Clamp captures per monitor, handle protected/blank content, and release every bitmap/DC promptly.
5. Never write screenshots to disk or logs.

Verification:

- Test bounded HGLOBAL snapshot restoration for Unicode text and rich clipboard content.
- Test that a foreground-process mismatch prevents `SendInput`, and that the single retry
  is suppressed after a foreground, clipboard-sequence, or clipboard-owner change.
- Test that a captured source root mismatch prevents `SendInput`, while a Chromium renderer
  PID difference under the same UI Automation/root-window tree remains admissible.
- Test SoftwareBitmap construction from a known BGRA buffer, including exact byte-length,
  dimensions, pixel format, and alpha mode.
- Test that Selection routes UIA -> native Edit -> clipboard -> OCR, Manual routes UIA ->
  clipboard -> OCR, and Hover routes UIA -> OCR without touching the clipboard.
- Test OCR with 100%, 150%, and 200% DPI and across two monitors when available.
- Test blank/protected captures and missing OCR language support.
- Assert fallback stops after the first valid extractor and empty OCR output produces zero requests.

Exit gate: all three trigger-specific extraction chains match the fixed order and stop-on-success rule.

Support boundary: the ordinary-application target includes native controls, browsers,
Electron applications, Office, terminals, and PDF readers. Windows deliberately prevents
universal extraction from password fields, protected/DRM surfaces, higher-integrity
processes, secure desktops, and applications that expose neither selectable text nor
capturable pixels. These platform restrictions must fail silently for automatic triggers
and must never cause an empty provider request.

### Task 8 — Implement configuration, prompts, and credentials (implemented; manager smoke test pending)

**Files:** `crates/core/src/config.rs`, `prompt.rs`; `windows/crates/platform-windows/src/credentials.rs`; manager Settings/Prompts views.

Configuration path: `%LOCALAPPDATA%\SelectionTranslate\config.toml`.

Profile fields:

- `id`, `name`, `system_prompt`, `user_template`
- optional `model`, `temperature`, `max_output_tokens`

Rules:

- Allow only `{target}`, `{context}`, and `{source}`.
- Require `{target}`; reject duplicate IDs and unknown placeholders.
- Ship editable contextual Chinese-English translation, word explanation, and wiki-style profiles.
- Store separate default profile IDs for Selection and Hover.
- Use the configured `hotkeys.cycle_profiles` chord (default `Ctrl+Alt+P`) to cycle profiles; `Ctrl+Alt+T` and `Ctrl+Alt+H` remain fixed. Re-register the cycle chord after each valid config reload.
- Save TOML atomically through the manager; reload with `ReadDirectoryChangesW`.
- Store API keys only in Windows Credential Manager through `CredWriteW/CredReadW`. TOML stores only a credential target name.
- If config or credentials are invalid, automatic jobs stay silent and Manual opens a local configuration error.

Verification:

- Unit-test parsing, validation, placeholder rendering, atomic replacement, reload, and missing credentials.
- Scan committed files for key-like literals before continuing.

Exit gate: a valid rendered prompt cannot exist without a valid target.

### Task 9 — Implement the OpenAI-compatible provider (complete)

**Files:** `crates/provider-openai/src/{client,sse,error}.rs`; local mock server under crate tests.

Actions:

1. Use WinHTTP on a worker thread; do not introduce an async runtime.
2. Support configurable base URL, default model, 30-second timeout, and `POST /v1/chat/completions`.
3. Permit HTTPS and loopback HTTP only.
4. Support SSE `data:` frames, `[DONE]`, split UTF-8 chunks, and non-streaming JSON fallback.
5. Stream deltas through `PopupSink`.
6. Cancel by setting the job token and closing the active WinHTTP request handle.
7. Map DNS, TLS, timeout, HTTP, rate-limit, malformed JSON, and cancellation failures to typed local errors.
8. Never log headers, credentials, prompt bodies, target text, context, or output.

Verification:

- Use only the local mock server to test streaming, chunk boundaries, non-streaming output, timeout, 401, 429, 500, malformed data, and cancellation.
- Prove the provider is unreachable unless it receives `PreparedRequest`.

Exit gate: all provider tests pass without external internet access.

### Task 10 — Complete coordinator, popup, and cache (implemented; end-to-end smoke test pending)

**Files:** core coordinator/cache; Windows popup; resident composition root.

Actions:

1. Connect triggers → ordered extractors → request gate → cache → provider → popup → history event.
2. Cache at most 256 completed results for 10 minutes using normalized target, context, prompt, model, and inference parameters.
3. Show loading only after the request gate passes or immediately show a cache result.
4. Implement a native non-activating, topmost popup near the pointer with streaming text,
   Copy, Retry, Prompt, Pin, and Close. Create it with `WS_EX_TOPMOST`; after creation and
   every reanchor, verify the window remains visible, uncloaked, on-screen, and topmost.
   A failed presentation invariant is a popup-admission failure, not a successful surface.
5. Retry and prompt change create new jobs through the same request gate.
6. Discard late deltas and completions from stale jobs.
7. Automatic extraction failure stays silent; Manual may show a local error.
8. Append a universal concise-Markdown output contract only after request admission and prompt rendering. Render completed responses with the system `RICHEDIT50W` control; support headings, lists, blockquotes, emphasis, strikeout, inline/fenced code, rules, and readable links without a resident WebView. Keep streaming immediate and preserve raw Markdown for Copy.

Verification:

- Integration-test every trigger/extractor path with fakes.
- Test cache keys, eviction, expiry, cancellation, retry, prompt change, and stale streaming deltas.
- Test Markdown parsing, UTF-16 formatting spans, request-contract propagation, raw-copy preservation, and actual bold formatting in a hidden `RICHEDIT50W` control.
- Manually verify focus preservation, keyboard accessibility, monitor clamping, and rapid repeated selections.

Exit gate: a complete translation works against the local mock provider with no duplicate requests.

### Task 11 — Implement SQLite history and the manager (implemented and smoke-tested)

SQLite schema version 1 was explicitly approved on 2026-08-19 before implementation. The resident writer and manager use the same validated storage contract; no credentials or screenshots enter the database.

**Files:** `crates/storage/src/{db,migrations,history}.rs`; `windows/apps/manager/src/*`.

Schema version 1:

```sql
CREATE TABLE history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at_utc TEXT NOT NULL,
    source TEXT NOT NULL,
    target TEXT NOT NULL,
    context TEXT,
    output TEXT NOT NULL,
    prompt_id TEXT NOT NULL,
    model TEXT NOT NULL,
    served_from_cache INTEGER NOT NULL CHECK (served_from_cache IN (0, 1))
);
CREATE INDEX history_created_at_idx ON history(created_at_utc DESC);
PRAGMA user_version = 1;
```

Actions:

1. Store only completed results; never store screenshots, credentials, partial output, failed jobs, or cancelled jobs.
2. Insert and delete rows older than the newest 1,000 in one transaction.
3. Open SQLite only for a short resident write or while manager History is open.
4. Build native History, Prompts, and Settings tabs in one manager executable.
5. History supports search over target/output, prompt filter, source filter, date ordering, copy, and delete-one-entry. Deletion requires an in-app confirmation because it is destructive.
6. The manager exits when its last window closes.

Verification:

- Test first creation, schema version, transactional pruning, Unicode, search/filtering, concurrent read/write, corrupt DB error handling, and no insertion for failed/cancelled jobs.
- Confirm exactly 1,000 rows remain after inserting 1,001.

Exit gate: the resident process closes the database after every write and manager memory disappears after exit.

### Task 12 — System verification and local packaging (in progress; final release gates pending)

Current r13 evidence (2026-08-26):

- Package: `windows/dist/selection-translate-x64-20260826-r13`.
- Resident SHA-256: `CAEE064535C20075097136BA59392C5D2D1782E3CEC109E76921745AA77D6394`.
- Manager SHA-256: `D2E95AE55EA155B67ACC520D18CA2989D20F18F44D91491700D8084DC92E8FCC`.
- The complete offline workspace verification passed: `cargo fmt --all -- --check`, workspace tests, strict workspace/all-target Clippy, and the optimized release build for resident and manager.
- The r13 manager source includes tab-overlap protection (`WS_CLIPSIBLINGS`) and the OCR formatting-token/no-target correction. These changes are covered by source-level tests; packaged visual behavior is still an open gate.
- The pre-request warmed-idle baseline, measured with Hover disabled, popup closed, manager stopped, and SQLite closed, was 1.250 MiB average private working set, 1.562 MiB peak, and 0% average CPU across 60-second and 300-second observations. This is not the final approved post-success-request memory gate, which remains open below.
- The r13 release-binary Selection E2E passed with the exact 57-character sentence: native extraction succeeded after UIA declined the control, exactly one local mock-provider request contained the sentence, loading and completed popup states were observed, foreground and clipboard were preserved, and exactly one history row was stored. Evidence: `windows/tmp/selection-sta-run-20260826094611830`.
- The folder is unsigned and intended for local testing. No real API claim is made by local mock-provider tests.

Still-open gates (each must be run against the current packaged candidate and recorded with its
exact path and SHA-256; older-package evidence is non-authoritative):

- Manual E2E from direct manager startup through configuration and translation.
- Packaged manager Settings/Prompts/History three-tab visual and reload verification.
- Memory/CPU measurement after a successful request, with popup/manager/SQLite closed afterward.
- Cross-application and DPI matrix: Notepad, Chromium, Firefox, Office, VS Code, terminal, PDF reader, 100%/150%/200% DPI, and multiple monitors where available.
- Real API connectivity, only in a turn with explicit user permission for network access.

Actions:

1. Run the package script or the equivalent commands below. The package script runs all four checks before copying and refuses a non-empty destination; it never deletes an old package:

```powershell
cmd /c 'call "D:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 && "D:\DevTools\cargo\bin\cargo.exe" fmt --all -- --check'
cmd /c 'call "D:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 && "D:\DevTools\cargo\bin\cargo.exe" clippy --workspace --all-targets --locked -- -D warnings'
cmd /c 'call "D:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 && "D:\DevTools\cargo\bin\cargo.exe" test --workspace --locked'
cmd /c 'call "D:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 && "D:\DevTools\cargo\bin\cargo.exe" build --locked --release -p selection-translate-resident -p selection-translate-manager'
```

2. Verify ordered fallbacks, clipboard restoration, OCR DPI behavior, screenshot non-retention, no-text/no-request, profile reload, cancellation, offline errors, and 1,000-entry pruning.
3. Test Chromium, Firefox, Office, VS Code, native controls, terminals, and PDF readers, including the required DPI and monitor cases.
4. Warm the resident process with one successful local mock request, close popup/manager/SQLite, idle 60 seconds, then measure for five minutes. The pre-request r11 idle result is recorded above; the post-success-request result is still required.
5. Accept only if private working set is below 20 MiB and average CPU is at most 0.1% in the required post-request condition.
6. Produce or refresh the local portable x64 release folder containing the two executables and user documentation. Do not publish, deploy, sign, or create a public release without separate approval.

Exact next-unlocked test order (stop at the first failure; retain evidence and clean processes before reporting it):

1. Selection release-binary E2E is complete; retain `windows/tmp/selection-sta-run-20260826094611830` as its evidence.
2. r51 UIA Hover and forced-OCR Hover release-binary E2Es are complete; retain `windows/tmp/hover-sta-run-20260830054221269` and `windows/tmp/hover-ocr-run-20260830054255338` as their evidence.
3. Run the packaged r51 Manual E2E from direct manager startup; verify configuration/credential refresh acknowledgement, one translation, popup/history, and cleanup.
4. Run the packaged manager three-tab visual/reload test; verify exactly one visible page and no overlap before and after reload.
5. Run the post-success-request memory/CPU measurement with Hover off and popup/manager/SQLite closed.
6. Run the cross-application/DPI/multi-monitor matrix and the remaining fallback/no-text/privacy checks.
7. Only after the local gates pass, ask for explicit permission before running a real API request.

Final deliverables:

- `selection-translate-resident.exe`
- `selection-translate-manager.exe`
- Default prompt/config template without credentials
- Local setup, privacy, hotkey, fallback, and troubleshooting documentation
- Test results and memory report

## 4. Global completion definition

v0.1 is complete only when:

- Selection and Manual are always available; Hover is session-only and defaults off.
- Each extraction chain stops at its first valid result.
- Selection preserves the first valid selected target, then automatically derives the full
  containing sentence. If UIA/native extraction cannot supply it, bounded Windows OCR may enrich
  missing context without replacing the target; failure to enrich does not fabricate context.
- Hover sends a target word plus locally derived sentence context.
- No valid target means no remote request, popup loading state, or history row.
- Screenshots remain memory-only and clipboard restoration is verified.
- Prompt profiles fully control translation/explanation behavior.
- History retains no more than 1,000 completed entries and its UI consumes no memory while closed.
- Release memory and CPU budgets pass using the documented measurement script.

### Task 14 — Selection prompt chooser (implemented; packaged interactive check pending)

Selection uses a native prompt chooser before request admission. The chooser is a compact single
 horizontal row centered immediately above the pointer. It displays one-word labels for the first
three prioritized standard profiles—`linguist-analysis`, `code-specialist`, and
`concise-explanation` when present—plus a `More…` fold button; other configured profiles remain
available under `More…` in their original configuration order. `More…` opens a native menu containing
one-word labels for all remaining profiles. Full configured names and IDs remain unchanged
internally. Each inline button sizes independently to just beyond its own label; buttons do not
share the longest label's width. It must not display the selected text, profile IDs, descriptions, prompt bodies,
output controls, or any other content.

Actions:

1. After Selection extraction returns a valid target, retain that extracted text locally and show
   the names-only chooser in one line above the selection pointer.
2. Do not run request admission, cache lookup, provider startup, or history insertion before the
   user clicks a profile name.
3. On a valid profile click, bind the retained Selection text to that profile and continue through
   the existing request gate, loading state, streaming Markdown result, cache, and history flow.
4. Closing the chooser with Escape, an outside click, or replacement by a newer Selection clears
   the retained text and produces zero provider requests and zero history entries.
5. Keep Hover and Manual on their existing direct/default-profile workflow. This chooser is only
   for the Selection trigger.
6. Refresh the displayed names when a valid configuration reloads while the chooser is open. An
   invalid reload closes it without sending a request.

Verification:

- No/invalid extracted target: no chooser and no request.
- Valid Selection target: names-only chooser and no request before a click.
- Each displayed name maps to exactly its configured profile ID.
- One profile click starts exactly one request with the retained target and chosen profile.
- Escape/outside click produces no request or history entry.
- Hover and Manual continue without the chooser.
- Config reload never leaves a stale profile mapping.

Exit gate: opening the names-only chooser is locally side-effect free; only an explicit profile-name
click can start the corresponding Selection request.

### Task 15 — Session-only Rest mode (implemented; packaged interactive check pending)

Rest mode is a tray-controlled session state. It starts disabled and is never persisted.

Actions:

1. Add a `Toggle Rest Mode` command to the resident tray menu and expose the active state in the
   tray tooltip.
2. Enabling Rest mode cancels pending extraction and provider work, closes any chooser/result
   popup, clears mouse deadlines, and prevents Selection, Hover, Manual, and prompt-cycle actions.
3. While resting, manager/configuration access and Exit remain available, and no new extraction,
   provider request, cache write, or history entry may start.
4. Disabling Rest mode resumes event handling from a clean mouse state. Preserve the user's
   session-only Hover enabled/disabled preference, but require fresh pointer movement/dwell.

Verification:

- Rest starts disabled on every resident launch.
- Enabling Rest cancels current work and closes the popup.
- Selection, Hover, Manual, and profile cycling are inert while Rest is enabled.
- Manager/config refresh and Exit remain functional.
- Disabling Rest restores normal triggers without replaying stale mouse events.

Exit gate: while Rest mode is enabled, the resident remains available in the tray but performs no
translation work and sends no provider requests.

### Task 16 — Unified dark popup surface (implemented; packaged visual check pending)

Replace the popup's three visibly separate regions with one cohesive native dark surface. Keep
semantic child controls internally for accessibility, selection, Markdown formatting, and prompt
commands; visual unification does not mean collapsing unrelated interactions into one RichEdit.

Actions:

1. Define one native dark palette for the popup background, primary/secondary text, separators,
   button states, and accent state. Paint the parent and all exposed child backgrounds from that
   palette without a WebView, resident UI framework, or new dependency.
2. Remove visible borders from the input and result controls. Present the containing sentence as
   muted text and the streamed Markdown result beneath it on the same background, separated only
   by spacing or a subtle divider.
3. Render the standard actions and dynamically created profile choices with consistent compact
   dark button styling. Keep the three prioritized inline profile choices and `More…` behavior.
4. Keep chooser-to-loading-to-result transitions within the same popup surface and suppress
   intermediate redraws so no light frame or empty panel flashes during the transition.
5. Preserve the r36 streaming invariant: update/format text while redraw is disabled, restore the
   caret and reader viewport, then paint one stable frame. Do not theme or recreate controls per
   response delta.
6. Keep the fallback EDIT path readable. Apply native dark scrollbar theming only when available;
   a Windows-provided scrollbar-theme fallback must not make popup creation fail.

Verification:

- Native tests verify the dark RichEdit background/default foreground, borderless control styles,
  button draw-state mapping, and chooser/result visibility invariants.
- Existing Markdown, first-visible-line, dismissal, profile mapping, DPI, and monitor-edge tests
  continue to pass.
- Workspace formatting, all tests, strict Clippy, and optimized MSVC packaging pass.
- A packaged manual check at normal DPI confirms one visually unified dark card with no light
  transition frame, readable input/result text, and usable prompt/action buttons.

Exit gate: chooser, input, streamed result, and actions read as one dark popup surface while
retaining native accessibility, selection, profile switching, and the sub-20-MiB resident design.

### Task 17 — Live Markdown rendering and popup dragging (implemented; packaged visual check pending)

Actions:

1. Parse and format the accumulated response as Markdown on every admitted streaming delta rather
   than exposing raw Markdown markers until completion. Incomplete trailing delimiters remain
   readable plain text until a later delta completes them.
2. Preserve the r36/r38 invariants while live formatting: update under redraw suppression, retain
   the first visible line, keep the light dark-theme foreground, and paint one stable frame.
3. Add a dedicated top drag band to the result popup. A client mouse-down inside that empty band
   explicitly enters the native `WM_NCLBUTTONDOWN/HTCAPTION` move loop; input/output text and all
   prompt/action controls retain their existing selection and click behavior.
4. Keep chooser positioning and compact dimensions unchanged. Dragging applies to the expanded
   result surface and must not start a provider request, change prompt selection, or activate the
   source application.
5. Coalesce provider deltas in the resident and render only the newest accumulated streaming state
   on one bounded 40-millisecond popup timer. Never perform more than one full Markdown/RichEdit
   projection for a queued burst, keep the final state lossless, and defer the timer while the
   native move loop is active.
6. Do not force synchronous RichEdit painting after a stream render. Invalidate the control and
   return to the Windows message pump so pointer/input messages are serviced before paint. Run the
   `WH_MOUSE_LL` hook and its message loop on a dedicated thread whose callback only copies event
   metadata, posts it to the resident, and promptly calls `CallNextHookEx`.

Verification:

- Incremental Markdown tests cover delimiters split across deltas, headings/lists, Unicode, and
  the final rendered-text equivalence with completed output.
- Native RichEdit tests verify live formatting, light foreground color, and viewport preservation.
- Routing tests verify only drag-band points enter the move path while content/action points remain
  ordinary client interactions at 100%, 150%, and 200% scaling.
- Coalescing tests verify burst deltas arm one render, timer delivery consumes only the newest
  state, completion cannot be lost, and movement defers visual work without dropping raw output.
- Mouse-hook lifecycle tests verify startup acknowledgement and bounded shutdown of the dedicated
  hook thread; the callback remains free of extraction, rendering, and provider work.
- Workspace formatting, all tests, strict Clippy, and optimized MSVC packaging pass.

Exit gate: Markdown becomes readable as it streams, the expanded popup can be moved from its top
drag band without sacrificing text selection, button interaction, or source focus behavior, and
stream rendering cannot delay global pointer delivery.

### Task 18 — Refined native result popup (implemented and packaged in r54)

The popup must look current without adding a resident UI framework. Keep the implementation in
Win32/GDI/RichEdit so the warmed idle memory contract and later portability boundaries do not
change.

Actions:

1. Replace the flat full-window gray/black presentation with a restrained layered dark palette:
   one deep outer canvas, slightly raised input/result surfaces, a low-contrast hairline border,
   high-contrast output text, muted metadata, and one cool accent color.
2. Use a DPI-scaled rounded outer silhouette and subtle native drop shadow. On systems where the
   shadow is unavailable, the rounded region and border must remain complete and readable.
3. Establish a clear type hierarchy using native Segoe UI-family fonts: compact uppercase section
   labels, smaller muted input/context text, comfortable result text, and concise action labels.
   Windows font substitution remains acceptable; no font file is bundled.
4. Increase whitespace deliberately rather than increasing chrome: use a 14-logical-pixel outer
   inset, 10-pixel inter-card gap, internal card padding, and a compact bottom action row. Preserve
   a blank drag band and keep the popup within the monitor work area at every DPI.
5. Draw action/profile controls as compact rounded pills with distinct normal, pressed, disabled,
   and keyboard-focus states. Keep all existing command IDs, accessible text, chooser ordering,
   pin/retry/prompt behavior, and no-activation semantics.
6. Preserve live Markdown coalescing, first-visible-line retention, raw Copy text, text selection,
   outside-click dismissal, dragging, popup pinning, streamed updates, and the existing
   `WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW` contract.

Verification:

- Geometry tests cover the outer inset, card/content rectangles, action row, drag band, rounded
  radius, and monitor clamping at 96/144/192 DPI.
- Palette/button tests cover contrast separation and owner-draw state mapping.
- Existing Markdown, streaming, dragging, dismissal, chooser, and presentation tests remain green.
- A local loopback popup is captured and visually inspected at the active display DPI; it must
  show one coherent surface with no default gray controls, clipped text, overlapping regions, or
  unreadable first line.
- Workspace formatting, all tests, strict Clippy, optimized packaging, and the sub-20-MiB idle
  memory check pass before the task is marked complete.

Exit gate: the result popup reads as a deliberate lightweight desktop product rather than a set of
default Win32 controls, while retaining every functional and resource invariant above.

Completion evidence (2026-08-30): packaged candidate
`windows/dist/selection-translate-x64-20260830-r54` passed the complete 202-test workspace gate,
strict Clippy, formatting, and optimized MSVC packaging. The packaged deterministic Selection test
confirmed one loopback request containing the whole selected sentence, a visible RichEdit result,
Markdown projection with markers removed, and readable Unicode. The final active-DPI screenshot at
`windows/tmp/markdown-popup-run-20260830104915795/popup-visual.png` was inspected for the rounded
silhouette, type hierarchy, spacing, action alignment, first-line visibility, clipping, overlap,
and default-control artifacts. The 60-second fresh idle memory gate peaked at 2.375 MiB, below the
20 MiB product limit.

### Task 19 — Stable universal Hover lifecycle and exact pointed text (implemented and verified)

Hover is a coordinate-driven text-inspection feature, not a selectable-text feature. It must keep
the currently presented result stable while the pointer moves, use a deliberate 500-millisecond
dwell, and dismiss the result only when the user changes foreground root window (or explicitly
closes/disables it). UI Automation remains the first extraction path and bounded Windows OCR is
the fallback for non-selectable or otherwise unexposed text.

Implementation sequence:

1. Change the tested Hover dwell constant from 350 ms to 500 ms. Preserve the four-pixel
   accumulated-motion threshold, one-shot timer design, disabled-at-start behavior, and generation
   checks that prevent stale point admission.
2. Separate candidate invalidation from presentation lifetime. Pointer movement may invalidate a
   pending point extraction, but must not cancel an admitted provider request, clear its stream, or
   dismiss its popup. A later valid Hover result may atomically replace the prior result; an empty
   or stale candidate must leave it untouched.
3. Store the source foreground root and trigger for every presented popup. Add one event-driven
   foreground-window observer which posts bounded metadata to the resident message loop. For an
   unpinned Hover popup, dismiss and cancel its work only when the foreground root differs from the
   recorded source root. Do not poll the foreground window. Explicit Close, Rest mode, Hover-off,
   shutdown, and higher-priority Manual/Selection actions retain their existing semantics.
4. Make the Hover extractor boundary explicit: `ElementFromPoint` plus raw-view ancestor walking
   tries exact UIA text geometry first; if the element exposes no usable text range, run one bounded
   local OCR capture around the pointed element/coordinate. Selection adapters and clipboard
   mutation are never part of Hover.
5. Preserve exactness for non-selectable text. UIA is admitted only when the unchanged pointer is
   inside the returned word rectangle. OCR is admitted only when the pointer lies inside an OCR
   word box; the exact box text is the target, while the geometrically joined containing sentence
   is context. A nearby/snapped word, unrelated semantic `Name`, or context without a target is a
   local miss and sends no request.
6. Stop the chain at the first valid target and perform at most one bounded context-enrichment
   step. Recheck source root and Hover generation before request admission so a window switch or
   stale coordinate cannot leak text from the wrong surface.
7. Allow Hover over the popup's own read-only input and RichEdit result text. Narrow the current
   resident-process exclusion so resident-owned clicks still go only to popup controls and never
   become Selection triggers, while resident-owned `WM_MOUSEMOVE` can arm Hover only over those two
   text controls. Extract through their native text geometry, not OCR. A valid self-popup Hover may
   replace the current result only after request admission; an empty/control-background point keeps
   the current result. Track extraction root separately from the external foreground guard root so
   the popup's `WS_EX_NOACTIVATE` behavior cannot cause immediate self-dismissal or a recursive loop.

Verification:

- State-machine tests prove 500 ms dwell, jitter behavior, pointer-motion invalidation of pending
  extraction only, stable active/completed Hover presentation, and valid-result replacement.
- Foreground-observer tests prove same-root focus changes retain the popup, different-root changes
  dismiss it, pinned popups remain, and no polling loop is introduced.
- UIA/OCR tests cover selectable text, non-selectable static labels, canvas/image text, exact word
  containment, sentence context, blank pixels, snapped UIA ranges, and stale root/generation.
- Self-popup tests cover input/result words, button/background rejection, absence of Selection
  synthesis, stable foreground guarding, and bounded replacement without recursive re-triggering.
- A packaged matrix test exercises native UIA text and forced OCR text, verifies one request for
  the exact pointed word plus sentence, then moves the pointer and changes focus/root to prove the
  new lifetime contract.
- Full formatting, workspace tests, strict Clippy, optimized packaging, clipboard/foreground
  preservation, no retained screenshots, and the sub-20-MiB Hover-off idle gate pass.

Exit gate: Hover works on selectable and non-selectable visible text, never guesses a nearby word,
waits 500 ms, and keeps its result readable until the foreground application changes.

Completion evidence (2026-08-30): the release-binary UIA Hover E2E admitted exactly the pointed
word with its containing sentence, proved that no request arrived before 500 ms, retained the
result across pointer movement, rejected blank and loading-state popup text, translated completed
text inside its own pinned popup without unpinning it, and dismissed the unpinned result while
cancelling a pending dwell after a real foreground-root change. The forced-OCR E2E admitted one
exact word from a non-selectable painted surface, retained the geometrically joined sentence,
excluded a remote column, sent no blank request, and retained no capture image. Clipboard and
foreground state were preserved in both paths.

### Task 20 — Modern on-demand manager UI (implemented and verified)

The manager may use more resources while open, but it must remain a separate on-demand process and
must not add any resident UI framework or background service. Keep the existing configuration,
credential, prompt, and history behavior while replacing the fixed default-control presentation
with a coherent DPI-aware native interface.

Implementation sequence:

1. Define a manager-owned native design system: the popup's dark palette, Segoe UI hierarchy,
   spacing/radius metrics, focus/error/accent states, and reusable GDI resource ownership. Do not
   bundle fonts, WebView, Electron, or a new runtime.
2. Replace the wide top tab strip with a compact left navigation rail and a clear page header.
   Keep exactly three destinations—Settings, Prompts, History—and preserve the existing exclusive
   page-container visibility invariant and all command IDs/business logic.
3. Introduce one tested layout model in logical pixels and apply it from `WM_SIZE` and
   `WM_DPICHANGED`. Each page receives a bounded content rectangle; controls are positioned only
   relative to that rectangle. Minimum window dimensions and page-local scrolling prevent any
   control overlap or clipping at 96, 144, and 192 DPI.
4. Restyle native labels, edits, combos, lists, navigation, and actions with consistent dark
   backgrounds, readable states, compact rounded primary/secondary buttons, and visible keyboard
   focus. Credentials remain in Windows Credential Manager; no value is painted, logged, or copied
   into configuration.
5. Recompose Settings into provider, credentials, defaults, and inference cards; Prompts into a
   profile selector/editor with one stable action row; History into filters, list, and readable
   target/output detail regions. Preserve lazy database opening and close it when leaving History.
6. Add accessibility names/tab order, resize/DPI tests, exclusive-page tests, and screenshot probes
   for all three pages. The manager executable may grow; the warmed resident with manager closed
   must remain below 20 MiB.

Verification and exit gate: all three pages are visually inspected at the active DPI with no
overlap, blank page, default light controls, clipped labels, or inaccessible actions; configuration,
credential refresh, prompt save, history search/delete, and resident acknowledgement tests pass;
the full package and resident-memory gates remain green.

Completion evidence (2026-08-30): the native manager now uses a dark Segoe UI design system,
compact left navigation, exclusive page containers, owner-drawn actions, DPI-aware resize handling,
and dark native edit/list/combo scrollbars without loading a UI framework into the resident. Root
review screenshots of Settings, Prompts, and History at the active 250% DPI show the correct selected
destination and no cross-page overlap or blank page. Ten manager tests, the 210-test workspace gate,
strict workspace Clippy, and optimized packaging passed. With the manager closed and Hover off, the
60-second resident gate measured 1.548 MiB average and 1.574 MiB peak private working set; observed
total working set was about 13.4 MiB, below the 20 MiB product limit.

### Task 21 — Cascaded popup inspection, Hover token cleanup, and manager localization (approved; implementation in progress)

This task intentionally supersedes Task 19's “foreground change only” dismissal rule. An unpinned
popup must also close when the user clicks outside it. Hovering completed result text creates a
separate cascaded result popup so the source explanation remains available; it must never overwrite
the source surface or route streamed output to the wrong window.

#### 21.1 — Replace singleton popup state with a bounded stable-ID registry

1. Keep one global provider job active at a time, but replace the singleton popup fields with a
   registry of at most four `PopupEntry` values keyed by monotonic `PopupId`, never HWND. Each entry
   owns its native popup, trigger, request metadata, foreground guard, optional parent ID, pin state,
   placement, and creation order. Active provider state owns the exact destination `PopupId`.
2. Put `PopupId` in every popup callback `LPARAM`; keep command-specific values such as profile
   index in `WPARAM`. Route Delta, Finished, Retry, Prompt, Close, and profile choice through the
   exact ID. Queued callbacks from a destroyed/reused HWND must be harmless.
3. A normal Selection or Manual result creates a root entry. Hover over completed popup output
   records its source ID and, only after extraction, normalization, source revalidation, request
   preflight, capacity admission, and native-window creation succeed, commits a distinct child
   entry. The source entry remains unchanged. A failed child transaction sends no request and leaves
   every existing popup intact.
4. Permit cascading from child to grandchild. At four visible popups, evict the oldest unpinned,
   completed entry that is neither source nor active destination. If every candidate is pinned or
   protected, reject the automatic cascade locally before provider startup. Pinning cannot bypass
   this fixed resident-memory bound.
5. Place a child beside its parent with a compact gap, preferring the right and falling back left;
   clamp to the monitor work area. Never reanchor or unpin the source popup.

#### 21.2 — Restore outside-click dismissal for multiple popups

1. Resolve every mouse-down against all registered popup roots and child controls before the
   resident-process exclusion. Clicking outside a popup closes that unpinned popup. Therefore an
   external click closes all unpinned popups, while a click inside popup B retains B but closes any
   other unpinned popup for which that point is outside. Pinned popups survive.
2. Dismissing an entry cancels only the provider job, pending extraction, or chooser owned by that
   entry. User Close removes only its ID. Retry and Prompt use the clicked entry's request, never a
   global “last popup” request.
3. Retain foreground-root dismissal as a second lifetime boundary. Activation of any registered
   app popup is internal; another application closes unpinned Hover/cascade entries whose inherited
   source guard changed and invalidates pending children.

#### 21.3 — Sanitize only exact Hover targets before remote admission

1. Add one portable, idempotent Hover-token sanitizer in `selection-core`. Use the already locked
   and locally cached `unicode-ident` tables for Unicode identifier continuations/combining marks;
   add no network, global dependency, normalization fold, or case conversion.
2. Remove zero-width formatting, surrounding whitespace, bullets, Markdown markers, quotation
   marks, brackets, and sentence punctuation only at token boundaries. Require at least one Unicode
   letter/number; preserve CJK, digits, decomposed combining marks, underscores, and supported
   internal apostrophe/hyphen/code connectors. Preserve the exact contiguous source substring;
   never delete arbitrary interior symbols and concatenate two pieces.
3. Reject punctuation-only, emoji/symbol-only, formatting-only, embedded control/newline, or
   ambiguous interior-junk targets. Sanitize each UIA Hover result before deciding whether OCR
   fallback is needed, so unusable UIA text does not block OCR. Repeat the idempotent check at the
   request gate as defense in depth.
4. Keep the extractor's pointer-anchored full sentence as context. After cleaning, require that the
   exact cleaned target still occurs in that normalized sentence; otherwise return a local miss and
   send zero requests. Selection and Manual inputs remain byte-for-byte unchanged apart from their
   existing whitespace/zero-width normalization because their punctuation and multiline code may
   be intentional.

#### 21.4 — Add persistent English / Simplified Chinese manager UI

1. Extend core configuration with optional `[ui] manager_language = "en" | "zh-CN"`. Existing
   files without `[ui]` load as English and remain valid; `ui` is not added to the required-section
   check. This is a backward-compatible config extension, not a database migration. Credentials,
   prompts, history content, and provider payloads are untouched.
2. Add a compact `Interface language / 界面语言` selector to the Settings header. On change, clone
   the loaded config, modify only the language field, atomically save it, and relabel only after the
   save succeeds. Do not capture or persist unsaved provider/prompt edits, notify/restart the
   resident, or expose credential values. Restore the old selection on failure.
3. Replace manager-authored literals with typed `TextKey` and `StatusEvent` catalogs complete in
   both languages. Relabel the window title, navigation, page titles/subtitles, static labels,
   actions, combo options, confirmations, entry/profile counts, validation/save/credential/history
   statuses, and stable diagnostic prefixes live without restart. Append opaque OS/Rust error detail
   unchanged after the localized prefix. User prompt/profile/history data is never translated.
4. Continue UTF-16 Win32 rendering and Segoe UI system fallback; bundle no font or UI runtime.
   Rebuild localized combo contents while preserving semantic selections and keep the existing
   DPI-aware exclusive page containers.

#### Task 21 verification and exit gate

- Unit/state tests cover stable callback routing, source-close cancellation, job-to-popup stream
  ownership, child/grandchild creation, failed transaction preservation, Retry/Prompt targeting,
  pinning, foreground handling, outside-click semantics, cap/eviction, and all-pinned zero-request.
- Hover sanitizer tests cover CJK, Unicode digits/marks, decomposed accents, curly quotes, Markdown
  decoration, internal apostrophes/hyphens/underscores/code connectors, ambiguous junk triggering
  OCR fallback, punctuation/emoji-only silence, exact target-in-context, and unchanged
  Selection/Manual behavior.
- Localization tests prove legacy-config compatibility, strict language codes, atomic persistence,
  catalog/status completeness, live en → zh-CN → en relabeling, preserved unsaved fields, and combo
  semantic indices. Visual captures cover all three pages in both languages at 96/144/192 DPI with
  no overlap, clipping, blank page, light control, or missing CJK glyph.
- Packaged E2E creates a root, child, and grandchild with three unique HWNDs and correctly routed
  streamed results; verifies completed-output-only self-Hover, outside-click closure, Pin survival,
  source closure during pending extraction, no-text/noise zero-request, clipboard preservation, and
  manager language persistence after reopen.
- Formatting, the full workspace tests, strict Clippy, optimized packaging, and a four-popup memory
  measurement pass. With manager closed, the resident remains below 20 MiB; if four popups exceed
  the product limit, reduce the fixed cap rather than weaken the memory gate.

Exit gate: outside clicks close unpinned popups; completed popup words open independent bounded
cascades with correct request ownership; Hover sends a clean exact token or nothing; and the manager
can switch and persist complete English or Simplified Chinese UI without affecting user data,
credentials, LLM prompts, or resident idle memory.

### Task 13 — Make the packaged runtime self-starting and failures observable (implemented; packaged interactive exit gate pending)

This task was added after the packaged application could be configured through the manager while no resident was running, leaving the product apparently inert even though isolated translation tests passed.

Actions:

1. When the packaged manager is opened directly, reliably detect or start its sibling resident without creating duplicate resident instances.
2. Replace best-effort configuration and credential refresh notifications with bounded acknowledgements; the manager must never report that the running resident was refreshed unless the resident confirms it.
3. Show privacy-safe visible diagnostics for fatal resident startup failure and required Manual-hotkey registration failure. Never include credentials, endpoint, model, prompt, selected text, window title, or raw provider/Win32 error details.
4. Treat popup creation as a request-admission prerequisite so a provider request cannot begin when no result surface can be created.
5. Respect supported Windows proxy configuration instead of forcing direct WinHTTP networking, while retaining HTTPS-only remote endpoint enforcement.
6. Add deterministic tests for resident-start decisions, refresh acknowledgement outcomes, visible diagnostic categories, popup-admission failure, and proxy access-mode selection.
7. Rebuild into a fresh package directory and validate direct-manager startup, config/key refresh, Manual translation, automatic no-text behavior, and exact cleanup before resuming resource gates.

Current evidence: the implementation and deterministic automated tests for resident-start decisions, refresh acknowledgements, diagnostic categories, popup admission, and proxy access-mode selection are present and pass in the offline workspace verification. The packaged direct-manager startup, refresh, Manual translation, automatic no-text behavior, and exact-cleanup checks have not yet been interactively verified.

Exit gate: a user can open the packaged manager, configure the app, close the manager, and immediately use Manual translation without separately discovering or launching a second executable; any local failure before a provider request has a visible privacy-safe explanation. This gate remains open until the packaged tests in Task 12 pass.
