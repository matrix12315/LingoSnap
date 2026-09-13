# Selection Translate Project Rules

## Scope

This repository contains an ultralight selection and hover translation application. Windows is the first implementation target. Shared behavior must remain portable to a later macOS implementation.

## Structure

- `windows/`: all Windows-specific source, applications, scripts, build output, and temporary build files.
- `windows/crates/platform-windows/`: Win32, UI Automation, clipboard, capture, OCR, credential, and other Windows adapters.
- `windows/apps/`: Windows executable packages (`resident` and `manager`).
- `windows/scripts/`: Windows-only development, measurement, and local packaging scripts.
- `windows/target/` and `windows/tmp/`: generated Windows build output and Windows-only scratch files; neither contains source of record.
- `crates/`: portable shared Rust crates only. Shared crates must not be placed under `windows/`.
- `android/`: Reserved for a possible Android client; do not add Android code without an approved plan.
- Shared Rust crates and additional platform directories may be created only after their structure is documented in the implementation plan.
- Root Markdown files contain project-wide specifications and plans.

## Engineering Rules

- Validate architectural and memory assumptions with a small feasibility prototype before building product features.
- Keep triggers separate from extraction paths.
- Stop target extraction as soon as one path returns valid text. For Selection or Hover, if that
  valid target lacks a sentence containing it, one bounded local context-enrichment step may run
  afterward; it may add sentence context only when that context contains the unchanged target and
  must never replace or modify the selected/pointed target. Hover fails locally when no containing
  sentence can be derived.
- Never send an LLM or other remote request when no valid target text was detected.
- Treat text as absent when the normalized target is empty or contains only whitespace or zero-width formatting characters. Context alone is not a valid target.
- Prefer event-driven native APIs; do not add continuous polling where an event API is available.
- The warmed idle resident must remain below 20 MiB private working set with Hover off, popup closed, database closed, and manager stopped; this is a hard product requirement, not a stretch goal.
- Do not use Electron, a resident WebView, or a bundled OCR model without revising and approving the architecture first.
- Keep API keys and credentials in environment variables or OS credential storage. Never commit or store secrets in configuration, prompts, logs, or history.
- Do not retain screenshots by default.
- Preserve user clipboard contents during automatic selection and hover handling.

## Change Discipline

- Major changes require an approved plan before implementation.
- Update specifications before changing established project structure or conventions.
- Add verification commands here when the initial toolchain is selected.
- After code changes, run the relevant tests and memory checks before reporting completion.

## Toolchain Layout (Task 1)

- Rust target: `x86_64-pc-windows-msvc`.
- Rustup and Cargo homes: `RUSTUP_HOME=D:\DevTools\rustup` and `CARGO_HOME=D:\DevTools\cargo`.
- Visual Studio Community 2026 `18.9.1` is installed at `D:\Program Files\Microsoft Visual Studio\18\Community`.
- The x64 MSVC environment is initialized with `D:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat -arch=x64`.
- Installed MSVC tools are version `14.51.36231` with compiler `19.51.36256`; the Windows SDK is `10.0.26100.0` at `D:\Windows Kits\10`.
- Normal PowerShell PATH does not include Cargo or `cl.exe`. Build and verification commands must invoke `D:\DevTools\cargo\bin\rustc.exe`, `D:\DevTools\cargo\bin\cargo.exe`, and `D:\DevTools\cargo\bin\rustup.exe` explicitly, and run `cl.exe` inside a `VsDevCmd.bat`-initialized x64 shell.
- Keep Windows build downloads and temporary files under `D:\pythonProject\2026\selectionTranslate\windows\tmp`. Installed Rust/MSVC tooling remains outside the repository in the documented D: locations.
- Small unavoidable Microsoft installer/registry components may remain on C:, but Rust and the main Visual Studio workload must remain on D:.
- Task 1 verification must confirm the explicit D: Rust tools, the active MSVC target, `VsDevCmd.bat`/`cl.exe`, MSVC tool/compiler versions, and Windows SDK presence.

## Safety

- Ask before deleting files or directories, changing Git history, modifying credentials or CI/CD configuration, performing schema migrations, installing global dependencies, or publishing/deploying artifacts.
