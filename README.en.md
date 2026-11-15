# LingoSnap

[简体中文](README.md)

LingoSnap is an ultralight, native Windows x64 smart AI text-processing tool and productivity workbench. Select text, hover over it, or summon the floating input card anywhere on your screen — across IDEs, web browsers, PDF readers, office documents, and terminals — to instantly execute LLM-powered text processing workflows (academic writing polish, code analysis & debugging, encyclopedic concept explanation, bilingual translation & vocabulary breakdown, or custom prompt transformations) streamed directly into a non-intrusive floating popup.

Designed from the ground up to be featherlight and privacy-conscious, LingoSnap keeps its background resident purely native (Rust + Win32). It features a modular dual-skin presentation: a modern, paper-like **Fair Copy** interface with liquid-glass accents and dark/light themes (powered by an on-demand companion process), alongside a lightweight **Classic** native Win32 popup. The manager runs strictly on demand.

---

## Key Features

- **Context-Aware Text Extraction**: Extracts both target text and its surrounding sentence context (via UI Automation, with automatic fallbacks to clipboard and bounded Windows OCR) so the LLM disambiguates meaning accurately.
- **Rich Built-in & Custom Processing Profiles**:
  - **IELTS Writing Instructor** (English & Chinese): Academic English sentence polish (Band 7+) with concrete improvement rationales.
  - **Code Specialist**: Code snippet breakdown, flaw identification, and logic explanations.
  - **Wiki Explanation**: Encyclopedic background and factual context summary.
  - **Expert Linguist Analysis** (English & Chinese): Structured Markdown with translations, phonetic IPA vocabulary, grammar/idiom usage, spelling/grammar error correction, and nuance explanations.
  - **Custom Profiles**: Create and order custom prompts with flexible temperature and token limits for any text processing task.
- **Non-Intrusive Floating Popup**: Floats directly above your current workspace without stealing keyboard focus or disrupting your workflow. Supports pinning, streaming output, and one-click copying.
- **Privacy & Lightweight by Design**:
  - **No Electron or background WebView bloat**: The resident runs as a tiny native background service (<20 MiB idle memory target). The WebView2 UI companion is invoked on demand and parks/exits automatically when idle.
  - **Secure Credentials**: API keys are securely stored in Windows Credential Manager or environment variables — never saved to configuration files, logs, or history.
  - **Zero Screenshot Storage**: OCR uses only a temporary, bounded in-memory buffer; no screenshots are ever written to disk.
  - **Clipboard Preservation**: User clipboard contents are safely preserved and restored during text capture.
  - **Local History**: Stored in a local SQLite database capped to the newest 1,000 entries.

---

## Default Hotkeys

| Action | Shortcut | Description |
|---|---|---|
| **Selection Process** | `Ctrl + Shift + T` | Process currently selected text using the active profile |
| **Hover Inspect** | `Ctrl + Shift + H` | Toggle automatic hover inspection under mouse pointer |
| **Manual Query** | `Ctrl + Shift + Space` | Open an input card to type or paste text, then press Enter or choose a profile |
| **Cycle Profiles** | `Ctrl + Alt + P` | Cycle through configured prompt profiles |
| **Pause / Resume** | `Ctrl + Shift + P` | Temporarily pause or resume global resident hotkeys |

*All hotkeys can be customized or disabled in Manager Settings.*

---

## Installation

1. Download the latest `lingosnap-x64-r*.zip` release archive from GitHub.
2. Extract the archive into a permanent writable folder (e.g., `D:\Tools\LingoSnap`, `C:\Program Files\LingoSnap`, or your preferred directory).
3. Run `lingosnap-manager.exe` to configure your API endpoint and key (see the guide below).
4. Run `lingosnap-resident.exe`. Its icon will appear in the Windows system tray.

> [!NOTE]
> Windows SmartScreen may show an untrusted binary prompt because preview builds are unsigned. User settings and history are stored safely under `%LOCALAPPDATA%\LingoSnap`.

---

## Setup & API Configuration

LingoSnap connects to any OpenAI-compatible Chat Completions API. It ships pre-configured with Alibaba Cloud Bailian (阿里云百炼) defaults for convenient out-of-the-box use in China, and works seamlessly with OpenAI, DeepSeek, Moonshot, SiliconFlow, or local models (e.g., Ollama, vLLM).

### Quick Start with Alibaba Cloud Bailian (Recommended for China)

Alibaba Cloud Bailian provides generous free quotas for fast flash models (such as `qwen3.8-flash`).

1. Open `lingosnap-manager.exe`, go to the **Settings** page, and click **Open key page** next to the Endpoint field to navigate to the Bailian API-Key console.

   ![Open key page button in Manager Settings](assets/image.png)

2. Sign in or complete real-name verification if prompted, then click **Create API-Key** (创建 API-Key).

   ![Create API Key on Bailian](assets/image-1.png)
   ![Bailian API Key Dialog](assets/image-2.png)

3. Copy your generated API Key.

   ![Copy Bailian API Key](assets/image-3.png)

4. Return to LingoSnap Manager, paste the key into the **API key** field, and click **Save key**.

   ![Save API Key in Manager](assets/image-4.png)

