# Selection Translate

[简体中文](README.zh-CN.md)

Selection Translate is a lightweight native Windows x64 assistant for selected or hovered text. It obtains sentence context when the target application exposes it, sends the chosen prompt to an OpenAI-compatible API, and streams Markdown into a compact popup. The resident uses native Windows UI instead of Electron or WebView, and the manager runs only when opened.

This repository currently provides an unsigned preview build. Selection, Manual, and opt-in Hover share one priority-aware extraction pipeline. UI Automation is tried first and clipboard or Windows OCR is used only when applicable. No provider request is made when no valid text is detected.

## Lightweight by design

Low resource use is a core product feature, not an afterthought:

- The always-running resident is native Rust and Win32, with no Electron, WebView, browser engine, async runtime, or bundled OCR model.
- Settings, prompts, and history live in a separate manager process that starts only when opened and exits when its last window closes.
- OCR captures only a bounded in-memory region when it is needed; screenshots are not saved to disk.
- SQLite is opened only for short background history writes, and completed history is capped at the newest 1,000 entries.
- Hover is opt-in, and extraction stops as soon as one path returns valid text.

The resident target with the manager closed is **below 20 MiB private working set** after warm-up. That target is still a pending five-minute release gate, so the preview does not yet claim a verified `<20 MiB` result; see [Verification status](docs/VERIFICATION.md).

## Install the preview

1. Download the Windows x64 ZIP from the GitHub release and extract it to a writable directory.
2. Run `selection-translate-manager.exe` and configure the API address and key as described below.
3. Run `selection-translate-resident.exe`. Its icon appears in the notification area.

Windows SmartScreen may warn because the binaries are not code-signed. User configuration and history are stored under `%LOCALAPPDATA%\SelectionTranslate`.

## Fill in the API address and key

Selection Translate expects an OpenAI-compatible Chat Completions API. Before first use, set two things on the manager's **Settings** page: the API base URL and the API key.

### API address (base URL)

1. Open `selection-translate-manager.exe` and go to the **Settings** tab.
2. Enter the base URL from your provider in the **API address** field, for example:

   ```text
   https://api.openai.com/v1
   ```

3. A host-only URL (such as `https://api.openai.com`) or a versioned URL ending in `/v1` both work. Do not append `/chat/completions` yourself; the application adds that route exactly once.
4. Enter the provider's exact model identifier (for example `gpt-4o-mini`) in **Model**. It must be a model the address supports.

### API key

1. Paste the key issued by your provider into the **API key** field on the Settings page.
2. Choose **Save key**, then **Save settings**.
3. Start (or restart) `selection-translate-resident.exe` for the change to take effect.

The key is stored as a Windows Credential Manager Generic Credential (default target name `SelectionTranslate/OpenAI`). It is never written to `config.toml`, history, or logs.

Environment variables are an alternative to saving a key: the resident reads credentials in the order `OPENAI_API_KEY`, `SELECTION_TRANSLATE_OPENAI_API_KEY`, then the Windows Credential Manager target.

To verify the configuration, select some text and choose a prompt profile: a streamed result means the address and key are valid. `Provider authentication failed` means the key is invalid; `Provider connection failed` means the address could not be reached — check the URL spelling, network, and proxy.
