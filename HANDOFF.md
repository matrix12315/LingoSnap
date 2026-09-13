# Selection Translate development handoff

## Snapshot scope

This folder is a source-only handoff snapshot of the Windows implementation. It contains the Rust workspace, Windows platform code, configuration template, packaging and verification scripts, project rules, documentation, lockfile, and implementation plan. It deliberately excludes generated output, local agent configuration, Git metadata, credentials, user configuration, databases, screenshots, logs, and test artifacts.

Source revision at handoff: `659756baf259613a744da89646052e402d211957` (`Document lightweight design in English and Chinese`).

## Directory map

| Path | Purpose |
| --- | --- |
| `crates/` | Portable Rust core, provider, storage, and platform-interface crates. |
| `windows/apps/` | Windows executables: resident and on-demand manager. |
| `windows/crates/platform-windows/` | Win32, UI Automation, OCR, clipboard, popup, tray, credential, and input adapters. |
| `windows/config/config.example.toml` | Safe configuration template. It never contains a real API key. |
| `windows/scripts/` | Package, memory, and Hover/OCR verification scripts. |
| `docs/` | Setup, privacy, fallback, troubleshooting, and verification documentation. |
| `IMPLEMENTATION_PLAN.md` | Project implementation plan and structural decisions. |
| `AGENTS.md` | Current project rules and D: toolchain layout. |

## Current product state

The active Windows goal covers:

1. Outside-click dismissal for unpinned result popups.
2. Hovering text in a completed result opens a bounded cascaded popup rather than replacing its source.
3. Hover target sanitization removes boundary noise but preserves CJK, code-like connectors, apostrophes, hyphens, sentence context, and the no-empty-request rule.
4. Persistent English/Simplified Chinese manager UI selection.
5. Idle resident memory below 20 MiB with the manager closed.

The implementation and unit coverage for all five items are present. The code keeps parent/source and destination popup identities separate, caps visible popup cascades at four, retains pinned popups, validates a source popup before admitting a cascade, and applies Hover sanitization both after extraction and again at the provider request boundary.

## Verified evidence

- Full package gate previously passed: formatting, 242 workspace tests, strict Clippy, and locked MSVC release build.
- A real forced-OCR Hover integration run passed. It proved one request, exact target and sentence context preservation, no unrelated OCR column, completed popup presentation, no request for blank/disabled Hover, clipboard/foreground preservation, no retained image files, and UIA-to-OCR fallback.
- The closed-manager idle resident memory gate passed over 288 samples / five minutes: 2.254 MiB peak private working set and 0.0117% average CPU. The required limits are below 20 MiB and at most 0.1% average CPU.

## Current uncommitted change

`windows/scripts/verify-hover-ocr.ps1` has one local, uncommitted change: it adds a real `Wait-PopupHidden` assertion after a fixture outside click. This strengthens runtime proof for unpinned outside-click dismissal.

The test command that would validate that new assertion was interrupted before it ran. The next developer must run this first:

```powershell
.\windows\scripts\verify-hover-ocr.ps1 -StaticCheck
.\windows\scripts\verify-hover-ocr.ps1 -ResidentPath .\windows\dist\selection-translate-x64-20260831-r66\selection-translate-resident.exe
```

If it passes, rerun the complete package gate to produce a fresh package. Do not claim final completion or publish a new release until that check has passed and the source is reviewed.

## Build and verification

Run from the repository root. The project uses the explicit D: toolchain defined in `AGENTS.md`:

```powershell
.\windows\scripts\package-release.ps1 -OutputDirectory windows/dist/selection-translate-x64-local
```

The script runs format checks, all workspace tests, Clippy with warnings denied, and the locked MSVC release build. Generated output remains under `windows/target`, `windows/tmp`, and `windows/dist`.

For the memory gate, launch a resident with manager closed, Hover off, and no popup open, then run:

```powershell
.\windows\scripts\measure-memory.ps1 -ProcessId PID -DurationSeconds 300 -SampleIntervalSeconds 1 -MaxPrivateWorkingSetMiB 20 -MaxAverageCpuPercent 0.1
```

## Security and publication

- Never put an API key in TOML, prompts, logs, history, the handoff folder, or Git.
- The supported credential sources are `OPENAI_API_KEY`, `SELECTION_TRANSLATE_OPENAI_API_KEY`, then Windows Credential Manager.
- The repository source is published on GitHub. The prior release is `v0.1.0-preview.1`; do not overwrite it.
- A Git push or public release needs the user's explicit approval under `AGENTS.md`.

## Excluded from this handoff

- `.git/`, `.codex/`, and the empty reserved `android/` folder.
- `windows/target/`, `windows/tmp/`, `windows/dist/`, and `windows/release/`.
- `window-inventory.obj` and any other compiler artifact.
- Local `config.toml`, API credentials, SQLite history, runtime traces, screenshots, logs, and old package ZIPs.

No files were deleted while creating this handoff.