5. **Quota Protection & Models**:
   - Check your free quota balance on the [Bailian Free Quota Page](https://bailian.console.aliyun.com/cn-beijing/costing-balance/free-quota).
   - > [!IMPORTANT]
     > Ensure the **"用完即停" (Stop when quota exceeded)** switch is turned **ON** in your Alibaba Cloud billing settings to prevent unintended charges once free quotas expire.

   ![Enable Stop on Quota Exceeded](assets/image-5.png)

6. **Select Model**:
   - In the Bailian model list, find your preferred model (e.g., `qwen3.8-flash`), copy its **Model Code (模型 Code)**, paste it into the **Model** field in LingoSnap Manager, and save.

   ![Copy Model Code into Manager](assets/image-6.png)

7. **Verify Presentation**:
   - Select text in any window to display the profile chooser, then click any profile to stream the structured result card:

   ![Profile Chooser](assets/image-7.png)
   ![Result Card](assets/image-8.png)

---

### Custom OpenAI-Compatible Provider

To use OpenAI, DeepSeek, or another provider:

1. Open `lingosnap-manager.exe` and navigate to **Settings**.
2. Enter your provider's base URL in the **API address** field:
   ```text
   https://api.openai.com/v1
   ```
   > [!TIP]
   > Both host URLs (`https://api.openai.com`) and versioned paths ending in `/v1` are supported. Do **not** append `/chat/completions`; LingoSnap automatically appends the endpoint route.
3. Enter the corresponding model identifier in **Model** (e.g., `gpt-4o-mini`, `deepseek-chat`).
4. Paste your API key into **API key**, click **Save key**, and click **Save settings**.
5. Restart `lingosnap-resident.exe` (or toggle Pause/Resume from the tray) for changes to take effect.

#### Alternative: Environment Variables
You can also supply credentials via environment variables without saving them in the Windows Credential Manager. LingoSnap searches in this order:
1. `OPENAI_API_KEY`
2. `LINGOSNAP_OPENAI_API_KEY`
3. Windows Credential Manager target (default `LingoSnap/bailian` or `LingoSnap/OpenAI`)

---

## Verifying Setup

Select a word or sentence in any application and press `Ctrl + Shift + T`. A streaming result card will pop up immediately:
- **Streaming response appears**: Connection and credentials are confirmed.
- **`Provider authentication failed`**: The API key is invalid, expired, or lacked permission.
- **`Provider connection failed`**: Network error or unreachable URL — check your internet connection, URL formatting, or proxy settings.

---

## Advanced Features & Practical Tips

### 1. Arrange Profile Order (Quick Access to Favorite Profiles)
In the Manager's **Profiles** or **Settings** page, you can reorder profiles via drag-and-drop:
- **Selection Chooser**: Higher-priority profiles appear first on the floating toolbar after selecting or hovering over text.
- **Manual Input Card**: When pressing `Ctrl + Shift + Space` to summon the manual query popup, the top **5 profiles** are presented directly on the embedded pill toolbar (using compact first-word titles) with your default profile pre-selected.

### 2. Auto-Start with Windows (Highly Recommended)
Enable **Start with Windows** in the Manager's **Settings** page:
- **Ultralight Background Footprint**: The resident process is written in pure native Rust and Win32, completely free from Electron or continuous browser memory consumption. Warmed idle private memory is typically `< 20 MiB` (often `< 10 MiB`), with zero CPU polling.
- **On-Demand Companion**: The modern Fair Copy presentation process launches on demand and sleeps or parks when idle. Auto-starting has practically zero impact on system boot time or daily gaming/productivity performance.

### 3. Inspect Existing Profiles to Design Custom Prompts
In the Manager's **Profiles** page, click on any built-in profile (such as *Linguist analysis*, *IELTS writing instructor*, or *Code specialist*):
- **Learn the Prompting Paradigm**: Examine how the official System Prompts and User Templates structure output using clear Markdown headings for predictable and reliable parsing.
- **Create Your Own Tools**: Click **Add profile** to craft custom workflows tailored to your needs (e.g., academic paper polishing, customer email drafting, line-by-line code review, domain-specific terminology translation) with fine-tuned temperature and max token settings.

### 4. Review Local History for Vocabulary & Study
In the Manager's **History** page:
- **Never Lose a Good Expression**: Every admitted selection, hover, or manual query target, along with its context and LLM analysis, is archived locally.
- **Search, Review, and Replay**: Filter entries by date, filter by profile, or search by keywords. Re-run text processing tasks or copy results with one click—ideal for review, refinement, and writing retrospectives.
- **100% Local & Private**: All history resides in a lightweight local SQLite database (capped to the newest 1,000 entries) and is never transmitted to any third-party cloud.

---

## ☕ Buy the Author a Coffee

If LingoSnap has helped improve your reading, writing, or learning workflow, consider buying the author a coffee to support continuous development!

| WeChat Pay (微信支付) | Alipay (支付宝) |
| :---: | :---: |
| <img src="assets/qr-wechat-pay-1.png" width="220" alt="WeChat Pay" /> | <img src="assets/qr-alipay-1.png" width="220" alt="Alipay" /> |

---

## Documentation

- [Setup Guide](docs/SETUP.md): Detailed installation and environment configuration.
- [Hotkeys Reference](docs/HOTKEYS.md): Comprehensive shortcut mappings and behavior.
- [Fallback Mechanisms](docs/FALLBACKS.md): How UI Automation, clipboard, and OCR extraction work together.
- [Privacy Policy](docs/PRIVACY.md): Credential safety, zero disk retention, and clipboard integrity.
- [Troubleshooting](docs/TROUBLESHOOTING.md): Common error codes and solutions.
- [Verification Guide](docs/VERIFICATION.md): Test gates, Clippy checks, and memory benchmarking.
- [WebView Memory Report](docs/WEBVIEW_MEMORY.md): Performance analysis and memory optimization for the Fair Copy UI.
