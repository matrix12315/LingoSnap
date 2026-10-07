# Selection Translate

[English](README.md)

Selection Translate 是一款轻量级 Windows x64 划词与悬停文本助手。目标程序能够提供上下文时，它会自动获取目标所在的完整句子，将所选提示词发送到 OpenAI 兼容 API，并在紧凑弹窗中流式渲染 Markdown。常驻程序采用原生 Rust 和 Win32，不使用 Electron 或 WebView；管理器仅在需要时启动。

当前仓库提供未签名的预览版本。划词、手动和可选的悬停功能共用一条带优先级的提取管线：先尝试 UI Automation，仅在适用时回退到剪贴板或 Windows OCR。没有检测到有效文本时，程序不会发送任何 API 请求。

## 轻量是核心特性

低资源占用不是事后优化，而是产品的基础设计：

- 常驻程序使用原生 Rust 和 Win32，不包含 Electron、WebView、浏览器内核、异步运行时或内置 OCR 模型。
- 设置、提示词和历史记录由独立管理器负责；管理器只在打开时运行，最后一个窗口关闭后立即退出。
- OCR 只在需要时截取有限的内存区域，不把截图保存到磁盘。
- SQLite 只在后台短暂写入历史记录时打开，并且仅保留最近 1,000 条完成记录。
- 悬停功能默认关闭；任一提取路径获得有效文本后，其他路径立即停止。

关闭管理器并完成预热后，常驻程序的目标是**专用工作集低于 20 MiB**。五分钟内存测试仍是待完成的发布门槛，因此当前预览版不会宣称已经验证 `<20 MiB`；详见[验证状态](docs/VERIFICATION.md)。

## 安装预览版

1. 从 GitHub Release 下载 Windows x64 ZIP，并解压到可写目录。
2. 运行 `selection-translate-manager.exe`，按下文配置 API 地址和密钥。
3. 运行 `selection-translate-resident.exe`，通知区域中会出现程序图标。

由于程序尚未进行代码签名，Windows SmartScreen 可能显示警告。用户配置和历史记录位于 `%LOCALAPPDATA%\SelectionTranslate`。

## 填写 API 地址与密钥

程序使用与 OpenAI Chat Completions 兼容的 API。首次使用前需要在管理器的**设置**页完成两项配置：API 基础地址和 API key。

### API 地址（Base URL）

1. 打开 `selection-translate-manager.exe`，进入**设置**页。
2. 在 **API 地址**中填入服务商提供的基础地址，例如：

   ```text
   https://api.openai.com/v1
   ```

3. 纯主机地址（如 `https://api.openai.com`）和以 `/v1` 结尾的地址都可以。不要自己追加 `/chat/completions`，程序只会自动添加一次。
4. 在**模型**中填入服务商的准确模型标识，例如 `gpt-4o-mini`，需与该地址支持的模型一致。

### API Key（密钥）

1. 在设置页的 **API key** 输入框中粘贴服务商生成的密钥。
2. 点击**保存密钥**，再点击**保存设置**。
3. 启动（或重启）`selection-translate-resident.exe` 使配置生效。

密钥会保存为 Windows 凭据管理器中的通用凭据（默认目标名 `SelectionTranslate/OpenAI`），不会写入 `config.toml`、历史记录或日志。

也可以不改密钥而改用环境变量：常驻程序按 `OPENAI_API_KEY`、`SELECTION_TRANSLATE_OPENAI_API_KEY`、Windows 凭据管理器的顺序读取密钥。

配置完成后，选中一段文本并选择提示词即可验证：能正常返回结果说明地址和密钥均有效。如果提示 `Provider authentication failed`，说明密钥无效；如果提示 `Provider connection failed`，说明网络无法连接到所填地址，请检查地址拼写、网络和代理。
