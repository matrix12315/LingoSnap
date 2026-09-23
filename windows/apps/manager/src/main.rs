#![cfg_attr(windows, windows_subsystem = "windows")]

//! The on-demand manager. It is a plain Win32 window: the resident process
//! never loads this executable, and no WebView/runtime is needed for editing
//! the small TOML configuration.

#[cfg(windows)]
mod windows_app {
    use selection_core::{
        default_config_path, save_atomic, AppConfig, ExtractionSource, PromptConfig, UiLanguage,
    };
    use selection_platform_windows::{
        app::{
            ensure_resident_running, notify_config_changed, notify_credentials_changed,
            RefreshOutcome, ResidentStartOutcome,
        },
        credentials, theme,
    };
    use selection_storage::{
        default_history_path, HistoryDatabase, HistoryEntry, HistoryOrder, HistoryQuery,
    };
    use std::ffi::c_void;
    use std::path::PathBuf;
    use windows::core::{w, Error, PCWSTR};
    use windows::Win32::Foundation::{
        COLORREF, HANDLE, HGLOBAL, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM,
    };
    use windows::Win32::Graphics::Gdi::{
        BeginPaint, CreateFontW, CreateRoundRectRgn, CreateSolidBrush, DeleteObject, DrawFocusRect,
        DrawTextW, EndPaint, FillRect, FillRgn, FrameRgn, InvalidateRect, SelectObject, SetBkColor,
        SetBkMode, SetTextCharacterExtra, SetTextColor, UpdateWindow, DRAW_TEXT_FORMAT,
        FONT_CHARSET, FONT_CLIP_PRECISION, FONT_OUTPUT_PRECISION, FONT_QUALITY, HBRUSH, HFONT,
        HGDIOBJ, TRANSPARENT,
    };
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::System::Ole::CF_UNICODETEXT;
    use windows::Win32::UI::Controls::{
        SetWindowTheme, DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODT_BUTTON, ODT_LISTBOX,
    };
    use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetDpiForWindow};
    use windows::Win32::UI::WindowsAndMessaging::{
        AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
        GetClientRect, GetDlgItem, GetMessageW, GetParent, GetWindowLongPtrW, GetWindowTextLengthW,
        GetWindowTextW, IsDialogMessageW, IsZoomed, MessageBoxW, PostQuitMessage, RegisterClassW,
        SendMessageW, SetWindowLongPtrW, SetWindowPos, SetWindowTextW, ShowWindow,
        TranslateMessage, BS_OWNERDRAW, BS_PUSHBUTTON, CREATESTRUCTW, ES_AUTOHSCROLL,
        ES_AUTOVSCROLL, ES_MULTILINE, ES_PASSWORD, GWLP_USERDATA, HTCAPTION, HTCLIENT, IDYES,
        MB_DEFBUTTON2, MB_ICONWARNING, MB_YESNO, MINMAXINFO, SC_CLOSE, SC_MAXIMIZE, SC_MINIMIZE,
        SC_RESTORE, SWP_NOACTIVATE, SWP_NOZORDER, SW_HIDE, SW_SHOW, WINDOW_STYLE, WM_CLOSE,
        WM_COMMAND, WM_CREATE, WM_DESTROY, WM_DPICHANGED, WM_DRAWITEM, WM_ERASEBKGND,
        WM_GETMINMAXINFO, WM_MEASUREITEM, WM_NCHITTEST, WM_NOTIFY, WM_PAINT, WM_SETFONT, WM_SIZE,
        WM_SYSCOMMAND, WNDCLASSW, WS_BORDER, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS,
        WS_EX_CONTROLPARENT, WS_POPUP, WS_TABSTOP, WS_THICKFRAME, WS_VISIBLE, WS_VSCROLL,
    };

    const CLASS_NAME: PCWSTR = w!("SelectionTranslateManager");
    const PAGE_CLASS: PCWSTR = w!("SelectionTranslateManagerPage");
    const ID_SETTINGS_TAB: usize = 100;
    const ID_PROMPTS_TAB: usize = 101;
    const ID_HISTORY_TAB: usize = 102;
    const ID_SAVE_SETTINGS: usize = 110;
    const ID_SAVE_KEY: usize = 111;
    const ID_DELETE_KEY: usize = 112;
    const ID_SAVE_PROMPT: usize = 120;
    const ID_HISTORY_PROMPT: usize = 141;
    const ID_HISTORY_SOURCE: usize = 142;
    const ID_HISTORY_ORDER: usize = 143;
    const ID_HISTORY_LIST: usize = 144;
    const ID_HISTORY_REFRESH: usize = 145;
    const ID_HISTORY_COPY: usize = 146;
    const ID_HISTORY_DELETE: usize = 147;
    const ID_WIN_MIN: usize = 180;
    const ID_WIN_MAX: usize = 181;
    const ID_WIN_CLOSE: usize = 182;
    const ID_LANGUAGE: usize = 148;
    // Stable child-control IDs are part of the manager's UI automation
    // surface.  Tests and assistive tools must not infer field identity from
    // mutable label/value text or child-window enumeration order.
    const ID_SETTINGS_PAGE: usize = 150;
    const ID_PROMPTS_PAGE: usize = 151;
    const ID_HISTORY_PAGE: usize = 152;
    const ID_SETTINGS_ENDPOINT: usize = 160;
    const ID_SETTINGS_MODEL: usize = 161;
    const ID_SETTINGS_CREDENTIAL_TARGET: usize = 162;
    const ID_SETTINGS_API_KEY: usize = 163;
    const ID_SETTINGS_SELECTION_DEFAULT: usize = 164;
    const ID_SETTINGS_HOVER_DEFAULT: usize = 165;
    const ID_PROMPT_ID: usize = 166;
    const ID_PROMPT_NAME: usize = 167;
    const ID_PROMPT_SYSTEM: usize = 168;
    const ID_PROMPT_USER_TEMPLATE: usize = 169;
    const ID_PROMPT_MODEL: usize = 170;
    const ID_PROMPT_TEMPERATURE: usize = 171;
    const ID_PROMPT_MAX_TOKENS: usize = 172;
    const ID_HISTORY_SEARCH: usize = 175;

    const DEFAULT_DPI: u32 = 96;
    /// Client title strip height (accent mark + window title).
    const TITLE_BAR_HEIGHT: i32 = 36;
    /// Horizontal tab bar height under the title strip.
    const TAB_BAR_HEIGHT: i32 = 45;
    /// Content chrome above the page area.
    const CHROME_HEIGHT: i32 = TITLE_BAR_HEIGHT + TAB_BAR_HEIGHT;
    const MIN_CONTENT_WIDTH: i32 = 780;
    const MIN_CLIENT_HEIGHT: i32 = 560;
    const DEFAULT_CLIENT_WIDTH: i32 = 980;
    /// Title strip + mockup shell-body (~720).
    const DEFAULT_CLIENT_HEIGHT: i32 = TITLE_BAR_HEIGHT + 720;
    const PAGE_PAD_X: i32 = 20;
    const PAGE_PAD_Y: i32 = 16;
    const GROUP_GAP: i32 = 10;
    const HISTORY_LIST_WIDTH: i32 = 280;
    const HISTORY_ROW_HEIGHT: i32 = 58;
    const BUTTON_HEIGHT: i32 = 36;
    const BUTTON_SMALL: i32 = 32;
    const INPUT_HEIGHT: i32 = 36;
    const WELL_RADIUS: i32 = 6;

    const LB_ADDSTRING: u32 = 0x0180;
    const LB_RESETCONTENT: u32 = 0x0184;
    const LB_GETCURSEL: u32 = 0x0188;
    const LBN_SELCHANGE: usize = 1;
    const CBN_SELCHANGE: usize = 1;
    const CB_ADDSTRING: u32 = 0x0143;
    const CB_RESETCONTENT: u32 = 0x014B;
    const CB_SETCURSEL: u32 = 0x014E;
    const CB_GETCURSEL: u32 = 0x0147;
    const LBS_NOTIFY: u32 = 0x0001;
    const LBS_OWNERDRAWVARIABLE: u32 = 0x0020;
    const LBS_HASSTRINGS: u32 = 0x0040;
    const LBS_NOINTEGRALHEIGHT: u32 = 0x0100;
    const CBS_DROPDOWN: u32 = 0x0002;
    const CBS_DROPDOWNLIST: u32 = 0x0003;
    const EM_SETREADONLY: u32 = 0x00CF;
    /// EM_SETCUEBANNER — grey placeholder in empty edit controls.
    const EM_SETCUEBANNER: u32 = 0x1501;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalFree(hmem: HGLOBAL) -> HGLOBAL;
    }

    #[derive(Clone, Copy, Eq, PartialEq)]
    enum View {
        Settings,
        Prompts,
        History,
    }

    /// Visual role of an owner-drawn button (DEVELOP_GUIDE §3.1).
    #[derive(Clone, Copy, Eq, PartialEq)]
    enum ButtonKind {
        Primary,
        Default,
        Danger,
        Tab,
    }

    /// Layout slot for a page child. Geometry is flex-computed from page size.
    #[derive(Clone, Copy, Eq, PartialEq, Debug)]
    enum Slot {
        // Settings
        SettingsProviderCap,
        SettingsEndpointLabel,
        SettingsEndpoint,
        SettingsModelLabel,
        SettingsModel,
        SettingsCredentialTargetLabel,
        SettingsCredentialTarget,
        SettingsCredentialsCap,
        SettingsApiKeyLabel,
        SettingsApiKey,
        SettingsSaveKey,
        SettingsDeleteKey,
        SettingsCredentialStatus,
        SettingsCredentialHint,
        SettingsDefaultsCap,
        SettingsSelectionDefaultLabel,
        SettingsSelectionDefault,
        SettingsHoverDefaultLabel,
        SettingsHoverDefault,
        SettingsLanguageLabel,
        SettingsLanguage,
        SettingsSave,
        // Prompts
        PromptsIdLabel,
        PromptsId,
        PromptsNameLabel,
        PromptsName,
        PromptsModelLabel,
        PromptsModel,
        PromptsTemperatureLabel,
        PromptsTemperature,
        PromptsMaxTokensLabel,
        PromptsMaxTokens,
        PromptsSave,
        PromptsHint,
        PromptsSystemCap,
        PromptsSystemWell,
        PromptsUserCap,
        PromptsUserWell,
        PromptsStatus,
        // History
        HistorySearchCap,
        HistorySearch,
        HistoryRefresh,
        HistoryCopy,
        HistoryPromptLabel,
        HistoryPrompt,
        HistorySourceLabel,
        HistorySource,
        HistoryOrderLabel,
        HistoryOrder,
        HistoryEntriesCap,
        HistoryList,
        HistorySelectionCap,
        HistoryTarget,
        HistoryContext,
        HistoryMeta,
        HistoryOutputCap,
        HistoryOutput,
        HistoryHint,
        HistoryCount,
        HistoryDelete,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum TextKey {
        WindowTitle,
        Settings,
        Prompts,
        History,
        GroupProvider,
        GroupCredentials,
        GroupDefaults,
        GroupSearch,
        GroupEntries,
        Endpoint,
        Model,
        CredentialTarget,
        ApiKey,
        SaveKey,
        DeleteSavedKey,
        SelectionProfile,
        HoverProfile,
        InterfaceLanguage,
        SaveSettings,
        CredentialPrivacy,
        Id,
        Name,
        SystemPrompt,
        UserTemplate,
        ModelOverride,
        Temperature,
        MaxTokens,
        SavePrompt,
        PromptHint,
        SearchTargetOutput,
        Refresh,
        CopyOutput,
        Prompt,
        Source,
        Order,
        Selection,
        Output,
        Context,
        DeleteSelected,
        HistoryPrivacy,
        AllPrompts,
        AllSources,
        Hover,
        Clipboard,
        Ocr,
        Newest,
        Oldest,
        English,
        SimplifiedChinese,
        KeyPresentValueHidden,
    }

    const ALL_TEXT_KEYS: &[TextKey] = &[
        TextKey::WindowTitle,
        TextKey::Settings,
        TextKey::Prompts,
        TextKey::History,
        TextKey::GroupProvider,
        TextKey::GroupCredentials,
        TextKey::GroupDefaults,
        TextKey::GroupSearch,
        TextKey::GroupEntries,
        TextKey::Endpoint,
        TextKey::Model,
        TextKey::CredentialTarget,
        TextKey::ApiKey,
        TextKey::SaveKey,
        TextKey::DeleteSavedKey,
        TextKey::SelectionProfile,
        TextKey::HoverProfile,
        TextKey::InterfaceLanguage,
        TextKey::SaveSettings,
        TextKey::CredentialPrivacy,
        TextKey::Id,
        TextKey::Name,
        TextKey::SystemPrompt,
        TextKey::UserTemplate,
        TextKey::ModelOverride,
        TextKey::Temperature,
        TextKey::MaxTokens,
        TextKey::SavePrompt,
        TextKey::PromptHint,
        TextKey::SearchTargetOutput,
        TextKey::Refresh,
        TextKey::CopyOutput,
        TextKey::Prompt,
        TextKey::Source,
        TextKey::Order,
        TextKey::Selection,
        TextKey::Output,
        TextKey::Context,
        TextKey::DeleteSelected,
        TextKey::HistoryPrivacy,
        TextKey::AllPrompts,
        TextKey::AllSources,
        TextKey::Hover,
        TextKey::Clipboard,
        TextKey::Ocr,
        TextKey::Newest,
        TextKey::Oldest,
        TextKey::English,
        TextKey::SimplifiedChinese,
        TextKey::KeyPresentValueHidden,
    ];

    fn ui_text(language: UiLanguage, key: TextKey) -> &'static str {
        use TextKey::*;
        match (language, key) {
            (UiLanguage::English, WindowTitle) => "Selection Translate — Manager",
            (UiLanguage::English, Settings) => "Settings",
            (UiLanguage::English, Prompts) => "Prompts",
            (UiLanguage::English, History) => "History",
            (UiLanguage::English, GroupProvider) => "Provider",
            (UiLanguage::English, GroupCredentials) => "Credentials",
            (UiLanguage::English, GroupDefaults) => "Defaults",
            (UiLanguage::English, GroupSearch) => "Search",
            (UiLanguage::English, GroupEntries) => "Entries",
            (UiLanguage::English, Endpoint) => "Endpoint",
            (UiLanguage::English, Model) => "Model",
            (UiLanguage::English, CredentialTarget) => "Credential target",
            (UiLanguage::English, ApiKey) => "API key",
            (UiLanguage::English, SaveKey) => "Save key",
            (UiLanguage::English, DeleteSavedKey) => "Delete saved key",
            (UiLanguage::English, SelectionProfile) => "Selection profile",
            (UiLanguage::English, HoverProfile) => "Hover profile",
            (UiLanguage::English, InterfaceLanguage) => "Interface language",
            (UiLanguage::English, SaveSettings) => "Save settings",
            (UiLanguage::English, CredentialPrivacy) => {
                "Keys are held by Windows Credential Manager; they never enter config.toml."
            }
            (UiLanguage::English, Id) => "ID",
            (UiLanguage::English, Name) => "Name",
            (UiLanguage::English, SystemPrompt) => "System prompt",
            (UiLanguage::English, UserTemplate) => "User template",
            (UiLanguage::English, ModelOverride) => "Model override",
            (UiLanguage::English, Temperature) => "Temperature",
            (UiLanguage::English, MaxTokens) => "Max tokens",
            (UiLanguage::English, SavePrompt) => "Save prompt",
            (UiLanguage::English, PromptHint) => {
                "Use {target}, {context}, and {source}; every user template needs {target}."
            }
            (UiLanguage::English, SearchTargetOutput) => "Search target/output",
            (UiLanguage::English, Refresh) => "Refresh",
            (UiLanguage::English, CopyOutput) => "Copy output",
            (UiLanguage::English, Prompt) => "Prompt",
            (UiLanguage::English, Source) => "Source",
            (UiLanguage::English, Order) => "Order",
            (UiLanguage::English, Selection) => "Selection",
            (UiLanguage::English, Output) => "Output",
            (UiLanguage::English, Context) => "Context",
            (UiLanguage::English, DeleteSelected) => "Delete selected",
            (UiLanguage::English, HistoryPrivacy) => {
                "History is loaded only while this tab is open; the database is never held open."
            }
            (UiLanguage::English, AllPrompts) => "All prompts",
            (UiLanguage::English, AllSources) => "All sources",
            (UiLanguage::English, Hover) => "Hover",
            (UiLanguage::English, Clipboard) => "Clipboard",
            (UiLanguage::English, Ocr) => "OCR",
            (UiLanguage::English, Newest) => "Newest",
            (UiLanguage::English, Oldest) => "Oldest",
            (UiLanguage::English, English) => "English",
            (UiLanguage::English, SimplifiedChinese) => "Simplified Chinese",
            (UiLanguage::English, KeyPresentValueHidden) => "Key present · value hidden",
            (UiLanguage::SimplifiedChinese, WindowTitle) => "划词翻译 — 管理器",
            (UiLanguage::SimplifiedChinese, Settings) => "设置",
            (UiLanguage::SimplifiedChinese, Prompts) => "提示词",
            (UiLanguage::SimplifiedChinese, History) => "历史记录",
            (UiLanguage::SimplifiedChinese, GroupProvider) => "服务商",
            (UiLanguage::SimplifiedChinese, GroupCredentials) => "凭据",
            (UiLanguage::SimplifiedChinese, GroupDefaults) => "默认",
            (UiLanguage::SimplifiedChinese, GroupSearch) => "搜索",
            (UiLanguage::SimplifiedChinese, GroupEntries) => "条目",
            (UiLanguage::SimplifiedChinese, Endpoint) => "端点",
            (UiLanguage::SimplifiedChinese, Model) => "模型",
            (UiLanguage::SimplifiedChinese, CredentialTarget) => "凭据目标",
            (UiLanguage::SimplifiedChinese, ApiKey) => "API 密钥",
            (UiLanguage::SimplifiedChinese, SaveKey) => "保存密钥",
            (UiLanguage::SimplifiedChinese, DeleteSavedKey) => "删除已存密钥",
            (UiLanguage::SimplifiedChinese, SelectionProfile) => "划词配置",
            (UiLanguage::SimplifiedChinese, HoverProfile) => "悬停配置",
            (UiLanguage::SimplifiedChinese, InterfaceLanguage) => "界面语言",
            (UiLanguage::SimplifiedChinese, SaveSettings) => "保存设置",
            (UiLanguage::SimplifiedChinese, CredentialPrivacy) => {
                "密钥保存在 Windows 凭据管理器中，绝不会写入 config.toml。"
            }
            (UiLanguage::SimplifiedChinese, Id) => "ID",
            (UiLanguage::SimplifiedChinese, Name) => "名称",
            (UiLanguage::SimplifiedChinese, SystemPrompt) => "系统提示词",
            (UiLanguage::SimplifiedChinese, UserTemplate) => "用户模板",
            (UiLanguage::SimplifiedChinese, ModelOverride) => "模型覆盖",
            (UiLanguage::SimplifiedChinese, Temperature) => "温度",
            (UiLanguage::SimplifiedChinese, MaxTokens) => "最大令牌数",
            (UiLanguage::SimplifiedChinese, SavePrompt) => "保存提示词",
            (UiLanguage::SimplifiedChinese, PromptHint) => {
                "可使用 {target}、{context} 和 {source}；用户模板必须包含 {target}。"
            }
            (UiLanguage::SimplifiedChinese, SearchTargetOutput) => "搜索目标/输出",
            (UiLanguage::SimplifiedChinese, Refresh) => "刷新",
            (UiLanguage::SimplifiedChinese, CopyOutput) => "复制输出",
            (UiLanguage::SimplifiedChinese, Prompt) => "提示词",
            (UiLanguage::SimplifiedChinese, Source) => "来源",
            (UiLanguage::SimplifiedChinese, Order) => "排序",
            (UiLanguage::SimplifiedChinese, Selection) => "划词",
            (UiLanguage::SimplifiedChinese, Output) => "输出",
            (UiLanguage::SimplifiedChinese, Context) => "上下文",
            (UiLanguage::SimplifiedChinese, DeleteSelected) => "删除所选项",
            (UiLanguage::SimplifiedChinese, HistoryPrivacy) => {
                "仅在此页面打开历史数据库，离开后立即关闭。"
            }
            (UiLanguage::SimplifiedChinese, AllPrompts) => "全部提示词",
            (UiLanguage::SimplifiedChinese, AllSources) => "全部来源",
            (UiLanguage::SimplifiedChinese, Hover) => "悬停",
            (UiLanguage::SimplifiedChinese, Clipboard) => "剪贴板",
            (UiLanguage::SimplifiedChinese, Ocr) => "OCR",
            (UiLanguage::SimplifiedChinese, Newest) => "最新优先",
            (UiLanguage::SimplifiedChinese, Oldest) => "最早优先",
            (UiLanguage::SimplifiedChinese, English) => "English",
            (UiLanguage::SimplifiedChinese, SimplifiedChinese) => "简体中文",
            (UiLanguage::SimplifiedChinese, KeyPresentValueHidden) => "密钥已保存 · 内容已隐藏",
        }
    }

    /// Every manager-authored status, validation error, and confirmation message is represented
    /// by this catalog before it is rendered.  Provider output, history fields, and OS/Rust error
    /// details are deliberately passed as opaque values and are never translated.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum StatusEvent<'a> {
        ManagerInitializationFailed {
            detail: &'a str,
        },
        ConfigLoadFailed {
            detail: &'a str,
        },
        LocalAppDataUnavailable {
            operation: StatusOperation,
        },
        SaveInterfaceLanguageFailed {
            detail: &'a str,
        },
        InterfaceLanguageSaved,
        HistoryRefreshed,
        HistoryUnavailable {
            detail: &'a str,
        },
        SelectHistoryEntry,
        HistoryEntryUnavailable,
        OutputCopied,
        CopyOutputFailed {
            detail: &'a str,
        },
        DeleteHistoryConfirm {
            target: &'a str,
        },
        DeletionCancelled,
        HistoryEntryDeleted,
        HistoryEntryAlreadyDeleted,
        DeleteHistoryFailed {
            detail: &'a str,
        },
        OutputTooLarge,
        ClipboardMemoryLockFailed,
        ResidentStart(ResidentStartOutcome),
        ConfigRefresh(RefreshOutcome),
        CredentialRefresh {
            outcome: RefreshOutcome,
            deleted: bool,
        },
        CannotSaveSettings {
            detail: &'a str,
        },
        EnterApiKey,
        ApiKeySavedToCredentialManager,
        ApiKeyInactiveTargetSaved,
        SaveApiKeyFailed {
            detail: &'a str,
        },
        NoSavedApiKey,
        ApiKeyInactiveTargetDeleted,
        DeleteApiKeyFailed {
            detail: &'a str,
        },
        NoPromptProfile,
        CannotSavePrompt {
            detail: &'a str,
        },
        PromptInvalid {
            detail: &'a str,
        },
        InvalidTemperature,
        InvalidMaxOutputTokens,
        ConfigPathUnavailable,
        CredentialStatusPresent,
        CredentialStatusAbsent,
        CredentialStatusUnavailable {
            detail: &'a str,
        },
        HistoryCount {
            count: usize,
        },
        NoContext,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum StatusOperation {
        Save,
        History,
    }

    fn status_text(language: UiLanguage, event: StatusEvent<'_>) -> String {
        use StatusEvent::*;
        match event {
            ManagerInitializationFailed { detail } => match language {
                UiLanguage::English => format!("Manager initialization failed: {detail}"),
                UiLanguage::SimplifiedChinese => format!("管理器初始化失败：{detail}"),
            },
            ConfigLoadFailed { detail } => match language {
                UiLanguage::English => format!("Could not load config: {detail}"),
                UiLanguage::SimplifiedChinese => format!("无法加载配置：{detail}"),
            },
            LocalAppDataUnavailable { operation } => match (language, operation) {
                (UiLanguage::English, StatusOperation::Save) => "LOCALAPPDATA is not available; changes cannot be saved".to_owned(),
                (UiLanguage::SimplifiedChinese, StatusOperation::Save) => "LOCALAPPDATA 不可用；无法保存更改".to_owned(),
                (UiLanguage::English, StatusOperation::History) => "LOCALAPPDATA is not available; history cannot be opened".to_owned(),
                (UiLanguage::SimplifiedChinese, StatusOperation::History) => "LOCALAPPDATA 不可用；无法打开历史记录".to_owned(),
            },
            SaveInterfaceLanguageFailed { detail } => match language {
                UiLanguage::English => format!("Could not save interface language: {detail}"),
                UiLanguage::SimplifiedChinese => format!("无法保存界面语言：{detail}"),
            },
            InterfaceLanguageSaved => match language {
                UiLanguage::English => "Interface language saved.".to_owned(),
                UiLanguage::SimplifiedChinese => "界面语言已保存。".to_owned(),
            },
            HistoryRefreshed => match language {
                UiLanguage::English => "History refreshed.".to_owned(),
                UiLanguage::SimplifiedChinese => "历史记录已刷新。".to_owned(),
            },
            HistoryUnavailable { detail } => match language {
                UiLanguage::English => format!("History unavailable: {detail}"),
                UiLanguage::SimplifiedChinese => format!("历史记录不可用：{detail}"),
            },
            SelectHistoryEntry => match language {
                UiLanguage::English => "Select a history entry first.".to_owned(),
                UiLanguage::SimplifiedChinese => "请先选择一条历史记录。".to_owned(),
            },
            HistoryEntryUnavailable => match language {
                UiLanguage::English => "The selected history entry is no longer available.".to_owned(),
                UiLanguage::SimplifiedChinese => "所选历史记录已不可用。".to_owned(),
            },
            OutputCopied => match language {
                UiLanguage::English => "Output copied to the clipboard.".to_owned(),
                UiLanguage::SimplifiedChinese => "输出已复制到剪贴板。".to_owned(),
            },
            CopyOutputFailed { detail } => match language {
                UiLanguage::English => format!("Could not copy output: {detail}"),
                UiLanguage::SimplifiedChinese => format!("无法复制输出：{detail}"),
            },
            DeleteHistoryConfirm { target } => match language {
                UiLanguage::English => format!("Delete this history entry?\n\n{target}"),
                UiLanguage::SimplifiedChinese => format!("删除这条历史记录？\n\n{target}"),
            },
            DeletionCancelled => match language {
                UiLanguage::English => "Deletion cancelled.".to_owned(),
                UiLanguage::SimplifiedChinese => "已取消删除。".to_owned(),
            },
            HistoryEntryDeleted => match language {
                UiLanguage::English => "History entry deleted.".to_owned(),
                UiLanguage::SimplifiedChinese => "历史记录已删除。".to_owned(),
            },
            HistoryEntryAlreadyDeleted => match language {
                UiLanguage::English => "The history entry was already deleted.".to_owned(),
                UiLanguage::SimplifiedChinese => "该历史记录已被删除。".to_owned(),
            },
            DeleteHistoryFailed { detail } => match language {
                UiLanguage::English => format!("Could not delete history entry: {detail}"),
                UiLanguage::SimplifiedChinese => format!("无法删除历史记录：{detail}"),
            },
            OutputTooLarge => match language {
                UiLanguage::English => "output is too large".to_owned(),
                UiLanguage::SimplifiedChinese => "输出内容过大".to_owned(),
            },
            ClipboardMemoryLockFailed => match language {
                UiLanguage::English => "could not lock clipboard memory".to_owned(),
                UiLanguage::SimplifiedChinese => "无法锁定剪贴板内存".to_owned(),
            },
            ResidentStart(outcome) => match (language, outcome) {
                (UiLanguage::English, ResidentStartOutcome::AlreadyRunning) => "Resident is running.".to_owned(),
                (UiLanguage::English, ResidentStartOutcome::Started) => "Resident started and is ready.".to_owned(),
                (UiLanguage::English, ResidentStartOutcome::Unavailable) => "Resident could not be reached; settings can be saved, but translation is unavailable.".to_owned(),
                (UiLanguage::SimplifiedChinese, ResidentStartOutcome::AlreadyRunning) => "驻留程序正在运行。".to_owned(),
                (UiLanguage::SimplifiedChinese, ResidentStartOutcome::Started) => "驻留程序已启动并准备就绪。".to_owned(),
                (UiLanguage::SimplifiedChinese, ResidentStartOutcome::Unavailable) => "无法连接驻留程序；可以保存设置，但翻译功能当前不可用。".to_owned(),
            },
            ConfigRefresh(outcome) => match (language, outcome) {
                (UiLanguage::English, RefreshOutcome::Acknowledged) => "Saved; the running resident confirmed the refresh.".to_owned(),
                (UiLanguage::English, RefreshOutcome::ResidentAbsent) => "Saved, but no running resident was found; translation is unavailable.".to_owned(),
                (UiLanguage::English, RefreshOutcome::Unacknowledged) => "Saved, but the resident did not confirm the refresh; restart the app before translating.".to_owned(),
                (UiLanguage::English, RefreshOutcome::Rejected) => "Saved, but the resident rejected the refresh; restart the app before translating.".to_owned(),
                (UiLanguage::SimplifiedChinese, RefreshOutcome::Acknowledged) => "已保存；正在运行的驻留程序已确认刷新。".to_owned(),
                (UiLanguage::SimplifiedChinese, RefreshOutcome::ResidentAbsent) => "已保存，但未找到正在运行的驻留程序；翻译功能不可用。".to_owned(),
                (UiLanguage::SimplifiedChinese, RefreshOutcome::Unacknowledged) => "已保存，但驻留程序未确认刷新；请在翻译前重启应用。".to_owned(),
                (UiLanguage::SimplifiedChinese, RefreshOutcome::Rejected) => "已保存，但驻留程序拒绝刷新；请在翻译前重启应用。".to_owned(),
            },
            CredentialRefresh { outcome, deleted } => {
                let (en, zh) = match (outcome, deleted) {
                    (RefreshOutcome::Acknowledged, false) => ("API key saved; the running resident confirmed the credential refresh.", "API 密钥已保存；正在运行的驻留程序已确认凭据刷新。"),
                    (RefreshOutcome::Acknowledged, true) => ("API key deleted; the running resident confirmed the credential refresh.", "API 密钥已删除；正在运行的驻留程序已确认凭据刷新。"),
                    (RefreshOutcome::ResidentAbsent, false) => ("API key saved, but no running resident was found; translation is unavailable.", "API 密钥已保存，但未找到正在运行的驻留程序；翻译功能不可用。"),
                    (RefreshOutcome::ResidentAbsent, true) => ("API key deleted, but no running resident was found; translation is unavailable.", "API 密钥已删除，但未找到正在运行的驻留程序；翻译功能不可用。"),
                    (RefreshOutcome::Unacknowledged | RefreshOutcome::Rejected, false) => ("API key saved, but the resident did not confirm the credential refresh; restart the app.", "API 密钥已保存，但驻留程序未确认凭据刷新；请重启应用。"),
                    (RefreshOutcome::Unacknowledged | RefreshOutcome::Rejected, true) => ("API key deleted, but the resident did not confirm the credential refresh; restart the app.", "API 密钥已删除，但驻留程序未确认凭据刷新；请重启应用。"),
                };
                match language { UiLanguage::English => en.to_owned(), UiLanguage::SimplifiedChinese => zh.to_owned() }
            }
            CannotSaveSettings { detail } => match language {
                UiLanguage::English => format!("Cannot save settings: {detail}"),
                UiLanguage::SimplifiedChinese => format!("无法保存设置：{detail}"),
            },
            EnterApiKey => match language { UiLanguage::English => "Enter a key before saving it.".to_owned(), UiLanguage::SimplifiedChinese => "请输入密钥后再保存。".to_owned() },
            ApiKeySavedToCredentialManager => match language { UiLanguage::English => "Saved in Windows Credential Manager.".to_owned(), UiLanguage::SimplifiedChinese => "已保存到 Windows 凭据管理器。".to_owned() },
            ApiKeyInactiveTargetSaved => match language { UiLanguage::English => "API key saved, but this credential target is not active; save settings to refresh the resident.".to_owned(), UiLanguage::SimplifiedChinese => "API 密钥已保存，但此凭据目标未启用；请保存设置以刷新驻留程序。".to_owned() },
            SaveApiKeyFailed { detail } => match language { UiLanguage::English => format!("Could not save API key: {detail}"), UiLanguage::SimplifiedChinese => format!("无法保存 API 密钥：{detail}") },
            NoSavedApiKey => match language { UiLanguage::English => "No saved key for this target.".to_owned(), UiLanguage::SimplifiedChinese => "此目标没有已保存的密钥。".to_owned() },
            ApiKeyInactiveTargetDeleted => match language { UiLanguage::English => "API key deleted, but this credential target is not active; the resident was not refreshed.".to_owned(), UiLanguage::SimplifiedChinese => "API 密钥已删除，但此凭据目标未启用；未刷新驻留程序。".to_owned() },
            DeleteApiKeyFailed { detail } => match language { UiLanguage::English => format!("Could not delete API key: {detail}"), UiLanguage::SimplifiedChinese => format!("无法删除 API 密钥：{detail}") },
            NoPromptProfile => match language { UiLanguage::English => "There is no prompt profile to save.".to_owned(), UiLanguage::SimplifiedChinese => "没有可保存的提示词配置。".to_owned() },
            CannotSavePrompt { detail } => match language { UiLanguage::English => format!("Cannot save prompt: {detail}"), UiLanguage::SimplifiedChinese => format!("无法保存提示词：{detail}") },
            PromptInvalid { detail } => match language { UiLanguage::English => format!("Prompt is invalid: {detail}"), UiLanguage::SimplifiedChinese => format!("提示词无效：{detail}") },
            InvalidTemperature => match language { UiLanguage::English => "Temperature must be a number from 0 to 2.".to_owned(), UiLanguage::SimplifiedChinese => "温度必须是 0 到 2 之间的数字。".to_owned() },
            InvalidMaxOutputTokens => match language { UiLanguage::English => "Max output tokens must be a positive integer.".to_owned(), UiLanguage::SimplifiedChinese => "最大输出令牌数必须是正整数。".to_owned() },
            ConfigPathUnavailable => match language { UiLanguage::English => "LOCALAPPDATA is not available".to_owned(), UiLanguage::SimplifiedChinese => "LOCALAPPDATA 不可用".to_owned() },
            CredentialStatusPresent => match language {
                UiLanguage::English => "Key present · value hidden".to_owned(),
                UiLanguage::SimplifiedChinese => "密钥已保存 · 内容已隐藏".to_owned(),
            },
            CredentialStatusAbsent => match language { UiLanguage::English => "No saved key for this target.".to_owned(), UiLanguage::SimplifiedChinese => "此目标没有已保存的密钥。".to_owned() },
            CredentialStatusUnavailable { detail } => match language { UiLanguage::English => format!("Credential status unavailable: {detail}"), UiLanguage::SimplifiedChinese => format!("凭据状态不可用：{detail}") },
            HistoryCount { count } => match language { UiLanguage::English if count == 1 => "1 entry".to_owned(), UiLanguage::English => format!("{count} entries"), UiLanguage::SimplifiedChinese => format!("{count} 条记录") },
            NoContext => match language {
                UiLanguage::English => "(no context)".to_owned(),
                UiLanguage::SimplifiedChinese => "（无上下文）".to_owned(),
            },
        }
    }

    fn text_key_from_english(value: &str) -> Option<TextKey> {
        ALL_TEXT_KEYS
            .iter()
            .copied()
            .find(|key| ui_text(UiLanguage::English, *key) == value)
    }

    #[derive(Clone, Copy)]
    struct ManagerLayout {
        title_bar: RECT,
        tab_bar: RECT,
        page: RECT,
        accent_mark: RECT,
        title_text: RECT,
        tab_buttons: [RECT; 3],
        status_dot: RECT,
        status_text: RECT,
    }

    impl ManagerLayout {
        fn for_client(width: i32, height: i32) -> Self {
            let w = width.max(1);
            let h = height.max(1);
            let title_bar = RECT {
                left: 0,
                top: 0,
                right: w,
                bottom: TITLE_BAR_HEIGHT,
            };
            let tab_bar = RECT {
                left: 0,
                top: TITLE_BAR_HEIGHT,
                right: w,
                bottom: TITLE_BAR_HEIGHT + TAB_BAR_HEIGHT,
            };
            let page = RECT {
                left: 0,
                top: CHROME_HEIGHT,
                right: w,
                bottom: h,
            };
            let accent_mark = RECT {
                left: 12,
                top: 11,
                right: 26,
                bottom: 25,
            };
            let title_text = RECT {
                left: 34,
                top: 8,
                right: w - 12,
                bottom: TITLE_BAR_HEIGHT - 4,
            };
            let tab_left = 12;
            let tab_width = 96;
            let tab_gap = 4;
            let tab_top = TITLE_BAR_HEIGHT + 6;
            let tab_bottom = tab_top + 33;
            let tab_buttons = [0, 1, 2].map(|index| RECT {
                left: tab_left + index * (tab_width + tab_gap),
                top: tab_top,
                right: tab_left + index * (tab_width + tab_gap) + tab_width,
                bottom: tab_bottom,
            });
            let status_text = RECT {
                left: tab_left + 3 * (tab_width + tab_gap) + 12,
                top: tab_top,
                right: w - 28,
                bottom: tab_bottom,
            };
            let status_dot = RECT {
                left: w - 24,
                top: TITLE_BAR_HEIGHT + (TAB_BAR_HEIGHT - 8) / 2,
                right: w - 16,
                bottom: TITLE_BAR_HEIGHT + (TAB_BAR_HEIGHT - 8) / 2 + 8,
            };
            Self {
                title_bar,
                tab_bar,
                page,
                accent_mark,
                title_text,
                tab_buttons,
                status_dot,
                status_text,
            }
        }
    }

    /// Flex layout for one page. Coordinates are page-local logical pixels.
    #[derive(Clone, Copy)]
    struct PageLayout {
        groups: [RECT; 3],
        slots: [(Slot, RECT); 40],
        slot_count: usize,
    }

    impl PageLayout {
        fn rect_for(&self, slot: Slot) -> Option<RECT> {
            self.slots[..self.slot_count]
                .iter()
                .find(|(s, _)| *s == slot)
                .map(|(_, r)| *r)
        }

        fn push(&mut self, slot: Slot, rect: RECT) {
            if self.slot_count < self.slots.len() {
                self.slots[self.slot_count] = (slot, rect);
                self.slot_count += 1;
            }
        }
    }

    fn rect(x: i32, y: i32, w: i32, h: i32) -> RECT {
        RECT {
            left: x,
            top: y,
            right: x + w.max(1),
            bottom: y + h.max(1),
        }
    }

    fn layout_settings(width: i32, height: i32) -> PageLayout {
        let mut layout = PageLayout {
            groups: [rect(0, 0, 1, 1); 3],
            slots: [(Slot::SettingsSave, rect(0, 0, 1, 1)); 40],
            slot_count: 0,
        };
        let content_w = (width - PAGE_PAD_X * 2).max(240);
        let x = PAGE_PAD_X;
        let mut y = PAGE_PAD_Y;
        let label_h = 18;
        let row_gap = 8;
        let pad = 12;
        let cap_h = 20;

        // Provider
        let provider_h = pad + cap_h + (label_h + INPUT_HEIGHT + row_gap) * 3 - row_gap + pad;
        let provider = rect(x, y, content_w, provider_h);
        layout.groups[0] = provider;
        layout.push(
            Slot::SettingsProviderCap,
            rect(x + pad, y + pad - 2, content_w - pad * 2, cap_h),
        );
        let mut cy = y + pad + cap_h;
        for (label_slot, field_slot) in [
            (Slot::SettingsEndpointLabel, Slot::SettingsEndpoint),
            (Slot::SettingsModelLabel, Slot::SettingsModel),
            (
                Slot::SettingsCredentialTargetLabel,
                Slot::SettingsCredentialTarget,
            ),
        ] {
            layout.push(label_slot, rect(x + pad, cy, content_w - pad * 2, label_h));
            cy += label_h + 2;
            layout.push(
                field_slot,
                rect(x + pad, cy, content_w - pad * 2, INPUT_HEIGHT),
            );
            cy += INPUT_HEIGHT + row_gap;
        }
        y += provider_h + GROUP_GAP;

        // Credentials
        let cred_h = pad
            + cap_h
            + label_h
            + 2
            + INPUT_HEIGHT
            + row_gap
            + BUTTON_HEIGHT
            + row_gap
            + 22
            + 6
            + 34
            + pad;
        let credentials = rect(x, y, content_w, cred_h);
        layout.groups[1] = credentials;
        layout.push(
            Slot::SettingsCredentialsCap,
            rect(x + pad, y + pad - 2, content_w - pad * 2, cap_h),
        );
        let mut cy = y + pad + cap_h;
        layout.push(
            Slot::SettingsApiKeyLabel,
            rect(x + pad, cy, content_w - pad * 2, label_h),
        );
        cy += label_h + 2;
        layout.push(
            Slot::SettingsApiKey,
            rect(x + pad, cy, content_w - pad * 2 - 260, INPUT_HEIGHT),
        );
        layout.push(
            Slot::SettingsSaveKey,
            rect(x + pad + content_w - pad * 2 - 250, cy, 120, BUTTON_SMALL),
        );
        layout.push(
            Slot::SettingsDeleteKey,
            rect(x + pad + content_w - pad * 2 - 120, cy, 120, BUTTON_SMALL),
        );
        cy += INPUT_HEIGHT + row_gap;
        layout.push(
            Slot::SettingsCredentialStatus,
            rect(x + pad, cy, content_w - pad * 2, 22),
        );
        cy += 22 + 6;
        layout.push(
            Slot::SettingsCredentialHint,
            rect(x + pad, cy, content_w - pad * 2, 34),
        );
        y += cred_h + GROUP_GAP;

        // Defaults
        let defaults_h = pad + cap_h + (label_h + INPUT_HEIGHT + row_gap) * 3 - row_gap + pad;
        let defaults = rect(x, y, content_w, defaults_h);
        layout.groups[2] = defaults;
        layout.push(
            Slot::SettingsDefaultsCap,
            rect(x + pad, y + pad - 2, content_w - pad * 2, cap_h),
        );
        let mut cy = y + pad + cap_h;
        for (label_slot, field_slot) in [
            (
                Slot::SettingsSelectionDefaultLabel,
                Slot::SettingsSelectionDefault,
            ),
            (Slot::SettingsHoverDefaultLabel, Slot::SettingsHoverDefault),
            (Slot::SettingsLanguageLabel, Slot::SettingsLanguage),
        ] {
            layout.push(label_slot, rect(x + pad, cy, content_w - pad * 2, label_h));
            cy += label_h + 2;
            layout.push(
                field_slot,
                rect(x + pad, cy, content_w - pad * 2, INPUT_HEIGHT),
            );
            cy += INPUT_HEIGHT + row_gap;
        }
        y += defaults_h + GROUP_GAP;

        layout.push(Slot::SettingsSave, rect(x, y + 4, 160, BUTTON_HEIGHT));
        let _ = height;
        layout
    }

    fn layout_prompts(width: i32, height: i32) -> PageLayout {
        let mut layout = PageLayout {
            groups: [rect(0, 0, 1, 1); 3],
            slots: [(Slot::SettingsSave, rect(0, 0, 1, 1)); 40],
            slot_count: 0,
        };
        let content_w = (width - PAGE_PAD_X * 2).max(240);
        let x = PAGE_PAD_X;
        let mut y = PAGE_PAD_Y;
        let label_h = 16;
        let meta_h = label_h + 2 + INPUT_HEIGHT;

        // Meta row: ID | Name | Model | Temp | Max | Save
        let gap = 8;
        let id_w = 150;
        let model_w = 140;
        let temp_w = 100;
        let max_w = 110;
        let save_w = 130;
        let name_w = (content_w - id_w - model_w - temp_w - max_w - save_w - gap * 5).max(120);
        let mut cx = x;
        layout.push(Slot::PromptsIdLabel, rect(cx, y, id_w, label_h));
        layout.push(
            Slot::PromptsId,
            rect(cx, y + label_h + 2, id_w, INPUT_HEIGHT),
        );
        cx += id_w + gap;
        layout.push(Slot::PromptsNameLabel, rect(cx, y, name_w, label_h));
        layout.push(
            Slot::PromptsName,
            rect(cx, y + label_h + 2, name_w, INPUT_HEIGHT),
        );
        cx += name_w + gap;
        layout.push(Slot::PromptsModelLabel, rect(cx, y, model_w, label_h));
        layout.push(
            Slot::PromptsModel,
            rect(cx, y + label_h + 2, model_w, INPUT_HEIGHT),
        );
        cx += model_w + gap;
        layout.push(Slot::PromptsTemperatureLabel, rect(cx, y, temp_w, label_h));
        layout.push(
            Slot::PromptsTemperature,
            rect(cx, y + label_h + 2, temp_w, INPUT_HEIGHT),
        );
        cx += temp_w + gap;
        layout.push(Slot::PromptsMaxTokensLabel, rect(cx, y, max_w, label_h));
        layout.push(
            Slot::PromptsMaxTokens,
            rect(cx, y + label_h + 2, max_w, INPUT_HEIGHT),
        );
        cx += max_w + gap;
        layout.push(
            Slot::PromptsSave,
            rect(cx, y + label_h + 2, save_w, BUTTON_HEIGHT),
        );
        y += meta_h + 6;
        layout.push(Slot::PromptsHint, rect(x, y, content_w, 20));
        y += 20 + 8;
        layout.push(Slot::PromptsStatus, rect(x, y, content_w, 18));
        y += 18 + 6;

        // Two editors fill remaining height
        let editors_h = (height - y - PAGE_PAD_Y).max(180);
        let col_w = (content_w - GROUP_GAP) / 2;
        let cap_h = 28;
        let well_h = (editors_h - cap_h - 16).max(120);

        layout.groups[0] = rect(x, y, col_w, editors_h);
        layout.push(
            Slot::PromptsSystemCap,
            rect(x + 12, y + 8, col_w - 24, cap_h - 8),
        );
        layout.push(
            Slot::PromptsSystemWell,
            rect(x + 10, y + cap_h, col_w - 20, well_h),
        );

        let x2 = x + col_w + GROUP_GAP;
        layout.groups[1] = rect(x2, y, content_w - col_w, editors_h);
        layout.push(
            Slot::PromptsUserCap,
            rect(x2 + 12, y + 8, content_w - col_w - 24, cap_h - 8),
        );
        layout.push(
            Slot::PromptsUserWell,
            rect(x2 + 10, y + cap_h, content_w - col_w - 20, well_h),
        );
        layout.groups[2] = rect(0, 0, 1, 1);
        layout
    }

    fn layout_history(width: i32, height: i32) -> PageLayout {
        let mut layout = PageLayout {
            groups: [rect(0, 0, 1, 1); 3],
            slots: [(Slot::SettingsSave, rect(0, 0, 1, 1)); 40],
            slot_count: 0,
        };
        let content_w = (width - PAGE_PAD_X * 2).max(240);
        let x = PAGE_PAD_X;
        let mut y = PAGE_PAD_Y;
        let pad = 12;
        let cap_h = 20;
        let label_h = 16;

        // Search group
        let search_h = pad + cap_h + INPUT_HEIGHT + 8 + INPUT_HEIGHT + pad;
        layout.groups[0] = rect(x, y, content_w, search_h);
        layout.push(
            Slot::HistorySearchCap,
            rect(x + pad, y + pad - 2, content_w - pad * 2, cap_h),
        );
        let mut cy = y + pad + cap_h;
        let btn_refresh_w = 100;
        let btn_copy_w = 120;
        let search_w = content_w - pad * 2 - btn_refresh_w - btn_copy_w - 16;
        layout.push(
            Slot::HistorySearch,
            rect(x + pad, cy, search_w, INPUT_HEIGHT),
        );
        layout.push(
            Slot::HistoryRefresh,
            rect(x + pad + search_w + 8, cy + 2, btn_refresh_w, BUTTON_SMALL),
        );
        layout.push(
            Slot::HistoryCopy,
            rect(
                x + pad + search_w + 8 + btn_refresh_w + 8,
                cy + 2,
                btn_copy_w,
                BUTTON_SMALL,
            ),
        );
        cy += INPUT_HEIGHT + 8;
        let filter_w = (content_w - pad * 2 - 16) / 3;
        layout.push(
            Slot::HistoryPromptLabel,
            rect(x + pad, cy, filter_w, label_h),
        );
        layout.push(
            Slot::HistoryPrompt,
            rect(x + pad, cy, filter_w, INPUT_HEIGHT),
        );
        layout.push(
            Slot::HistorySourceLabel,
            rect(x + pad + filter_w + 8, cy, filter_w, label_h),
        );
        layout.push(
            Slot::HistorySource,
            rect(x + pad + filter_w + 8, cy, filter_w, INPUT_HEIGHT),
        );
        layout.push(
            Slot::HistoryOrderLabel,
            rect(x + pad + (filter_w + 8) * 2, cy, filter_w, label_h),
        );
        layout.push(
            Slot::HistoryOrder,
            rect(x + pad + (filter_w + 8) * 2, cy, filter_w, INPUT_HEIGHT),
        );
        y += search_h + GROUP_GAP;

        // Footer
        let footer_h = BUTTON_HEIGHT + 8;
        let entries_h = (height - y - PAGE_PAD_Y - footer_h - GROUP_GAP).max(220);
        layout.groups[1] = rect(x, y, content_w, entries_h);
        layout.push(
            Slot::HistoryEntriesCap,
            rect(x + pad, y + pad - 2, content_w - pad * 2, cap_h),
        );
        let split_y = y + pad + cap_h;
        let split_h = entries_h - pad * 2 - cap_h - 28;
        let list_x = x + pad;
        layout.push(
            Slot::HistoryList,
            rect(list_x, split_y, HISTORY_LIST_WIDTH, split_h),
        );
        let detail_x = list_x + HISTORY_LIST_WIDTH + GROUP_GAP;
        let detail_w = content_w - pad * 2 - HISTORY_LIST_WIDTH - GROUP_GAP;
        let sel_h = (split_h * 36 / 100).clamp(88, 160);
        layout.push(
            Slot::HistorySelectionCap,
            rect(detail_x, split_y, detail_w, 18),
        );
        layout.push(
            Slot::HistoryTarget,
            rect(detail_x, split_y + 20, detail_w, 36),
        );
        layout.push(
            Slot::HistoryContext,
            rect(detail_x, split_y + 60, detail_w, (sel_h - 80).max(36)),
        );
        layout.push(
            Slot::HistoryMeta,
            rect(detail_x, split_y + sel_h - 4, detail_w, 18),
        );
        let out_y = split_y + sel_h + 18;
        let out_h = (split_y + split_h - out_y).max(80);
        layout.push(Slot::HistoryOutputCap, rect(detail_x, out_y, detail_w, 18));
        layout.push(
            Slot::HistoryOutput,
            rect(detail_x, out_y + 20, detail_w, out_h - 20),
        );
        layout.push(
            Slot::HistoryHint,
            rect(x + pad, y + entries_h - 26, content_w - pad * 2, 20),
        );
        y += entries_h + GROUP_GAP;
        layout.push(Slot::HistoryCount, rect(x, y + 8, 160, 22));
        layout.push(Slot::HistoryDelete, rect(x + 170, y + 4, 150, BUTTON_SMALL));
        layout.groups[2] = rect(0, 0, 1, 1);
        layout
    }

    fn page_layout(view: View, width: i32, height: i32) -> PageLayout {
        match view {
            View::Settings => layout_settings(width, height),
            View::Prompts => layout_prompts(width, height),
            View::History => layout_history(width, height),
        }
    }

    #[derive(Clone, Copy)]
    struct ControlPlacement {
        hwnd: HWND,
        view: View,
        slot: Slot,
    }

    #[derive(Clone, Copy)]
    struct LocalizedControl {
        hwnd: HWND,
        key: TextKey,
    }

    struct ThemeResources {
        background: HBRUSH,
        surface: HBRUSH,
        raised: HBRUSH,
        line: HBRUSH,
        accent: HBRUSH,
        accent_dim: HBRUSH,
        accent_edge: HBRUSH,
        ok: HBRUSH,
        err: HBRUSH,
        err_fill: HBRUSH,
        body_font: HFONT,
        button_font: HFONT,
        caption_font: HFONT,
        title_font: HFONT,
        mono_font: HFONT,
        cjk_font: HFONT,
    }

    fn blend(fg: COLORREF, bg: COLORREF, alpha: f32) -> COLORREF {
        let a = alpha.clamp(0.0, 1.0);
        let fr = (fg.0 & 0xff) as f32;
        let fg_ = ((fg.0 >> 8) & 0xff) as f32;
        let fb = ((fg.0 >> 16) & 0xff) as f32;
        let br = (bg.0 & 0xff) as f32;
        let bg_ = ((bg.0 >> 8) & 0xff) as f32;
        let bb = ((bg.0 >> 16) & 0xff) as f32;
        let r = (fr * a + br * (1.0 - a)).round() as u32;
        let g = (fg_ * a + bg_ * (1.0 - a)).round() as u32;
        let b = (fb * a + bb * (1.0 - a)).round() as u32;
        COLORREF((b & 0xff) << 16 | (g & 0xff) << 8 | (r & 0xff))
    }

    impl ThemeResources {
        fn new(dpi: u32) -> Self {
            let create_font = |face: PCWSTR, size: i32, weight: i32| unsafe {
                CreateFontW(
                    -scale_for_dpi(size, dpi),
                    0,
                    0,
                    0,
                    weight,
                    0,
                    0,
                    0,
                    FONT_CHARSET(1),
                    FONT_OUTPUT_PRECISION(0),
                    FONT_CLIP_PRECISION(0),
                    FONT_QUALITY(5),
                    0,
                    face,
                )
            };
            let ui = theme::UI_FONT;
            let mono = theme::MONO_FONT;
            let cjk = theme::CJK_FONT;
            let ui_w = wide(ui);
            let mono_w = wide(mono);
            let cjk_w = wide(cjk);
            Self {
                background: unsafe { CreateSolidBrush(theme::VOID) },
                surface: unsafe { CreateSolidBrush(theme::SURFACE) },
                raised: unsafe { CreateSolidBrush(theme::RAISED) },
                line: unsafe { CreateSolidBrush(theme::LINE) },
                accent: unsafe { CreateSolidBrush(theme::ACCENT) },
                accent_dim: unsafe { CreateSolidBrush(blend(theme::ACCENT, theme::VOID, 0.14)) },
                accent_edge: unsafe { CreateSolidBrush(blend(theme::ACCENT, theme::VOID, 0.35)) },
                ok: unsafe { CreateSolidBrush(theme::OK) },
                err: unsafe { CreateSolidBrush(theme::ERR) },
                err_fill: unsafe { CreateSolidBrush(blend(theme::ERR, theme::RAISED, 0.08)) },
                body_font: create_font(PCWSTR(ui_w.as_ptr()), theme::BODY_SIZE_PT, 400),
                button_font: create_font(PCWSTR(ui_w.as_ptr()), theme::BUTTON_SIZE_PT, 600),
                caption_font: create_font(PCWSTR(ui_w.as_ptr()), 11, 600),
                title_font: create_font(PCWSTR(ui_w.as_ptr()), 13, 600),
                mono_font: create_font(PCWSTR(mono_w.as_ptr()), theme::MONO_SIZE_PT, 400),
                cjk_font: create_font(PCWSTR(cjk_w.as_ptr()), 12, 400),
            }
        }
    }

    impl Drop for ThemeResources {
        fn drop(&mut self) {
            unsafe {
                for object in [
                    HGDIOBJ(self.background.0),
                    HGDIOBJ(self.surface.0),
                    HGDIOBJ(self.raised.0),
                    HGDIOBJ(self.line.0),
                    HGDIOBJ(self.accent.0),
                    HGDIOBJ(self.accent_dim.0),
                    HGDIOBJ(self.accent_edge.0),
                    HGDIOBJ(self.ok.0),
                    HGDIOBJ(self.err.0),
                    HGDIOBJ(self.err_fill.0),
                    HGDIOBJ(self.body_font.0),
                    HGDIOBJ(self.button_font.0),
                    HGDIOBJ(self.caption_font.0),
                    HGDIOBJ(self.title_font.0),
                    HGDIOBJ(self.mono_font.0),
                    HGDIOBJ(self.cjk_font.0),
                ] {
                    if !object.0.is_null() {
                        let _ = DeleteObject(object);
                    }
                }
            }
        }
    }

    struct Handles {
        settings_nav: HWND,
        prompts_nav: HWND,
        history_nav: HWND,
        language: HWND,
        settings_page: HWND,
        prompts_page: HWND,
        history_page: HWND,
        endpoint: HWND,
        model: HWND,
        credential_target: HWND,
        api_key: HWND,
        credential_status: HWND,
        settings_selection_default: HWND,
        settings_hover_default: HWND,
        profile_id: HWND,
        profile_name: HWND,
        system_prompt: HWND,
        user_template: HWND,
        profile_model: HWND,
        temperature: HWND,
        max_tokens: HWND,
        prompt_status: HWND,
        history_search: HWND,
        history_prompt: HWND,
        history_source: HWND,
        history_order: HWND,
        history_list: HWND,
        history_target: HWND,
        history_context: HWND,
        history_output: HWND,
        history_meta: HWND,
        history_count: HWND,
        status: HWND,
        placements: Vec<ControlPlacement>,
        localized: Vec<LocalizedControl>,
    }

    impl Default for Handles {
        fn default() -> Self {
            let null = HWND(std::ptr::null_mut());
            Self {
                settings_nav: null,
                prompts_nav: null,
                history_nav: null,
                language: null,
                settings_page: null,
                prompts_page: null,
                history_page: null,
                endpoint: null,
                model: null,
                credential_target: null,
                api_key: null,
                credential_status: null,
                settings_selection_default: null,
                settings_hover_default: null,
                profile_id: null,
                profile_name: null,
                system_prompt: null,
                user_template: null,
                profile_model: null,
                temperature: null,
                max_tokens: null,
                prompt_status: null,
                history_search: null,
                history_prompt: null,
                history_source: null,
                history_order: null,
                history_list: null,
                history_target: null,
                history_context: null,
                history_output: null,
                history_meta: null,
                history_count: null,
                status: null,
                placements: Vec::new(),
                localized: Vec::new(),
            }
        }
    }

    struct ManagerState {
        config: AppConfig,
        config_path: Option<PathBuf>,
        load_error: Option<String>,
        handles: Handles,
        view: View,
        profile_index: usize,
        history_entries: Vec<HistoryEntry>,
        history_loaded: bool,
        resident_start: ResidentStartOutcome,
        credential_status: CredentialStatusState,
        theme: ThemeResources,
        dpi: u32,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum CredentialStatusState {
        Present,
        Absent,
        Unavailable(String),
    }

    impl ManagerState {
        fn language(&self) -> UiLanguage {
            self.config.ui.manager_language
        }
    }

    struct Secret(String);
    impl Drop for Secret {
        fn drop(&mut self) {
            unsafe { self.0.as_mut_vec().fill(0) }
        }
    }

    pub fn run() -> windows::core::Result<()> {
        unsafe {
            let _ = windows::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(
                windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
            );
        }
        let resident_start = ensure_resident_running();
        let instance = unsafe { GetModuleHandleW(None)? };
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: HINSTANCE(instance.0),
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        if unsafe { RegisterClassW(&class) } == 0 {
            return Err(Error::from_win32());
        }
        let page_class = WNDCLASSW {
            lpfnWndProc: Some(page_proc),
            hInstance: HINSTANCE(instance.0),
            lpszClassName: PAGE_CLASS,
            ..Default::default()
        };
        if unsafe { RegisterClassW(&page_class) } == 0 {
            return Err(Error::from_win32());
        }
        let (config, load_error) = load_config();
        let initial_language = config.ui.manager_language;
        let window_title = wide(ui_text(initial_language, TextKey::WindowTitle));
        let dpi = unsafe { GetDpiForSystem() }.max(DEFAULT_DPI);
        let state = Box::new(ManagerState {
            config,
            config_path: default_config_path(),
            load_error,
            handles: Handles::default(),
            view: View::Settings,
            profile_index: 0,
            history_entries: Vec::new(),
            history_loaded: false,
            resident_start,
            credential_status: CredentialStatusState::Absent,
            theme: ThemeResources::new(dpi),
            dpi,
        });
        let state_ptr = Box::into_raw(state);
        let style = WS_POPUP | WS_THICKFRAME | WS_CLIPCHILDREN;
        // Outer size includes the native caption/frame so client == DEFAULT_CLIENT_*.
        let mut frame = RECT::default();
        unsafe {
            let _ = AdjustWindowRectEx(&mut frame, style, false, WS_EX_CONTROLPARENT);
        }
        let outer_w = DEFAULT_CLIENT_WIDTH + (frame.right - frame.left);
        let outer_h = DEFAULT_CLIENT_HEIGHT + (frame.bottom - frame.top);
        let hwnd = unsafe {
            match CreateWindowExW(
                WS_EX_CONTROLPARENT,
                CLASS_NAME,
                PCWSTR(window_title.as_ptr()),
                style,
                0,
                0,
                scale_for_dpi(outer_w, dpi),
                scale_for_dpi(outer_h, dpi),
                None,
                None,
                Some(HINSTANCE(instance.0)),
                Some(state_ptr.cast()),
            ) {
                Ok(hwnd) => hwnd,
                Err(error) => {
                    drop(Box::from_raw(state_ptr));
                    return Err(error);
                }
            }
        };
        unsafe {
            apply_dark_title_bar(hwnd);
            let window_dpi = GetDpiForWindow(hwnd).max(DEFAULT_DPI);
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                scale_for_dpi(outer_w, window_dpi),
                scale_for_dpi(outer_h, window_dpi),
                SWP_NOACTIVATE | SWP_NOZORDER,
            );
            let _ = ShowWindow(
                hwnd,
                windows::Win32::UI::WindowsAndMessaging::SW_SHOWDEFAULT,
            );
            apply_dark_title_bar(hwnd);
        }
        let mut message = windows::Win32::UI::WindowsAndMessaging::MSG::default();
        let result = loop {
            let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
            if result.0 == -1 {
                break Err(Error::from_win32());
            }
            if result.0 == 0 {
                break Ok(());
            }
            unsafe {
                if IsDialogMessageW(hwnd, &message).as_bool() {
                    continue;
                }
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        };
        unsafe {
            let raw = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ManagerState;
            if !raw.is_null() {
                let _ = SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                drop(Box::from_raw(raw));
            }
        }
        result
    }

    fn load_config() -> (AppConfig, Option<String>) {
        // Prefer %LOCALAPPDATA%\SelectionTranslate\config.toml, then a portable
        // config.toml next to the executable (package folder).
        let mut candidates: Vec<std::path::PathBuf> = Vec::new();
        if let Some(path) = default_config_path() {
            candidates.push(path);
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                candidates.push(dir.join("config.toml"));
            }
        }
        for path in &candidates {
            if !path.exists() {
                continue;
            }
            return match AppConfig::load(path) {
                Ok(config) => (config, None),
                Err(error) => (
                    AppConfig::default(),
                    Some(status_text(
                        UiLanguage::English,
                        StatusEvent::ConfigLoadFailed {
                            detail: &format!("{}: {error}", path.display()),
                        },
                    )),
                ),
            };
        }
        if candidates.is_empty() {
            return (
                AppConfig::default(),
                Some(status_text(
                    UiLanguage::English,
                    StatusEvent::LocalAppDataUnavailable {
                        operation: StatusOperation::Save,
                    },
                )),
            );
        }
        (AppConfig::default(), None)
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if message == WM_CREATE {
            let create = &*(lparam.0 as *const CREATESTRUCTW);
            let state = create.lpCreateParams as *mut ManagerState;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize);
            let window_dpi = GetDpiForWindow(hwnd).max(DEFAULT_DPI);
            if (*state).dpi != window_dpi {
                (*state).dpi = window_dpi;
                (*state).theme = ThemeResources::new(window_dpi);
            }
            if let Err(error) = initialize_controls(hwnd, &mut *state) {
                set_status(
                    &*state,
                    &status_text(
                        (*state).language(),
                        StatusEvent::ManagerInitializationFailed {
                            detail: &error.to_string(),
                        },
                    ),
                );
            }
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                scale_for_dpi(DEFAULT_CLIENT_WIDTH, window_dpi),
                scale_for_dpi(DEFAULT_CLIENT_HEIGHT, window_dpi),
                SWP_NOACTIVATE | SWP_NOZORDER,
            );
            apply_manager_layout(hwnd, &*state);
            return LRESULT(0);
        }
        let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ManagerState;
        if state.is_null() {
            return DefWindowProcW(hwnd, message, wparam, lparam);
        }
        match message {
            WM_COMMAND => handle_command(hwnd, &mut *state, wparam.0),
            WM_SIZE => {
                apply_manager_layout(hwnd, &*state);
                LRESULT(0)
            }
            WM_DPICHANGED => {
                let dpi = ((wparam.0 >> 16) as u32).max(DEFAULT_DPI);
                let suggested = &*(lparam.0 as *const RECT);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    suggested.left,
                    suggested.top,
                    (suggested.right - suggested.left).max(1),
                    (suggested.bottom - suggested.top).max(1),
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
                (*state).dpi = dpi;
                (*state).theme = ThemeResources::new(dpi);
                apply_control_fonts(&*state);
                apply_manager_layout(hwnd, &*state);
                let _ = InvalidateRect(Some(hwnd), None, true);
                LRESULT(0)
            }
            WM_GETMINMAXINFO => {
                let limits = &mut *(lparam.0 as *mut MINMAXINFO);
                limits.ptMinTrackSize.x = scale_for_dpi(MIN_CONTENT_WIDTH, (*state).dpi);
                limits.ptMinTrackSize.y = scale_for_dpi(MIN_CLIENT_HEIGHT, (*state).dpi);
                LRESULT(0)
            }
            WM_NCHITTEST => {
                let hit = DefWindowProcW(hwnd, message, wparam, lparam);
                if hit != LRESULT(HTCLIENT as isize) {
                    return hit;
                }
                let _x = (lparam.0 as u32 & 0xffff) as u16 as i16 as i32;
                let y = ((lparam.0 as u32 >> 16) & 0xffff) as u16 as i16 as i32;
                let mut wr = RECT::default();
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut wr);
                }
                let client_y = y - wr.top;
                let logical_y = unscale_from_dpi(client_y.max(0), (*state).dpi);
                if logical_y < TITLE_BAR_HEIGHT {
                    return LRESULT(HTCAPTION as isize);
                }
                hit
            }
            WM_PAINT => {
                paint_manager(hwnd, &*state);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_MEASUREITEM => {
                if lparam.0 != 0 {
                    let measure = &mut *(lparam.0 as *mut MEASUREITEMSTRUCT);
                    measure.itemHeight = scale_for_dpi(HISTORY_ROW_HEIGHT, (*state).dpi) as u32;
                }
                LRESULT(1)
            }
            WM_DRAWITEM => {
                if lparam.0 != 0 {
                    draw_manager_item(&*(lparam.0 as *const DRAWITEMSTRUCT), &*state);
                }
                LRESULT(1)
            }
            0x0133 | 0x0134 | 0x0135 | 0x0138 => {
                themed_control_color(&*state, message, wparam, lparam)
            }
            WM_CLOSE => {
                DestroyWindow(hwnd).ok();
                LRESULT(0)
            }
            WM_DESTROY => {
                let _ = SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                drop(Box::from_raw(state));
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }

    unsafe extern "system" fn page_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if matches!(
            message,
            WM_COMMAND
                | WM_NOTIFY
                | WM_DRAWITEM
                | WM_MEASUREITEM
                | 0x0133
                | 0x0134
                | 0x0135
                | 0x0138
        ) {
            if let Ok(parent) = unsafe { windows::Win32::UI::WindowsAndMessaging::GetParent(hwnd) }
            {
                return unsafe { SendMessageW(parent, message, Some(wparam), Some(lparam)) };
            }
        }
        if message == WM_PAINT {
            paint_page(hwnd);
            return LRESULT(0);
        }
        if message == WM_ERASEBKGND {
            return LRESULT(1);
        }
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }

    fn initialize_controls(hwnd: HWND, state: &mut ManagerState) -> windows::core::Result<()> {
        let mut h = Handles::default();
        for (id, text, x) in [
            (ID_SETTINGS_TAB, "Settings", 12),
            (ID_PROMPTS_TAB, "Prompts", 112),
            (ID_HISTORY_TAB, "History", 212),
        ] {
            let tab = create_control(
                hwnd,
                w!("BUTTON"),
                text,
                WS_CHILD
                    | WS_VISIBLE
                    | WS_TABSTOP
                    | WINDOW_STYLE(BS_PUSHBUTTON as u32 | BS_OWNERDRAW as u32),
                x,
                TITLE_BAR_HEIGHT + 6,
                96,
                33,
                id,
            )?;
            if let Some(key) = text_key_from_english(text) {
                h.localized.push(LocalizedControl { hwnd: tab, key });
            }
            match id {
                ID_SETTINGS_TAB => h.settings_nav = tab,
                ID_PROMPTS_TAB => h.prompts_nav = tab,
                ID_HISTORY_TAB => h.history_nav = tab,
                _ => {}
            }
        }

        // Page controls live under one dedicated child container each.  This
        // makes tab switching a single parent visibility operation; hidden
        // controls can never paint over the newly selected page.
        h.settings_page = create_page_container(hwnd, ID_SETTINGS_PAGE)?;
        h.prompts_page = create_page_container(hwnd, ID_PROMPTS_PAGE)?;
        h.history_page = create_page_container(hwnd, ID_HISTORY_PAGE)?;

        // Settings
        add_label(
            hwnd,
            &mut h,
            "Endpoint",
            Slot::SettingsEndpointLabel,
            Some(View::Settings),
        )?;
        h.endpoint = add_edit_with_id(
            hwnd,
            &mut h,
            "",
            Slot::SettingsEndpoint,
            Some(View::Settings),
            false,
            false,
            ID_SETTINGS_ENDPOINT,
        )?;
        add_label(
            hwnd,
            &mut h,
            "Model",
            Slot::SettingsModelLabel,
            Some(View::Settings),
        )?;
        h.model = add_edit_with_id(
            hwnd,
            &mut h,
            "",
            Slot::SettingsModel,
            Some(View::Settings),
            false,
            false,
            ID_SETTINGS_MODEL,
        )?;
        add_label(
            hwnd,
            &mut h,
            "Credential target",
            Slot::SettingsCredentialTargetLabel,
            Some(View::Settings),
        )?;
        h.credential_target = add_edit_with_id(
            hwnd,
            &mut h,
            "",
            Slot::SettingsCredentialTarget,
            Some(View::Settings),
            false,
            false,
            ID_SETTINGS_CREDENTIAL_TARGET,
        )?;
        add_label(
            hwnd,
            &mut h,
            "API key",
            Slot::SettingsApiKeyLabel,
            Some(View::Settings),
        )?;
        h.api_key = add_edit_with_id(
            hwnd,
            &mut h,
            "",
            Slot::SettingsApiKey,
            Some(View::Settings),
            true,
            false,
            ID_SETTINGS_API_KEY,
        )?;
        add_button(
            hwnd,
            &mut h,
            ID_SAVE_KEY,
            "Save key",
            Slot::SettingsSaveKey,
            Some(View::Settings),
            ButtonKind::Primary,
        )?;
        add_button(
            hwnd,
            &mut h,
            ID_DELETE_KEY,
            "Delete saved key",
            Slot::SettingsDeleteKey,
            Some(View::Settings),
            ButtonKind::Danger,
        )?;
        h.credential_status = add_label(
            hwnd,
            &mut h,
            "",
            Slot::SettingsCredentialStatus,
            Some(View::Settings),
        )?;
        add_label(
            hwnd,
            &mut h,
            "Keys are held by Windows Credential Manager; they never enter config.toml.",
            Slot::SettingsCredentialHint,
            Some(View::Settings),
        )?;
        add_label(
            hwnd,
            &mut h,
            "Selection profile",
            Slot::SettingsSelectionDefaultLabel,
            Some(View::Settings),
        )?;
        h.settings_selection_default = add_combo(
            hwnd,
            &mut h,
            ID_SETTINGS_SELECTION_DEFAULT,
            Slot::SettingsSelectionDefault,
            Some(View::Settings),
            false,
        )?;
        add_label(
            hwnd,
            &mut h,
            "Hover profile",
            Slot::SettingsHoverDefaultLabel,
            Some(View::Settings),
        )?;
        h.settings_hover_default = add_combo(
            hwnd,
            &mut h,
            ID_SETTINGS_HOVER_DEFAULT,
            Slot::SettingsHoverDefault,
            Some(View::Settings),
            false,
        )?;
        add_label(
            hwnd,
            &mut h,
            "Interface language",
            Slot::SettingsLanguageLabel,
            Some(View::Settings),
        )?;
        h.language = add_combo(
            hwnd,
            &mut h,
            ID_LANGUAGE,
            Slot::SettingsLanguage,
            Some(View::Settings),
            false,
        )?;
        add_button(
            hwnd,
            &mut h,
            ID_SAVE_SETTINGS,
            "Save settings",
            Slot::SettingsSave,
            Some(View::Settings),
            ButtonKind::Primary,
        )?;

        // Prompts — meta row + two editors (no pager, no defaults).
        add_label(
            hwnd,
            &mut h,
            "ID",
            Slot::PromptsIdLabel,
            Some(View::Prompts),
        )?;
        h.profile_id = add_combo(
            hwnd,
            &mut h,
            ID_PROMPT_ID,
            Slot::PromptsId,
            Some(View::Prompts),
            true,
        )?;
        add_label(
            hwnd,
            &mut h,
            "Name",
            Slot::PromptsNameLabel,
            Some(View::Prompts),
        )?;
        h.profile_name = add_edit_with_id(
            hwnd,
            &mut h,
            "",
            Slot::PromptsName,
            Some(View::Prompts),
            false,
            false,
            ID_PROMPT_NAME,
        )?;
        add_label(
            hwnd,
            &mut h,
            "Model override",
            Slot::PromptsModelLabel,
            Some(View::Prompts),
        )?;
        h.profile_model = add_edit_with_id(
            hwnd,
            &mut h,
            "",
            Slot::PromptsModel,
            Some(View::Prompts),
            false,
            false,
            ID_PROMPT_MODEL,
        )?;
        add_label(
            hwnd,
            &mut h,
            "Temperature",
            Slot::PromptsTemperatureLabel,
            Some(View::Prompts),
        )?;
        h.temperature = add_edit_with_id(
            hwnd,
            &mut h,
            "",
            Slot::PromptsTemperature,
            Some(View::Prompts),
            false,
            false,
            ID_PROMPT_TEMPERATURE,
        )?;
        add_label(
            hwnd,
            &mut h,
            "Max tokens",
            Slot::PromptsMaxTokensLabel,
            Some(View::Prompts),
        )?;
        h.max_tokens = add_edit_with_id(
            hwnd,
            &mut h,
            "",
            Slot::PromptsMaxTokens,
            Some(View::Prompts),
            false,
            false,
            ID_PROMPT_MAX_TOKENS,
        )?;
        add_button(
            hwnd,
            &mut h,
            ID_SAVE_PROMPT,
            "Save prompt",
            Slot::PromptsSave,
            Some(View::Prompts),
            ButtonKind::Primary,
        )?;
        add_label(
            hwnd,
            &mut h,
            "Use {target}, {context}, and {source}; every user template needs {target}.",
            Slot::PromptsHint,
            Some(View::Prompts),
        )?;
        h.prompt_status = add_label(hwnd, &mut h, "", Slot::PromptsStatus, Some(View::Prompts))?;
        h.system_prompt = add_edit_with_id(
            hwnd,
            &mut h,
            "",
            Slot::PromptsSystemWell,
            Some(View::Prompts),
            false,
            true,
            ID_PROMPT_SYSTEM,
        )?;
        h.user_template = add_edit_with_id(
            hwnd,
            &mut h,
            "",
            Slot::PromptsUserWell,
            Some(View::Prompts),
            false,
            true,
            ID_PROMPT_USER_TEMPLATE,
        )?;

        // History
        h.history_search = add_edit_with_id(
            hwnd,
            &mut h,
            "",
            Slot::HistorySearch,
            Some(View::History),
            false,
            false,
            ID_HISTORY_SEARCH,
        )?;
        set_edit_cue(
            h.history_search,
            ui_text(state.language(), TextKey::SearchTargetOutput),
        );
        add_button(
            hwnd,
            &mut h,
            ID_HISTORY_REFRESH,
            "Refresh",
            Slot::HistoryRefresh,
            Some(View::History),
            ButtonKind::Primary,
        )?;
        add_button(
            hwnd,
            &mut h,
            ID_HISTORY_COPY,
            "Copy output",
            Slot::HistoryCopy,
            Some(View::History),
            ButtonKind::Default,
        )?;
        // History filters (Prompt / Source / Order) — option text carries the label.
        h.history_prompt = add_combo(
            hwnd,
            &mut h,
            ID_HISTORY_PROMPT,
            Slot::HistoryPrompt,
            Some(View::History),
            false,
        )?;
        h.history_source = add_combo(
            hwnd,
            &mut h,
            ID_HISTORY_SOURCE,
            Slot::HistorySource,
            Some(View::History),
            false,
        )?;
        h.history_order = add_combo(
            hwnd,
            &mut h,
            ID_HISTORY_ORDER,
            Slot::HistoryOrder,
            Some(View::History),
            false,
        )?;
        h.history_list = add_list(hwnd, &mut h, Slot::HistoryList, Some(View::History))?;
        h.history_target =
            add_readonly_edit(hwnd, &mut h, Slot::HistoryTarget, Some(View::History), true)?;
        h.history_context = add_readonly_edit(
            hwnd,
            &mut h,
            Slot::HistoryContext,
            Some(View::History),
            true,
        )?;
        h.history_meta = add_label(hwnd, &mut h, "", Slot::HistoryMeta, Some(View::History))?;
        h.history_output =
            add_readonly_edit(hwnd, &mut h, Slot::HistoryOutput, Some(View::History), true)?;
        add_label(
            hwnd,
            &mut h,
            "History is loaded only while this tab is open; the database is never held open.",
            Slot::HistoryHint,
            Some(View::History),
        )?;
        h.history_count = add_label(hwnd, &mut h, "", Slot::HistoryCount, Some(View::History))?;
        add_button(
            hwnd,
            &mut h,
            ID_HISTORY_DELETE,
            "Delete selected",
            Slot::HistoryDelete,
            Some(View::History),
            ButtonKind::Danger,
        )?;
        // Status string is stored here; paint_manager draws it on the tab bar.
        // Keep the STATIC hidden so it cannot double-paint under the chrome.
        h.status = create_control(hwnd, w!("STATIC"), "", WS_CHILD, 0, 0, 10, 10, 0)?;
        // Custom window controls (no OS caption on the popup-style frame).
        for (id, text) in [(ID_WIN_MIN, "—"), (ID_WIN_MAX, "□"), (ID_WIN_CLOSE, "✕")] {
            create_control(
                hwnd,
                w!("BUTTON"),
                text,
                WS_CHILD
                    | WS_VISIBLE
                    | WS_TABSTOP
                    | WINDOW_STYLE(BS_PUSHBUTTON as u32 | BS_OWNERDRAW as u32),
                0,
                0,
                36,
                28,
                id,
            )?;
        }
        state.handles = h;
        apply_control_fonts(state);
        apply_control_themes(state);
        apply_static_localization(hwnd, state);
        set_text(state.handles.endpoint, &state.config.provider.endpoint);
        set_text(state.handles.model, &state.config.provider.model);
        set_text(
            state.handles.credential_target,
            &state.config.provider.credential_target,
        );
        state.profile_index = 0;
        refresh_prompt_form(state);
        state.view = View::Settings;
        show_view(hwnd, state);
        refresh_settings_form(state);
        if let Some(error) = state.load_error.clone() {
            set_status(state, &error);
        } else {
            update_credential_status(state);
            set_status(
                state,
                &status_text(
                    state.language(),
                    StatusEvent::ResidentStart(state.resident_start),
                ),
            );
        }
        Ok(())
    }

    fn add_control(state: &mut Handles, hwnd: HWND, view: Option<View>, slot: Slot) -> HWND {
        if let Some(view) = view {
            state.placements.push(ControlPlacement { hwnd, view, slot });
        }
        hwnd
    }

    fn page_parent(parent: HWND, h: &Handles, view: Option<View>) -> HWND {
        match view {
            Some(View::Settings) => h.settings_page,
            Some(View::Prompts) => h.prompts_page,
            Some(View::History) => h.history_page,
            None => parent,
        }
    }

    fn create_page_container(parent: HWND, id: usize) -> windows::core::Result<HWND> {
        create_control_with_style(
            parent,
            PAGE_CLASS,
            "",
            WS_CHILD | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
            0,
            CHROME_HEIGHT,
            780,
            528,
            id,
        )
    }

    fn add_label(
        parent: HWND,
        h: &mut Handles,
        text: &str,
        slot: Slot,
        view: Option<View>,
    ) -> windows::core::Result<HWND> {
        let parent = page_parent(parent, h, view);
        let hwnd = create_control(
            parent,
            w!("STATIC"),
            text,
            WS_CHILD | visibility_style(view),
            0,
            0,
            10,
            10,
            0,
        )?;
        if let Some(key) = text_key_from_english(text) {
            h.localized.push(LocalizedControl { hwnd, key });
        }
        // Field labels are painted by paint_page (avoids STATIC black-on-black).
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
        set_text(hwnd, text);
        Ok(add_control(h, hwnd, view, slot))
    }

    #[allow(clippy::too_many_arguments)]
    fn add_button(
        parent: HWND,
        h: &mut Handles,
        id: usize,
        text: &str,
        slot: Slot,
        view: Option<View>,
        _kind: ButtonKind,
    ) -> windows::core::Result<HWND> {
        let parent = page_parent(parent, h, view);
        let hwnd = create_control(
            parent,
            w!("BUTTON"),
            text,
            WS_CHILD
                | visibility_style(view)
                | WS_TABSTOP
                | WINDOW_STYLE(BS_PUSHBUTTON as u32 | BS_OWNERDRAW as u32),
            0,
            0,
            10,
            10,
            id,
        )?;
        if let Some(key) = text_key_from_english(text) {
            h.localized.push(LocalizedControl { hwnd, key });
        }
        Ok(add_control(h, hwnd, view, slot))
    }

    #[allow(clippy::too_many_arguments)]
    fn add_edit_with_id(
        parent: HWND,
        h: &mut Handles,
        text: &str,
        slot: Slot,
        view: Option<View>,
        password: bool,
        multiline: bool,
        id: usize,
    ) -> windows::core::Result<HWND> {
        let parent = page_parent(parent, h, view);
        let mut style = WS_CHILD
            | WS_CLIPSIBLINGS
            | visibility_style(view)
            | WS_TABSTOP
            | WS_BORDER
            | WINDOW_STYLE(ES_AUTOHSCROLL as u32);
        if password {
            style |= WINDOW_STYLE(ES_PASSWORD as u32);
        }
        if multiline {
            style |= WINDOW_STYLE(ES_MULTILINE as u32)
                | WINDOW_STYLE(ES_AUTOVSCROLL as u32)
                | WS_VSCROLL;
        }
        let hwnd = create_control(parent, w!("EDIT"), text, style, 0, 0, 10, 10, id)?;
        apply_dark_scrollbar(hwnd);
        Ok(add_control(h, hwnd, view, slot))
    }

    fn add_edit(
        parent: HWND,
        h: &mut Handles,
        text: &str,
        slot: Slot,
        view: Option<View>,
        password: bool,
        multiline: bool,
    ) -> windows::core::Result<HWND> {
        add_edit_with_id(parent, h, text, slot, view, password, multiline, 0)
    }

    fn add_readonly_edit(
        parent: HWND,
        h: &mut Handles,
        slot: Slot,
        view: Option<View>,
        multiline: bool,
    ) -> windows::core::Result<HWND> {
        let hwnd = add_edit(parent, h, "", slot, view, false, multiline)?;
        unsafe {
            let _ = SendMessageW(hwnd, EM_SETREADONLY, Some(WPARAM(1)), Some(LPARAM(0)));
        }
        Ok(hwnd)
    }

    /// `editable` selects CBS_DROPDOWN (Prompts ID) vs CBS_DROPDOWNLIST.
    fn add_combo(
        parent: HWND,
        h: &mut Handles,
        id: usize,
        slot: Slot,
        view: Option<View>,
        editable: bool,
    ) -> windows::core::Result<HWND> {
        let parent = page_parent(parent, h, view);
        let list_style = if editable {
            CBS_DROPDOWN
        } else {
            CBS_DROPDOWNLIST
        };
        let style = WS_CHILD
            | WS_CLIPSIBLINGS
            | visibility_style(view)
            | WS_TABSTOP
            | WS_VSCROLL
            | WINDOW_STYLE(list_style)
            | WS_BORDER;
        let hwnd = create_control_with_style(parent, w!("COMBOBOX"), "", style, 0, 0, 10, 10, id)?;
        apply_dark_scrollbar(hwnd);
        Ok(add_control(h, hwnd, view, slot))
    }

    fn add_list(
        parent: HWND,
        h: &mut Handles,
        slot: Slot,
        view: Option<View>,
    ) -> windows::core::Result<HWND> {
        let parent = page_parent(parent, h, view);
        let style = WS_CHILD
            | WS_CLIPSIBLINGS
            | visibility_style(view)
            | WS_TABSTOP
            | WS_BORDER
            | WS_VSCROLL
            | WINDOW_STYLE(
                LBS_NOTIFY | LBS_OWNERDRAWVARIABLE | LBS_HASSTRINGS | LBS_NOINTEGRALHEIGHT,
            );
        let hwnd = create_control_with_style(
            parent,
            w!("LISTBOX"),
            "",
            style,
            0,
            0,
            10,
            10,
            ID_HISTORY_LIST,
        )?;
        apply_dark_scrollbar(hwnd);
        Ok(add_control(h, hwnd, view, slot))
    }

    #[allow(clippy::too_many_arguments)]
    fn create_control(
        parent: HWND,
        class: PCWSTR,
        text: &str,
        style: WINDOW_STYLE,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        id: usize,
    ) -> windows::core::Result<HWND> {
        create_control_with_style(parent, class, text, style, x, y, width, height, id)
    }

    #[allow(clippy::too_many_arguments)]
    fn create_control_with_style(
        parent: HWND,
        class: PCWSTR,
        text: &str,
        style: WINDOW_STYLE,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        id: usize,
    ) -> windows::core::Result<HWND> {
        let text = wide(text);
        let style = style | WS_CLIPSIBLINGS;
        let dpi = unsafe { GetDpiForWindow(parent) }.max(96);
        let x = scale_for_dpi(x, dpi);
        let y = scale_for_dpi(y, dpi);
        let width = scale_for_dpi(width, dpi);
        let height = scale_for_dpi(height, dpi);
        unsafe {
            let extended_style = if class == PAGE_CLASS {
                WS_EX_CONTROLPARENT
            } else {
                Default::default()
            };
            let hwnd = CreateWindowExW(
                extended_style,
                class,
                PCWSTR(text.as_ptr()),
                style,
                x,
                y,
                width,
                height,
                Some(parent),
                if id == 0 {
                    None
                } else {
                    Some(windows::Win32::UI::WindowsAndMessaging::HMENU(
                        id as *mut c_void,
                    ))
                },
                None,
                None,
            )?;
            apply_dark_scrollbar(hwnd);
            Ok(hwnd)
        }
    }

    fn apply_dark_scrollbar(hwnd: HWND) {
        if hwnd.0.is_null() {
            return;
        }
        unsafe {
            let _ = SetWindowTheme(hwnd, w!("DarkMode_Explorer"), PCWSTR::null());
        }
    }

    fn apply_dark_title_bar(hwnd: HWND) {
        use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWINDOWATTRIBUTE};
        let enabled: i32 = 1;
        let payload = &enabled as *const i32 as *const core::ffi::c_void;
        let size = std::mem::size_of::<i32>() as u32;
        // Force caption/border/text colors so accent-color title bars cannot win.
        let caption: u32 = theme::SURFACE.0;
        let border: u32 = theme::LINE.0;
        let text: u32 = theme::INK.0;
        unsafe {
            let _ = DwmSetWindowAttribute(hwnd, DWMWINDOWATTRIBUTE(20), payload, size);
            let _ = DwmSetWindowAttribute(hwnd, DWMWINDOWATTRIBUTE(19), payload, size);
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWINDOWATTRIBUTE(35), // DWMWA_CAPTION_COLOR
                &caption as *const u32 as *const core::ffi::c_void,
                size,
            );
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWINDOWATTRIBUTE(34), // DWMWA_BORDER_COLOR
                &border as *const u32 as *const core::ffi::c_void,
                size,
            );
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWINDOWATTRIBUTE(36), // DWMWA_TEXT_COLOR
                &text as *const u32 as *const core::ffi::c_void,
                size,
            );
            let _ = SetWindowTheme(hwnd, w!("DarkMode_Explorer"), PCWSTR::null());
        }
    }

    fn scale_for_dpi(value: i32, dpi: u32) -> i32 {
        ((value as i64 * dpi.max(96) as i64) / 96).clamp(1, i32::MAX as i64) as i32
    }

    fn unscale_from_dpi(value: i32, dpi: u32) -> i32 {
        ((value as i64 * 96) / dpi.max(96) as i64).clamp(1, i32::MAX as i64) as i32
    }

    fn scaled_rect(rect: RECT, dpi: u32) -> RECT {
        RECT {
            left: scale_for_dpi(rect.left, dpi),
            top: scale_for_dpi(rect.top, dpi),
            right: scale_for_dpi(rect.right, dpi),
            bottom: scale_for_dpi(rect.bottom, dpi),
        }
    }

    fn place_control(hwnd: HWND, rect: RECT, dpi: u32) {
        let rect = scaled_rect(rect, dpi);
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                None,
                rect.left,
                rect.top,
                (rect.right - rect.left).max(1),
                (rect.bottom - rect.top).max(1),
                SWP_NOACTIVATE | SWP_NOZORDER,
            );
        }
    }

    fn apply_manager_layout(hwnd: HWND, state: &ManagerState) {
        let mut client = RECT::default();
        if unsafe { GetClientRect(hwnd, &mut client) }.is_err() {
            return;
        }
        let logical_w = unscale_from_dpi(client.right.max(1), state.dpi);
        let logical_h = unscale_from_dpi(client.bottom.max(1), state.dpi);
        let layout = ManagerLayout::for_client(logical_w, logical_h);
        for (control, rect) in [
            (state.handles.settings_nav, layout.tab_buttons[0]),
            (state.handles.prompts_nav, layout.tab_buttons[1]),
            (state.handles.history_nav, layout.tab_buttons[2]),
            (state.handles.status, layout.status_text),
        ] {
            place_control(control, rect, state.dpi);
        }
        // Window controls sit on the title-bar strip.
        let btn_w = 40;
        let btn_h = 28;
        let y = 4;
        let close_x = logical_w - 12 - btn_w;
        let max_x = close_x - btn_w;
        let min_x = max_x - btn_w;
        for (id, x) in [
            (ID_WIN_MIN, min_x),
            (ID_WIN_MAX, max_x),
            (ID_WIN_CLOSE, close_x),
        ] {
            let hwnd_btn = unsafe { GetDlgItem(Some(hwnd), id as i32) };
            if let Ok(hwnd_btn) = hwnd_btn {
                if !hwnd_btn.0.is_null() {
                    place_control(hwnd_btn, rect(x, y, btn_w, btn_h), state.dpi);
                }
            }
        }
        let page_rect = scaled_rect(layout.page, state.dpi);
        for page in [
            state.handles.settings_page,
            state.handles.prompts_page,
            state.handles.history_page,
        ] {
            unsafe {
                let _ = SetWindowPos(
                    page,
                    None,
                    page_rect.left,
                    page_rect.top,
                    (page_rect.right - page_rect.left).max(1),
                    (page_rect.bottom - page_rect.top).max(1),
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
            }
        }
        let page_w = page_rect.right - page_rect.left;
        let page_h = page_rect.bottom - page_rect.top;
        let page_logical_w = unscale_from_dpi(page_w, state.dpi);
        let page_logical_h = unscale_from_dpi(page_h, state.dpi);
        for view in [View::Settings, View::Prompts, View::History] {
            let page_layout = page_layout(view, page_logical_w, page_logical_h);
            for placement in state.handles.placements.iter().filter(|p| p.view == view) {
                if let Some(rect) = page_layout.rect_for(placement.slot) {
                    place_control(placement.hwnd, rect, state.dpi);
                }
            }
        }
        unsafe {
            for button in [
                state.handles.settings_nav,
                state.handles.prompts_nav,
                state.handles.history_nav,
            ] {
                let _ = InvalidateRect(Some(button), None, true);
                let _ = UpdateWindow(button);
            }
            let _ = InvalidateRect(Some(hwnd), None, true);
            for page in [
                state.handles.settings_page,
                state.handles.prompts_page,
                state.handles.history_page,
            ] {
                let _ = InvalidateRect(Some(page), None, true);
            }
        }
    }

    fn apply_control_fonts(state: &ManagerState) {
        let body = state.theme.body_font;
        let mono = state.theme.mono_font;
        let button = state.theme.button_font;
        let cjk = state.theme.cjk_font;
        let mono_slots = [
            Slot::SettingsEndpoint,
            Slot::SettingsModel,
            Slot::SettingsCredentialTarget,
            Slot::SettingsApiKey,
            Slot::PromptsId,
            Slot::PromptsSystemWell,
            Slot::PromptsUserWell,
            Slot::HistoryTarget,
            Slot::HistoryContext,
            Slot::HistoryOutput,
        ];
        let cjk_slots = [Slot::HistoryList];
        for placement in &state.handles.placements {
            let font = if mono_slots.contains(&placement.slot) {
                mono
            } else if cjk_slots.contains(&placement.slot) {
                cjk
            } else {
                body
            };
            unsafe {
                let _ = SendMessageW(
                    placement.hwnd,
                    WM_SETFONT,
                    Some(WPARAM(font.0 as usize)),
                    Some(LPARAM(1)),
                );
            }
        }
        for tab in [
            state.handles.settings_nav,
            state.handles.prompts_nav,
            state.handles.history_nav,
        ] {
            unsafe {
                let _ = SendMessageW(
                    tab,
                    WM_SETFONT,
                    Some(WPARAM(button.0 as usize)),
                    Some(LPARAM(1)),
                );
            }
        }
        unsafe {
            let _ = SendMessageW(
                state.handles.status,
                WM_SETFONT,
                Some(WPARAM(state.theme.body_font.0 as usize)),
                Some(LPARAM(1)),
            );
            let _ = SendMessageW(
                state.handles.prompt_status,
                WM_SETFONT,
                Some(WPARAM(state.theme.body_font.0 as usize)),
                Some(LPARAM(1)),
            );
            let _ = SendMessageW(
                state.handles.credential_status,
                WM_SETFONT,
                Some(WPARAM(state.theme.body_font.0 as usize)),
                Some(LPARAM(1)),
            );
        }
    }

    fn apply_control_themes(state: &ManagerState) {
        for placement in &state.handles.placements {
            apply_dark_scrollbar(placement.hwnd);
        }
        for combo in [
            state.handles.language,
            state.handles.settings_selection_default,
            state.handles.settings_hover_default,
            state.handles.profile_id,
            state.handles.history_prompt,
            state.handles.history_source,
            state.handles.history_order,
        ] {
            apply_dark_scrollbar(combo);
            unsafe {
                let _ = InvalidateRect(Some(combo), None, true);
            }
        }
        apply_dark_scrollbar(state.handles.history_list);
        for editor in [
            state.handles.system_prompt,
            state.handles.user_template,
            state.handles.history_target,
            state.handles.history_context,
            state.handles.history_output,
        ] {
            apply_dark_scrollbar(editor);
        }
    }

    fn paint_manager(hwnd: HWND, state: &ManagerState) {
        let mut paint = windows::Win32::Graphics::Gdi::PAINTSTRUCT::default();
        let hdc = unsafe { BeginPaint(hwnd, &mut paint) };
        let mut client = RECT::default();
        if unsafe { GetClientRect(hwnd, &mut client) }.is_ok() {
            let logical_w = unscale_from_dpi(client.right.max(1), state.dpi);
            let logical_h = unscale_from_dpi(client.bottom.max(1), state.dpi);
            let layout = ManagerLayout::for_client(logical_w, logical_h);
            unsafe {
                let _ = FillRect(hdc, &client, state.theme.background);
                let title = scaled_rect(layout.title_bar, state.dpi);
                let tabs = scaled_rect(layout.tab_bar, state.dpi);
                let _ = FillRect(hdc, &title, state.theme.surface);
                let _ = FillRect(hdc, &tabs, state.theme.surface);
                // Bottom border under the tab bar.
                let mut border = tabs;
                border.top = border.bottom - scale_for_dpi(1, state.dpi).max(1);
                let _ = FillRect(hdc, &border, state.theme.line);
                // Accent mark.
                let mark = scaled_rect(layout.accent_mark, state.dpi);
                paint_round_rect(hdc, mark, scale_for_dpi(3, state.dpi), theme::ACCENT, None);
                // Title text.
                let mut title_text = scaled_rect(layout.title_text, state.dpi);
                let old = SelectObject(hdc, HGDIOBJ(state.theme.title_font.0));
                SetBkMode(hdc, TRANSPARENT);
                SetTextColor(hdc, theme::INK);
                let mut text: Vec<u16> = ui_text(state.language(), TextKey::WindowTitle)
                    .encode_utf16()
                    .collect();
                let _ = DrawTextW(
                    hdc,
                    &mut text,
                    &mut title_text,
                    DRAW_TEXT_FORMAT(0x0020 | 0x0100 | 0x0800),
                );
                // Status chip: 8px ok dot (text comes from the status static).
                let dot = scaled_rect(layout.status_dot, state.dpi);
                let dot_brush = CreateSolidBrush(theme::OK);
                let _ = FillRect(hdc, &dot, dot_brush);
                let _ = DeleteObject(dot_brush.into());
                let _ = SelectObject(hdc, old);
            }
        }
        unsafe {
            let _ = EndPaint(hwnd, &paint);
        }
    }

    fn page_identity(state: &ManagerState, hwnd: HWND) -> Option<View> {
        if hwnd == state.handles.settings_page {
            Some(View::Settings)
        } else if hwnd == state.handles.prompts_page {
            Some(View::Prompts)
        } else if hwnd == state.handles.history_page {
            Some(View::History)
        } else {
            None
        }
    }

    fn paint_page(hwnd: HWND) {
        let Ok(parent) = (unsafe { GetParent(hwnd) }) else {
            return;
        };
        let state_ptr = unsafe { GetWindowLongPtrW(parent, GWLP_USERDATA) as *const ManagerState };
        if state_ptr.is_null() {
            return;
        }
        let state = unsafe { &*state_ptr };
        let Some(view) = page_identity(state, hwnd) else {
            return;
        };
        let mut paint = windows::Win32::Graphics::Gdi::PAINTSTRUCT::default();
        let hdc = unsafe { BeginPaint(hwnd, &mut paint) };
        let mut client = RECT::default();
        if unsafe { GetClientRect(hwnd, &mut client) }.is_ok() {
            unsafe {
                let _ = FillRect(hdc, &client, state.theme.background);
            }
            let w = unscale_from_dpi(client.right.max(1), state.dpi);
            let h = unscale_from_dpi(client.bottom.max(1), state.dpi);
            let layout = page_layout(view, w, h);
            let radius = scale_for_dpi(theme::RADIUS_CARD, state.dpi);
            for group in layout.groups {
                if group.right - group.left <= 2 && group.bottom - group.top <= 2 {
                    continue;
                }
                let g = scaled_rect(group, state.dpi);
                paint_round_rect(hdc, g, radius, theme::RAISED, Some(theme::LINE));
            }
            // Captions (uppercase, letter-spaced, muted) + field labels painted
            // directly so STATIC defaults cannot produce black-on-black text.
            let caption_slots: &[(Slot, TextKey)] = match view {
                View::Settings => &[
                    (Slot::SettingsProviderCap, TextKey::GroupProvider),
                    (Slot::SettingsCredentialsCap, TextKey::GroupCredentials),
                    (Slot::SettingsDefaultsCap, TextKey::GroupDefaults),
                ],
                View::Prompts => &[
                    (Slot::PromptsSystemCap, TextKey::SystemPrompt),
                    (Slot::PromptsUserCap, TextKey::UserTemplate),
                ],
                View::History => &[
                    (Slot::HistorySearchCap, TextKey::GroupSearch),
                    (Slot::HistoryEntriesCap, TextKey::GroupEntries),
                    (Slot::HistorySelectionCap, TextKey::Selection),
                    (Slot::HistoryOutputCap, TextKey::Output),
                ],
            };
            let label_slots: &[(Slot, TextKey)] = match view {
                View::Settings => &[
                    (Slot::SettingsEndpointLabel, TextKey::Endpoint),
                    (Slot::SettingsModelLabel, TextKey::Model),
                    (
                        Slot::SettingsCredentialTargetLabel,
                        TextKey::CredentialTarget,
                    ),
                    (Slot::SettingsApiKeyLabel, TextKey::ApiKey),
                    (
                        Slot::SettingsSelectionDefaultLabel,
                        TextKey::SelectionProfile,
                    ),
                    (Slot::SettingsHoverDefaultLabel, TextKey::HoverProfile),
                    (Slot::SettingsLanguageLabel, TextKey::InterfaceLanguage),
                ],
                View::Prompts => &[
                    (Slot::PromptsIdLabel, TextKey::Id),
                    (Slot::PromptsNameLabel, TextKey::Name),
                    (Slot::PromptsModelLabel, TextKey::ModelOverride),
                    (Slot::PromptsTemperatureLabel, TextKey::Temperature),
                    (Slot::PromptsMaxTokensLabel, TextKey::MaxTokens),
                ],
                View::History => &[],
            };
            unsafe {
                let old = SelectObject(hdc, HGDIOBJ(state.theme.caption_font.0));
                SetBkMode(hdc, TRANSPARENT);
                SetTextCharacterExtra(hdc, scale_for_dpi(1, state.dpi));
                SetTextColor(hdc, theme::MUTED);
                for (slot, key) in caption_slots {
                    if let Some(r) = layout.rect_for(*slot) {
                        let mut r = scaled_rect(r, state.dpi);
                        let caption = ui_text(state.language(), *key).to_uppercase();
                        let mut text: Vec<u16> = caption.encode_utf16().collect();
                        let _ = DrawTextW(
                            hdc,
                            &mut text,
                            &mut r,
                            DRAW_TEXT_FORMAT(0x0020 | 0x0100 | 0x0800),
                        );
                    }
                }
                SetTextCharacterExtra(hdc, 0);
                SetTextColor(hdc, theme::MUTED);
                let old_body = SelectObject(hdc, HGDIOBJ(state.theme.body_font.0));
                for (slot, key) in label_slots {
                    if let Some(r) = layout.rect_for(*slot) {
                        let mut r = scaled_rect(r, state.dpi);
                        // Paint even if localization is missing so no black gap remains.
                        let label = ui_text(state.language(), *key);
                        let label = if label.is_empty() { "Label" } else { label };
                        let mut text: Vec<u16> = label.encode_utf16().collect();
                        SetTextColor(hdc, theme::INK);
                        let _ = DrawTextW(
                            hdc,
                            &mut text,
                            &mut r,
                            DRAW_TEXT_FORMAT(0x0020 | 0x0100 | 0x0800),
                        );
                    }
                }
                let _ = SelectObject(hdc, old_body);
                SetTextCharacterExtra(hdc, 0);
                // Editor / output wells (void + line).
                let wells: &[Slot] = match view {
                    View::Settings => &[],
                    View::Prompts => &[Slot::PromptsSystemWell, Slot::PromptsUserWell],
                    View::History => &[
                        Slot::HistoryList,
                        Slot::HistoryTarget,
                        Slot::HistoryContext,
                        Slot::HistoryOutput,
                    ],
                };
                for slot in wells {
                    if let Some(r) = layout.rect_for(*slot) {
                        let r = scaled_rect(r, state.dpi);
                        paint_round_rect(
                            hdc,
                            r,
                            scale_for_dpi(WELL_RADIUS, state.dpi),
                            theme::VOID,
                            Some(theme::LINE),
                        );
                    }
                }
                SetTextCharacterExtra(hdc, 0);
                let _ = SelectObject(hdc, old);
            }
        }
        unsafe {
            let _ = EndPaint(hwnd, &paint);
        }
    }

    fn paint_round_rect(
        hdc: windows::Win32::Graphics::Gdi::HDC,
        rect: RECT,
        radius: i32,
        fill: COLORREF,
        border: Option<COLORREF>,
    ) {
        let fill_brush = unsafe { CreateSolidBrush(fill) };
        let region = unsafe {
            CreateRoundRectRgn(
                rect.left,
                rect.top,
                rect.right + 1,
                rect.bottom + 1,
                radius.max(2),
                radius.max(2),
            )
        };
        unsafe {
            if !region.0.is_null() {
                let _ = FillRgn(hdc, region, fill_brush);
                if let Some(color) = border {
                    let frame = CreateSolidBrush(color);
                    let _ = FrameRgn(hdc, region, frame, 1, 1);
                    let _ = DeleteObject(frame.into());
                }
                let _ = DeleteObject(region.into());
            } else {
                let _ = FillRect(hdc, &rect, fill_brush);
            }
            let _ = DeleteObject(fill_brush.into());
        }
    }

    fn themed_control_color(
        state: &ManagerState,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        let hdc = windows::Win32::Graphics::Gdi::HDC(wparam.0 as *mut _);
        let child = HWND(lparam.0 as *mut c_void);
        let is_label = message == 0x0138;
        let is_credential_status = child == state.handles.credential_status;
        let is_prompt_status = child == state.handles.prompt_status;
        let is_tab_status = child == state.handles.status;
        let is_history_meta = child == state.handles.history_meta;
        let is_muted = is_tab_status || is_history_meta || is_prompt_status;
        let present =
            is_credential_status && state.credential_status == CredentialStatusState::Present;
        let (background, brush) = if is_tab_status {
            (theme::SURFACE, state.theme.surface)
        } else if is_label {
            // Solid raised so labels match their field-group card.
            (theme::RAISED, state.theme.raised)
        } else {
            (theme::RAISED, state.theme.raised)
        };
        unsafe {
            let text = if present {
                theme::OK
            } else if is_muted {
                theme::MUTED
            } else if is_credential_status {
                match state.credential_status {
                    CredentialStatusState::Unavailable(_) => theme::ERR,
                    _ => theme::MUTED,
                }
            } else {
                theme::INK
            };
            SetTextColor(hdc, text);
            SetBkColor(hdc, background);
        }
        LRESULT(brush.0 as isize)
    }

    fn button_kind(id: usize) -> ButtonKind {
        match id {
            ID_SETTINGS_TAB | ID_PROMPTS_TAB | ID_HISTORY_TAB => ButtonKind::Tab,
            ID_SAVE_SETTINGS | ID_SAVE_KEY | ID_SAVE_PROMPT | ID_HISTORY_REFRESH => {
                ButtonKind::Primary
            }
            ID_DELETE_KEY | ID_HISTORY_DELETE => ButtonKind::Danger,
            _ => ButtonKind::Default,
        }
    }

    fn draw_manager_item(item: &DRAWITEMSTRUCT, state: &ManagerState) {
        if item.CtlType == ODT_LISTBOX {
            draw_history_row(item, state);
            return;
        }
        if item.CtlType != ODT_BUTTON {
            return;
        }
        let pressed = item.itemState.0 & 0x0001 != 0;
        let disabled = item.itemState.0 & 0x0004 != 0;
        let focused = item.itemState.0 & 0x0010 != 0;
        let kind = button_kind(item.CtlID as usize);
        let selected_tab = kind == ButtonKind::Tab
            && matches!(
                (item.CtlID as usize, state.view),
                (ID_SETTINGS_TAB, View::Settings)
                    | (ID_PROMPTS_TAB, View::Prompts)
                    | (ID_HISTORY_TAB, View::History)
            );
        let (fill, text_color, border_color) = if disabled {
            (theme::VOID, theme::MUTED, theme::LINE)
        } else {
            match kind {
                ButtonKind::Primary => {
                    let fill = if pressed {
                        blend(theme::ACCENT, theme::VOID, 0.85)
                    } else {
                        theme::ACCENT
                    };
                    (fill, theme::ON_ACCENT, fill)
                }
                ButtonKind::Danger => (
                    blend(theme::ERR, theme::RAISED, 0.08),
                    theme::ERR,
                    blend(theme::ERR, theme::RAISED, 0.35),
                ),
                ButtonKind::Tab if selected_tab => (
                    blend(theme::ACCENT, theme::VOID, 0.14),
                    theme::ACCENT,
                    if focused { theme::ACCENT } else { theme::LINE },
                ),
                ButtonKind::Tab => (
                    if pressed {
                        blend(theme::ACCENT, theme::VOID, 0.08)
                    } else {
                        theme::SURFACE
                    },
                    theme::MUTED,
                    theme::SURFACE,
                ),
                ButtonKind::Default => (
                    if pressed {
                        blend(theme::LINE, theme::RAISED, 0.5)
                    } else {
                        theme::RAISED
                    },
                    theme::INK,
                    if focused { theme::ACCENT } else { theme::LINE },
                ),
            }
        };
        let fill_brush = unsafe { CreateSolidBrush(fill) };
        let border_brush = unsafe { CreateSolidBrush(border_color) };
        let mut rect = item.rcItem;
        let radius = scale_for_dpi(theme::RADIUS_CONTROL, state.dpi).max(2);
        let region = unsafe {
            CreateRoundRectRgn(
                rect.left,
                rect.top,
                rect.right + 1,
                rect.bottom + 1,
                radius,
                radius,
            )
        };
        unsafe {
            if !region.0.is_null() {
                let _ = FillRgn(item.hDC, region, fill_brush);
                let _ = FrameRgn(item.hDC, region, border_brush, 1, 1);
                let _ = DeleteObject(region.into());
            } else {
                let _ = FillRect(item.hDC, &rect, fill_brush);
            }
            let _ = DeleteObject(fill_brush.into());
            let _ = DeleteObject(border_brush.into());
            let length = GetWindowTextLengthW(item.hwndItem).max(0) as usize;
            let mut text = vec![0u16; length + 1];
            let written = GetWindowTextW(item.hwndItem, &mut text).max(0) as usize;
            // WM_GETFONT
            let get_font = SendMessageW(item.hwndItem, 0x0031, Some(WPARAM(0)), Some(LPARAM(0)));
            let old = if get_font.0 != 0 {
                Some(SelectObject(item.hDC, HGDIOBJ(get_font.0 as *mut _)))
            } else {
                None
            };
            SetBkMode(item.hDC, TRANSPARENT);
            SetTextColor(item.hDC, text_color);
            let drawn = written.min(text.len());
            let _ = DrawTextW(
                item.hDC,
                &mut text[..drawn],
                &mut rect,
                DRAW_TEXT_FORMAT(0x0001 | 0x0020 | 0x0100 | 0x0800),
            );
            if focused {
                let inset = scale_for_dpi(3, state.dpi);
                rect.left += inset;
                rect.top += inset;
                rect.right -= inset;
                rect.bottom -= inset;
                let _ = DrawFocusRect(item.hDC, &rect);
            }
            if let Some(old) = old {
                let _ = SelectObject(item.hDC, old);
            }
        }
    }

    fn draw_history_row(item: &DRAWITEMSTRUCT, state: &ManagerState) {
        let index = item.itemID as usize;
        let Some(entry) = state.history_entries.get(index) else {
            return;
        };
        let selected = item.itemState.0 & 0x0001 != 0;
        let mut rect = item.rcItem;
        let fill = if selected {
            blend(theme::ACCENT, theme::VOID, 0.10)
        } else {
            theme::VOID
        };
        unsafe {
            let brush = CreateSolidBrush(fill);
            let _ = FillRect(item.hDC, &rect, brush);
            let _ = DeleteObject(brush.into());
            if selected {
                let edge = scale_for_dpi(3, state.dpi);
                let accent = CreateSolidBrush(theme::ACCENT);
                let edge_rect = RECT {
                    left: rect.left,
                    top: rect.top,
                    right: rect.left + edge,
                    bottom: rect.bottom,
                };
                let _ = FillRect(item.hDC, &edge_rect, accent);
                let _ = DeleteObject(accent.into());
            }
            let line = CreateSolidBrush(theme::LINE);
            let sep = RECT {
                left: rect.left,
                top: rect.bottom - 1,
                right: rect.right,
                bottom: rect.bottom,
            };
            let _ = FillRect(item.hDC, &sep, line);
            let _ = DeleteObject(line.into());

            let pad_x = scale_for_dpi(10, state.dpi);
            rect.left += pad_x
                + if selected {
                    scale_for_dpi(3, state.dpi)
                } else {
                    0
                };
            rect.right -= pad_x;
            SetBkMode(item.hDC, TRANSPARENT);

            // Target (mono, ink)
            let mut line1 = rect;
            line1.bottom = line1.top + scale_for_dpi(20, state.dpi);
            let mono = SelectObject(item.hDC, HGDIOBJ(state.theme.mono_font.0));
            SetTextColor(item.hDC, theme::INK);
            let mut text: Vec<u16> = single_line(&entry.target).encode_utf16().collect();
            let _ = DrawTextW(
                item.hDC,
                &mut text,
                &mut line1,
                DRAW_TEXT_FORMAT(0x0020 | 0x0100 | 0x0800 | 0x0010),
            );

            // Preview (CJK muted)
            let mut line2 = rect;
            line2.top = line1.bottom;
            line2.bottom = line2.top + scale_for_dpi(18, state.dpi);
            let _ = SelectObject(item.hDC, HGDIOBJ(state.theme.cjk_font.0));
            SetTextColor(item.hDC, theme::MUTED);
            let mut text: Vec<u16> = single_line(&entry.output).encode_utf16().collect();
            let _ = DrawTextW(
                item.hDC,
                &mut text,
                &mut line2,
                DRAW_TEXT_FORMAT(0x0020 | 0x0100 | 0x0800 | 0x0010),
            );

            // time · profile (accent on profile)
            let _ = SelectObject(item.hDC, HGDIOBJ(state.theme.mono_font.0));
            let mut line3 = rect;
            line3.top = line2.bottom;
            line3.bottom = line3.top + scale_for_dpi(16, state.dpi);
            SetTextColor(item.hDC, theme::MUTED);
            let time = short_time(&entry.created_at_utc);
            let mut text: Vec<u16> = format!("{time} · ").encode_utf16().collect();
            let _ = DrawTextW(
                item.hDC,
                &mut text,
                &mut line3,
                DRAW_TEXT_FORMAT(0x0020 | 0x0100 | 0x0800),
            );
            // Approximate the left edge for the profile name.
            let mut probe = line3;
            let mut prefix: Vec<u16> = format!("{time} · ").encode_utf16().collect();
            let _ = DrawTextW(
                item.hDC,
                &mut prefix,
                &mut probe,
                DRAW_TEXT_FORMAT(0x0400 | 0x0800), // DT_CALCRECT | DT_NOPREFIX
            );
            line3.left = probe.right;
            SetTextColor(item.hDC, theme::ACCENT);
            let mut text: Vec<u16> = single_line(&entry.prompt_id).encode_utf16().collect();
            let _ = DrawTextW(
                item.hDC,
                &mut text,
                &mut line3,
                DRAW_TEXT_FORMAT(0x0020 | 0x0100 | 0x0800),
            );
            let _ = SelectObject(item.hDC, mono);
        }
    }

    fn short_time(created_at_utc: &str) -> String {
        // "2026-08-19T14:22:00Z" → "14:22" when possible.
        let bytes = created_at_utc.as_bytes();
        if created_at_utc.len() >= 16 && bytes[10] == b'T' {
            created_at_utc[11..16].to_owned()
        } else {
            single_line(created_at_utc)
        }
    }

    /// Page children are visible within their initially hidden container.
    /// Exactly one container is shown by `show_view` after initialization.
    fn visibility_style(view: Option<View>) -> WINDOW_STYLE {
        let _ = view;
        WS_VISIBLE
    }

    fn handle_command(hwnd: HWND, state: &mut ManagerState, command: usize) -> LRESULT {
        let id = command & 0xffff;
        let notification = (command >> 16) & 0xffff;
        if id == ID_HISTORY_LIST && notification == LBN_SELCHANGE {
            history_selection_changed(state);
            return LRESULT(0);
        }
        if id == ID_PROMPT_ID && notification == CBN_SELCHANGE {
            prompt_profile_selected(state);
            return LRESULT(0);
        }
        if state.view == View::History
            && notification == CBN_SELCHANGE
            && matches!(id, ID_HISTORY_PROMPT | ID_HISTORY_SOURCE | ID_HISTORY_ORDER)
        {
            refresh_history(state);
            return LRESULT(0);
        }
        if id == ID_LANGUAGE && notification == CBN_SELCHANGE {
            save_manager_language(hwnd, state);
            return LRESULT(0);
        }
        match id {
            ID_WIN_MIN => unsafe {
                let _ = SendMessageW(
                    hwnd,
                    WM_SYSCOMMAND,
                    Some(WPARAM(SC_MINIMIZE as usize)),
                    None,
                );
            },
            ID_WIN_MAX => {
                let is_zoomed = unsafe { IsZoomed(hwnd).as_bool() };
                let cmd = if is_zoomed {
                    SC_RESTORE as usize
                } else {
                    SC_MAXIMIZE as usize
                };
                unsafe {
                    let _ = SendMessageW(hwnd, WM_SYSCOMMAND, Some(WPARAM(cmd)), None);
                }
            }
            ID_WIN_CLOSE => unsafe {
                let _ = SendMessageW(hwnd, WM_SYSCOMMAND, Some(WPARAM(SC_CLOSE as usize)), None);
            },
            ID_SETTINGS_TAB => switch_view(hwnd, state, View::Settings),
            ID_PROMPTS_TAB => switch_view(hwnd, state, View::Prompts),
            ID_HISTORY_TAB => switch_view(hwnd, state, View::History),
            ID_SAVE_SETTINGS => save_settings(state),
            ID_SAVE_KEY => save_key(state),
            ID_DELETE_KEY => delete_key(state),
            ID_SAVE_PROMPT => save_prompt(state),
            ID_HISTORY_REFRESH => refresh_history(state),
            ID_HISTORY_COPY => copy_history_output(hwnd, state),
            ID_HISTORY_DELETE => delete_history(hwnd, state),
            _ => {}
        }
        LRESULT(0)
    }

    fn switch_view(hwnd: HWND, state: &mut ManagerState, view: View) {
        state.view = view;
        show_view(hwnd, state);
        if view == View::Settings {
            refresh_settings_form(state);
            update_credential_status(state);
        } else if view == View::History {
            refresh_history(state);
        }
    }

    fn show_view(hwnd: HWND, state: &ManagerState) {
        // Switch visibility at the page-container boundary.  The page
        // containers own every page-specific child, so there is no stale
        // per-control visibility state that can leak across tabs.
        for (page, visible) in [
            (View::Settings, state.handles.settings_page),
            (View::Prompts, state.handles.prompts_page),
            (View::History, state.handles.history_page),
        ] {
            set_page_visibility(visible, page_is_visible(page, state.view));
        }
        unsafe {
            for button in [
                state.handles.settings_nav,
                state.handles.prompts_nav,
                state.handles.history_nav,
            ] {
                let _ = InvalidateRect(Some(button), None, true);
                let _ = UpdateWindow(button);
            }
            let _ = InvalidateRect(Some(hwnd), None, true);
            let _ = UpdateWindow(hwnd);
        }
    }

    fn set_page_visibility(hwnd: HWND, visible: bool) {
        unsafe {
            // ShowWindow owns the WS_VISIBLE transition. Mutating that style
            // bit first makes ShowWindow believe the requested state is
            // already active, which can leave a hidden page unpainted or an
            // old page's pixels on screen.
            let _ = ShowWindow(hwnd, if visible { SW_SHOW } else { SW_HIDE });
        }
    }

    /// Exactly one page container is visible after every tab selection.
    fn page_is_visible(page: View, selected_view: View) -> bool {
        page == selected_view
    }

    fn populate_language_selector(state: &ManagerState) {
        reset_combo(state.handles.language);
        for key in [TextKey::English, TextKey::SimplifiedChinese] {
            add_combo_string(state.handles.language, ui_text(state.language(), key));
        }
        set_combo_selection(
            state.handles.language,
            match state.language() {
                UiLanguage::English => 0,
                UiLanguage::SimplifiedChinese => 1,
            },
        );
    }

    fn apply_static_localization(hwnd: HWND, state: &ManagerState) {
        set_text(hwnd, ui_text(state.language(), TextKey::WindowTitle));
        for control in &state.handles.localized {
            set_text(control.hwnd, ui_text(state.language(), control.key));
            unsafe {
                let _ = InvalidateRect(Some(control.hwnd), None, true);
            }
        }
        populate_language_selector(state);
        populate_history_filters(state);
        populate_prompt_id_list(state);
        set_edit_cue(
            state.handles.history_search,
            ui_text(state.language(), TextKey::SearchTargetOutput),
        );
        unsafe {
            let _ = InvalidateRect(Some(hwnd), None, true);
            for page in [
                state.handles.settings_page,
                state.handles.prompts_page,
                state.handles.history_page,
            ] {
                let _ = InvalidateRect(Some(page), None, true);
            }
        }
    }

    fn save_manager_language(hwnd: HWND, state: &mut ManagerState) {
        let next_language = match combo_selection(state.handles.language) {
            Some(0) => UiLanguage::English,
            Some(1) => UiLanguage::SimplifiedChinese,
            _ => {
                populate_language_selector(state);
                return;
            }
        };
        if next_language == state.language() {
            return;
        }

        // Clone the last saved model and change only the presentation field.
        // Reading any Settings/Prompts controls here would accidentally save
        // unrelated edits that the user has not committed yet.
        let next = config_with_manager_language(&state.config, next_language);
        if let Err(error) = save_config(state, &next) {
            populate_language_selector(state);
            set_status(
                state,
                &status_text(
                    state.language(),
                    StatusEvent::SaveInterfaceLanguageFailed {
                        detail: &error.to_string(),
                    },
                ),
            );
            return;
        }

        state.config = next;
        apply_static_localization(hwnd, state);
        relabel_credential_status(state);
        if selected_history_index(state.handles.history_list, state.history_entries.len()).is_some()
        {
            history_selection_changed(state);
        } else {
            clear_history_detail(state);
        }
        // Keep any prompt validation/save result visible while relabeling controls.  It may be a
        // transient message in the previous language, but clearing it makes a language switch
        // look like the operation was lost.
        set_status(
            state,
            &status_text(state.language(), StatusEvent::InterfaceLanguageSaved),
        );
    }

    fn config_with_manager_language(config: &AppConfig, language: UiLanguage) -> AppConfig {
        let mut next = config.clone();
        next.ui.manager_language = language;
        next
    }

    fn populate_history_filters(state: &ManagerState) {
        let prompt_selection = combo_selection(state.handles.history_prompt).unwrap_or(0);
        let source_selection = combo_selection(state.handles.history_source).unwrap_or(0);
        let order_selection = combo_selection(state.handles.history_order).unwrap_or(0);
        reset_combo(state.handles.history_prompt);
        add_combo_string(
            state.handles.history_prompt,
            ui_text(state.language(), TextKey::AllPrompts),
        );
        for profile in &state.config.profiles {
            add_combo_string(
                state.handles.history_prompt,
                &format!("{} — {}", profile.id, profile.name),
            );
        }
        set_combo_selection(
            state.handles.history_prompt,
            prompt_selection.min(state.config.profiles.len()),
        );

        reset_combo(state.handles.history_source);
        for key in [
            TextKey::AllSources,
            TextKey::Selection,
            TextKey::Hover,
            TextKey::Clipboard,
            TextKey::Ocr,
        ] {
            add_combo_string(state.handles.history_source, ui_text(state.language(), key));
        }
        set_combo_selection(state.handles.history_source, source_selection.min(4));

        reset_combo(state.handles.history_order);
        add_combo_string(
            state.handles.history_order,
            ui_text(state.language(), TextKey::Newest),
        );
        add_combo_string(
            state.handles.history_order,
            ui_text(state.language(), TextKey::Oldest),
        );
        set_combo_selection(state.handles.history_order, order_selection.min(1));
    }

    /// Rebuild the Settings-page default selectors from the saved profile list.
    ///
    /// The combo item text is presentation-only: the selected item's index is
    /// resolved back to the profile ID when settings are saved. This keeps the
    /// TOML format stable while letting users choose profiles by their friendly
    /// names instead of memorizing IDs.
    fn populate_profile_selectors(state: &ManagerState) {
        let selection =
            selected_profile_index(&state.config.profiles, &state.config.defaults.selection);
        let hover = selected_profile_index(&state.config.profiles, &state.config.defaults.hover);
        for (hwnd, selected) in [
            (state.handles.settings_selection_default, selection),
            (state.handles.settings_hover_default, hover),
        ] {
            reset_combo(hwnd);
            for profile in &state.config.profiles {
                add_combo_string(hwnd, &profile_option_label(profile));
            }
            if let Some(index) = selected {
                set_combo_selection(hwnd, index);
            }
        }
    }

    fn selected_profile_index(profiles: &[PromptConfig], id: &str) -> Option<usize> {
        profiles.iter().position(|profile| profile.id == id)
    }

    fn profile_option_label(profile: &PromptConfig) -> String {
        let name = profile.name.trim();
        if name.is_empty() {
            profile.id.clone()
        } else {
            format!("{name} — {}", profile.id)
        }
    }

    fn selected_profile_id(hwnd: HWND, profiles: &[PromptConfig]) -> Option<String> {
        profile_id_for_index(combo_selection(hwnd), profiles)
    }

    fn profile_id_for_index(index: Option<usize>, profiles: &[PromptConfig]) -> Option<String> {
        index
            .and_then(|index| profiles.get(index))
            .map(|profile| profile.id.clone())
    }

    fn populate_prompt_id_list(state: &ManagerState) {
        let previous = combo_selection(state.handles.profile_id).unwrap_or(state.profile_index);
        reset_combo(state.handles.profile_id);
        for profile in &state.config.profiles {
            add_combo_string(state.handles.profile_id, &profile.id);
        }
        if !state.config.profiles.is_empty() {
            let index = previous.min(state.config.profiles.len().saturating_sub(1));
            set_combo_selection(state.handles.profile_id, index);
            set_text(state.handles.profile_id, &state.config.profiles[index].id);
        }
    }

    fn prompt_profile_selected(state: &mut ManagerState) {
        let Some(index) = combo_selection(state.handles.profile_id) else {
            return;
        };
        if index >= state.config.profiles.len() {
            return;
        }
        state.profile_index = index;
        refresh_prompt_form(state);
    }

    fn refresh_history(state: &mut ManagerState) {
        state.history_loaded = true;
        let query = history_query(state);
        let result = default_history_path()
            .ok_or_else(|| {
                status_text(
                    state.language(),
                    StatusEvent::LocalAppDataUnavailable {
                        operation: StatusOperation::History,
                    },
                )
            })
            .and_then(|path| HistoryDatabase::open(path).map_err(|error| error.to_string()))
            .and_then(|database| database.search(&query).map_err(|error| error.to_string()));
        match result {
            Ok(entries) => {
                state.history_entries = entries;
                populate_history_list(state);
                set_text(
                    state.handles.history_count,
                    &status_text(
                        state.language(),
                        StatusEvent::HistoryCount {
                            count: state.history_entries.len(),
                        },
                    ),
                );
                set_status(
                    state,
                    &status_text(state.language(), StatusEvent::HistoryRefreshed),
                );
            }
            Err(error) => {
                state.history_entries.clear();
                populate_history_list(state);
                clear_history_detail(state);
                let message = status_text(
                    state.language(),
                    StatusEvent::HistoryUnavailable { detail: &error },
                );
                set_text(state.handles.history_count, &message);
                set_status(state, &message);
            }
        }
    }

    fn history_query(state: &ManagerState) -> HistoryQuery {
        let search = nonempty(read_text(state.handles.history_search));
        let prompt_id = selected_history_prompt(state);
        let source = selected_history_source(state);
        let order = match combo_selection(state.handles.history_order) {
            Some(1) => HistoryOrder::OldestFirst,
            _ => HistoryOrder::NewestFirst,
        };
        HistoryQuery {
            search,
            prompt_id,
            source,
            order,
            ..HistoryQuery::default()
        }
    }

    fn selected_history_prompt(state: &ManagerState) -> Option<String> {
        let index = combo_selection(state.handles.history_prompt)?;
        index.checked_sub(1).and_then(|profile_index| {
            state
                .config
                .profiles
                .get(profile_index)
                .map(|profile| profile.id.clone())
        })
    }

    fn selected_history_source(state: &ManagerState) -> Option<ExtractionSource> {
        history_source_for_index(combo_selection(state.handles.history_source))
    }

    fn history_source_for_index(index: Option<usize>) -> Option<ExtractionSource> {
        match index {
            Some(1) => Some(ExtractionSource::UiaSelection),
            Some(2) => Some(ExtractionSource::UiaPoint),
            Some(3) => Some(ExtractionSource::Clipboard),
            Some(4) => Some(ExtractionSource::Ocr),
            _ => None,
        }
    }

    fn populate_history_list(state: &ManagerState) {
        unsafe {
            let _ = SendMessageW(
                state.handles.history_list,
                LB_RESETCONTENT,
                Some(WPARAM(0)),
                Some(LPARAM(0)),
            );
        }
        for entry in &state.history_entries {
            add_list_string(state.handles.history_list, &format_history_row(entry));
        }
        clear_history_detail(state);
    }

    fn history_selection_changed(state: &mut ManagerState) {
        let index = selected_history_index(state.handles.history_list, state.history_entries.len());
        if let Some(index) = index {
            if let Some(entry) = state.history_entries.get(index) {
                set_text(state.handles.history_target, &entry.target);
                let fallback = status_text(state.language(), StatusEvent::NoContext);
                set_text(
                    state.handles.history_context,
                    entry.context.as_deref().unwrap_or(fallback.as_str()),
                );
                set_text(state.handles.history_output, &entry.output);
                set_text(
                    state.handles.history_meta,
                    &format!(
                        "{} · {} · {} · {}{}",
                        entry.created_at_utc,
                        source_label(state.language(), entry.source),
                        entry.prompt_id,
                        entry.model,
                        if entry.served_from_cache {
                            match state.language() {
                                UiLanguage::English => " · cache",
                                UiLanguage::SimplifiedChinese => " · 缓存",
                            }
                        } else {
                            ""
                        },
                    ),
                );
                return;
            }
        }
        clear_history_detail(state);
    }

    fn clear_history_detail(state: &ManagerState) {
        set_text(state.handles.history_target, "");
        set_text(state.handles.history_context, "");
        set_text(state.handles.history_output, "");
        set_text(state.handles.history_meta, "");
        if state.history_loaded {
            set_text(
                state.handles.history_count,
                &status_text(
                    state.language(),
                    StatusEvent::HistoryCount {
                        count: state.history_entries.len(),
                    },
                ),
            );
        }
    }

    #[cfg(test)]
    fn history_count_text(language: UiLanguage, count: usize) -> String {
        status_text(language, StatusEvent::HistoryCount { count })
    }

    fn copy_history_output(_hwnd: HWND, state: &mut ManagerState) {
        let Some(index) =
            selected_history_index(state.handles.history_list, state.history_entries.len())
        else {
            set_status(
                state,
                &status_text(state.language(), StatusEvent::SelectHistoryEntry),
            );
            return;
        };
        let Some(entry) = state.history_entries.get(index) else {
            set_status(
                state,
                &status_text(state.language(), StatusEvent::HistoryEntryUnavailable),
            );
            return;
        };
        match copy_text_to_clipboard(&entry.output, state.language()) {
            Ok(()) => set_status(
                state,
                &status_text(state.language(), StatusEvent::OutputCopied),
            ),
            Err(error) => set_status(
                state,
                &status_text(
                    state.language(),
                    StatusEvent::CopyOutputFailed { detail: &error },
                ),
            ),
        }
    }

    fn delete_history(hwnd: HWND, state: &mut ManagerState) {
        let Some(index) =
            selected_history_index(state.handles.history_list, state.history_entries.len())
        else {
            set_status(
                state,
                &status_text(state.language(), StatusEvent::SelectHistoryEntry),
            );
            return;
        };
        let Some(entry) = state.history_entries.get(index) else {
            set_status(
                state,
                &status_text(state.language(), StatusEvent::HistoryEntryUnavailable),
            );
            return;
        };
        let confirmation = status_text(
            state.language(),
            StatusEvent::DeleteHistoryConfirm {
                target: &single_line(&entry.target),
            },
        );
        let message = wide(&confirmation);
        let title = wide(ui_text(state.language(), TextKey::WindowTitle));
        let result = unsafe {
            MessageBoxW(
                Some(hwnd),
                PCWSTR(message.as_ptr()),
                PCWSTR(title.as_ptr()),
                MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2,
            )
        };
        if result != IDYES {
            set_status(
                state,
                &status_text(state.language(), StatusEvent::DeletionCancelled),
            );
            return;
        }
        let result = default_history_path()
            .ok_or_else(|| {
                status_text(
                    state.language(),
                    StatusEvent::LocalAppDataUnavailable {
                        operation: StatusOperation::History,
                    },
                )
            })
            .and_then(|path| HistoryDatabase::open(path).map_err(|error| error.to_string()))
            .and_then(|database| {
                database
                    .delete_one(entry.id)
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(true) => {
                set_status(
                    state,
                    &status_text(state.language(), StatusEvent::HistoryEntryDeleted),
                );
                refresh_history(state);
            }
            Ok(false) => set_status(
                state,
                &status_text(state.language(), StatusEvent::HistoryEntryAlreadyDeleted),
            ),
            Err(error) => set_status(
                state,
                &status_text(
                    state.language(),
                    StatusEvent::DeleteHistoryFailed { detail: &error },
                ),
            ),
        }
    }

    fn format_history_row(entry: &HistoryEntry) -> String {
        format!(
            "{}  |  {}",
            single_line(&entry.target),
            single_line(&entry.output)
        )
    }

    fn single_line(value: &str) -> String {
        let mut result = value.replace(['\r', '\n', '\t'], " ");
        if result.chars().count() > 180 {
            result = result.chars().take(177).collect();
            result.push_str("...");
        }
        result
    }

    fn source_label(language: UiLanguage, source: ExtractionSource) -> &'static str {
        match (language, source) {
            (UiLanguage::English, ExtractionSource::UiaSelection) => "selection",
            (UiLanguage::English, ExtractionSource::UiaPoint) => "hover",
            (UiLanguage::English, ExtractionSource::Clipboard) => "clipboard",
            (_, ExtractionSource::Ocr) => "OCR",
            (UiLanguage::SimplifiedChinese, ExtractionSource::UiaSelection) => "划词",
            (UiLanguage::SimplifiedChinese, ExtractionSource::UiaPoint) => "悬停",
            (UiLanguage::SimplifiedChinese, ExtractionSource::Clipboard) => "剪贴板",
        }
    }

    fn selected_history_index(hwnd: HWND, length: usize) -> Option<usize> {
        let selected =
            unsafe { SendMessageW(hwnd, LB_GETCURSEL, Some(WPARAM(0)), Some(LPARAM(0))).0 };
        valid_history_index(selected, length)
    }

    fn valid_history_index(selected: isize, length: usize) -> Option<usize> {
        usize::try_from(selected)
            .ok()
            .filter(|index| *index < length)
    }

    fn reset_combo(hwnd: HWND) {
        unsafe {
            let _ = SendMessageW(hwnd, CB_RESETCONTENT, Some(WPARAM(0)), Some(LPARAM(0)));
        }
    }

    fn add_combo_string(hwnd: HWND, value: &str) {
        let value = wide(value);
        unsafe {
            let _ = SendMessageW(
                hwnd,
                CB_ADDSTRING,
                Some(WPARAM(0)),
                Some(LPARAM(value.as_ptr() as isize)),
            );
        }
    }

    fn set_combo_selection(hwnd: HWND, index: usize) {
        unsafe {
            let _ = SendMessageW(hwnd, CB_SETCURSEL, Some(WPARAM(index)), Some(LPARAM(0)));
        }
    }

    fn combo_selection(hwnd: HWND) -> Option<usize> {
        let value = unsafe { SendMessageW(hwnd, CB_GETCURSEL, Some(WPARAM(0)), Some(LPARAM(0))).0 };
        usize::try_from(value).ok()
    }

    fn add_list_string(hwnd: HWND, value: &str) {
        let value = wide(value);
        unsafe {
            let _ = SendMessageW(
                hwnd,
                LB_ADDSTRING,
                Some(WPARAM(0)),
                Some(LPARAM(value.as_ptr() as isize)),
            );
        }
    }

    fn copy_text_to_clipboard(value: &str, language: UiLanguage) -> Result<(), String> {
        let mut utf16: Vec<u16> = value.encode_utf16().collect();
        utf16.push(0);
        let bytes = utf16
            .len()
            .checked_mul(std::mem::size_of::<u16>())
            .ok_or_else(|| status_text(language, StatusEvent::OutputTooLarge))?;
        let memory =
            unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes) }.map_err(|error| error.to_string())?;
        let pointer = unsafe { GlobalLock(memory) }.cast::<u16>();
        if pointer.is_null() {
            unsafe {
                GlobalFree(memory);
            }
            return Err(status_text(
                language,
                StatusEvent::ClipboardMemoryLockFailed,
            ));
        }
        unsafe {
            std::ptr::copy_nonoverlapping(utf16.as_ptr(), pointer, utf16.len());
            let _ = GlobalUnlock(memory);
        }
        unsafe { OpenClipboard(None) }.map_err(|error| {
            unsafe {
                GlobalFree(memory);
            }
            error.to_string()
        })?;
        let result = unsafe { EmptyClipboard() }
            .and_then(|_| unsafe {
                SetClipboardData(CF_UNICODETEXT.0.into(), Some(HANDLE(memory.0)))
            })
            .map_err(|error| error.to_string());
        unsafe {
            CloseClipboard().ok();
        }
        if result.is_err() {
            unsafe {
                GlobalFree(memory);
            }
        }
        result.map(|_| ())
    }

    fn refresh_settings_form(state: &ManagerState) {
        set_text(state.handles.endpoint, &state.config.provider.endpoint);
        set_text(state.handles.model, &state.config.provider.model);
        set_text(
            state.handles.credential_target,
            &state.config.provider.credential_target,
        );
        populate_profile_selectors(state);
    }

    #[cfg(test)]
    fn resident_start_status(language: UiLanguage, outcome: ResidentStartOutcome) -> String {
        status_text(language, StatusEvent::ResidentStart(outcome))
    }

    fn config_refresh_status(language: UiLanguage, outcome: RefreshOutcome) -> String {
        status_text(language, StatusEvent::ConfigRefresh(outcome))
    }

    fn credential_refresh_status(
        language: UiLanguage,
        outcome: RefreshOutcome,
        deleted: bool,
    ) -> String {
        status_text(
            language,
            StatusEvent::CredentialRefresh { outcome, deleted },
        )
    }

    fn save_settings(state: &mut ManagerState) {
        let mut next = state.config.clone();
        next.provider.endpoint = read_text(state.handles.endpoint).trim().to_owned();
        next.provider.model = read_text(state.handles.model).trim().to_owned();
        next.provider.credential_target =
            read_text(state.handles.credential_target).trim().to_owned();
        // The Settings controls display profile names, but the configuration
        // stores stable profile IDs. Keep the last valid ID if a native combo
        // has no selection (for example while it is being rebuilt).
        if let Some(id) = selected_profile_id(
            state.handles.settings_selection_default,
            &state.config.profiles,
        ) {
            next.defaults.selection = id;
        }
        if let Some(id) =
            selected_profile_id(state.handles.settings_hover_default, &state.config.profiles)
        {
            next.defaults.hover = id;
        }
        if let Err(error) = next.validate() {
            set_status(
                state,
                &status_text(
                    state.language(),
                    StatusEvent::CannotSaveSettings {
                        detail: &error.to_string(),
                    },
                ),
            );
            return;
        }
        if let Err(error) = save_config(state, &next) {
            set_status(
                state,
                &status_text(
                    state.language(),
                    StatusEvent::CannotSaveSettings {
                        detail: &error.to_string(),
                    },
                ),
            );
            return;
        }
        state.config = next;
        let refresh = notify_config_changed();
        populate_profile_selectors(state);
        populate_history_filters(state);
        set_status(state, &config_refresh_status(state.language(), refresh));
        update_credential_status(state);
    }

    fn save_key(state: &mut ManagerState) {
        let target = read_text(state.handles.credential_target).trim().to_owned();
        let secret = read_secret(state.handles.api_key);
        if secret.0.trim().is_empty() {
            set_status(
                state,
                &status_text(state.language(), StatusEvent::EnterApiKey),
            );
            return;
        }
        match credentials::write_api_key(&target, &secret.0) {
            Ok(()) => {
                set_text(state.handles.api_key, "");
                state.credential_status = CredentialStatusState::Present;
                set_text(
                    state.handles.credential_status,
                    &status_text(
                        state.language(),
                        StatusEvent::ApiKeySavedToCredentialManager,
                    ),
                );
                if target == state.config.provider.credential_target {
                    let refresh = notify_credentials_changed();
                    set_status(
                        state,
                        &credential_refresh_status(state.language(), refresh, false),
                    );
                } else {
                    set_status(
                        state,
                        &status_text(state.language(), StatusEvent::ApiKeyInactiveTargetSaved),
                    );
                }
                relabel_credential_status(state);
            }
            Err(error) => set_status(
                state,
                &status_text(
                    state.language(),
                    StatusEvent::SaveApiKeyFailed {
                        detail: &error.to_string(),
                    },
                ),
            ),
        }
    }

    fn delete_key(state: &mut ManagerState) {
        let target = read_text(state.handles.credential_target).trim().to_owned();
        match credentials::delete_api_key(&target) {
            Ok(()) => {
                state.credential_status = CredentialStatusState::Absent;
                set_text(
                    state.handles.credential_status,
                    &status_text(state.language(), StatusEvent::NoSavedApiKey),
                );
                if target == state.config.provider.credential_target {
                    let refresh = notify_credentials_changed();
                    set_status(
                        state,
                        &credential_refresh_status(state.language(), refresh, true),
                    );
                } else {
                    set_status(
                        state,
                        &status_text(state.language(), StatusEvent::ApiKeyInactiveTargetDeleted),
                    );
                }
                relabel_credential_status(state);
            }
            Err(error) => set_status(
                state,
                &status_text(
                    state.language(),
                    StatusEvent::DeleteApiKeyFailed {
                        detail: &error.to_string(),
                    },
                ),
            ),
        }
    }

    fn save_prompt(state: &mut ManagerState) {
        let prompt = match prompt_from_form(state) {
            Ok(prompt) => prompt,
            Err(error) => {
                set_text(state.handles.prompt_status, &error);
                return;
            }
        };
        let existing = state
            .config
            .profiles
            .iter()
            .position(|profile| profile.id == prompt.id);
        if existing.is_none() && state.config.profiles.is_empty() {
            // Creating the first profile via the ID field is allowed.
        }
        if existing.is_none() && prompt.id.trim().is_empty() {
            set_text(
                state.handles.prompt_status,
                &status_text(state.language(), StatusEvent::NoPromptProfile),
            );
            return;
        }
        let old_id = existing.map(|index| state.config.profiles[index].id.clone());
        let next = apply_prompt(&state.config, existing, old_id.as_deref(), prompt);
        if let Err(error) = next.validate() {
            set_text(
                state.handles.prompt_status,
                &status_text(
                    state.language(),
                    StatusEvent::CannotSavePrompt {
                        detail: &error.to_string(),
                    },
                ),
            );
            return;
        }
        if let Err(error) = save_config(state, &next) {
            set_text(
                state.handles.prompt_status,
                &status_text(
                    state.language(),
                    StatusEvent::CannotSavePrompt {
                        detail: &error.to_string(),
                    },
                ),
            );
            return;
        }
        state.config = next;
        if let Some(index) = existing {
            state.profile_index = index;
        } else if let Some(index) = state
            .config
            .profiles
            .iter()
            .position(|profile| profile.id == read_text(state.handles.profile_id).trim())
        {
            state.profile_index = index;
        } else {
            state.profile_index = state.config.profiles.len().saturating_sub(1);
        }
        let refresh = notify_config_changed();
        populate_history_filters(state);
        populate_prompt_id_list(state);
        set_text(
            state.handles.prompt_status,
            &config_refresh_status(state.language(), refresh),
        );
        refresh_prompt_form(state);
    }

    fn refresh_prompt_form(state: &mut ManagerState) {
        if let Some(prompt) = state.config.profiles.get(state.profile_index) {
            set_text(state.handles.profile_id, &prompt.id);
            set_text(state.handles.profile_name, &prompt.name);
            set_text(state.handles.system_prompt, &prompt.system_prompt);
            set_text(state.handles.user_template, &prompt.user_template);
            set_text(
                state.handles.profile_model,
                prompt.model.as_deref().unwrap_or_default(),
            );
            set_text(
                state.handles.temperature,
                &prompt
                    .temperature
                    .map_or_else(String::new, |v| v.to_string()),
            );
            set_text(
                state.handles.max_tokens,
                &prompt
                    .max_output_tokens
                    .map_or_else(String::new, |v| v.to_string()),
            );
            set_combo_selection(state.handles.profile_id, state.profile_index);
        }
    }

    fn apply_prompt(
        config: &AppConfig,
        index: Option<usize>,
        old_id: Option<&str>,
        prompt: PromptConfig,
    ) -> AppConfig {
        let mut next = config.clone();
        match index {
            Some(index) => {
                next.profiles[index] = prompt;
                if let Some(old_id) = old_id {
                    let new_id = next.profiles[index].id.clone();
                    if next.defaults.selection == old_id {
                        next.defaults.selection = new_id.clone();
                    }
                    if next.defaults.hover == old_id {
                        next.defaults.hover = new_id;
                    }
                }
            }
            None => next.profiles.push(prompt),
        }
        next
    }

    fn prompt_from_form(state: &ManagerState) -> Result<PromptConfig, String> {
        let temperature =
            parse_optional_f32(&read_text(state.handles.temperature), state.language())?;
        let max_output_tokens =
            parse_optional_u32(&read_text(state.handles.max_tokens), state.language())?;
        let prompt = PromptConfig {
            id: read_text(state.handles.profile_id).trim().to_owned(),
            name: read_text(state.handles.profile_name).trim().to_owned(),
            system_prompt: read_text(state.handles.system_prompt),
            user_template: read_text(state.handles.user_template),
            model: nonempty(read_text(state.handles.profile_model)),
            temperature,
            max_output_tokens,
        };
        prompt.validate().map_err(|error| {
            status_text(
                state.language(),
                StatusEvent::PromptInvalid {
                    detail: &error.to_string(),
                },
            )
        })?;
        Ok(prompt)
    }

    fn parse_optional_f32(value: &str, language: UiLanguage) -> Result<Option<f32>, String> {
        let value = value.trim();
        if value.is_empty() {
            return Ok(None);
        }
        let parsed = value
            .parse::<f32>()
            .map_err(|_| status_text(language, StatusEvent::InvalidTemperature))?;
        if !parsed.is_finite() || !(0.0..=2.0).contains(&parsed) {
            return Err(status_text(language, StatusEvent::InvalidTemperature));
        }
        Ok(Some(parsed))
    }

    fn parse_optional_u32(value: &str, language: UiLanguage) -> Result<Option<u32>, String> {
        let value = value.trim();
        if value.is_empty() {
            return Ok(None);
        }
        let parsed = value
            .parse::<u32>()
            .map_err(|_| status_text(language, StatusEvent::InvalidMaxOutputTokens))?;
        if parsed == 0 {
            return Err(status_text(language, StatusEvent::InvalidMaxOutputTokens));
        }
        Ok(Some(parsed))
    }

    fn nonempty(value: String) -> Option<String> {
        (!value.trim().is_empty()).then_some(value.trim().to_owned())
    }

    fn save_config(state: &ManagerState, config: &AppConfig) -> Result<(), String> {
        let path = state
            .config_path
            .as_ref()
            .ok_or_else(|| status_text(state.language(), StatusEvent::ConfigPathUnavailable))?;
        save_atomic(path, config).map_err(|error| error.to_string())
    }

    fn update_credential_status(state: &mut ManagerState) {
        let target = read_text(state.handles.credential_target).trim().to_owned();
        match credentials::read_api_key(&target) {
            Ok(Some(secret)) => {
                let _secret = Secret(secret);
                state.credential_status = CredentialStatusState::Present;
            }
            Ok(None) => state.credential_status = CredentialStatusState::Absent,
            Err(error) => {
                state.credential_status = CredentialStatusState::Unavailable(error.to_string())
            }
        }
        relabel_credential_status(state);
    }

    fn relabel_credential_status(state: &ManagerState) {
        let event = credential_status_event(&state.credential_status);
        set_text(
            state.handles.credential_status,
            &status_text(state.language(), event),
        );
    }

    fn credential_status_event(status: &CredentialStatusState) -> StatusEvent<'_> {
        match status {
            CredentialStatusState::Present => StatusEvent::CredentialStatusPresent,
            CredentialStatusState::Absent => StatusEvent::CredentialStatusAbsent,
            CredentialStatusState::Unavailable(detail) => {
                StatusEvent::CredentialStatusUnavailable { detail }
            }
        }
    }

    fn set_status(state: &ManagerState, message: &str) {
        set_text(state.handles.status, message);
        unsafe {
            let _ = InvalidateRect(Some(state.handles.status), None, true);
        }
    }

    fn read_secret(hwnd: HWND) -> Secret {
        let length = unsafe { GetWindowTextLengthW(hwnd) } as usize;
        let mut value = vec![0u16; length.saturating_add(1)];
        let written = unsafe { GetWindowTextW(hwnd, &mut value) } as usize;
        let secret = String::from_utf16_lossy(&value[..written.min(value.len())]);
        value.fill(0);
        Secret(secret)
    }

    fn set_text(hwnd: HWND, value: &str) {
        let value = wide(value);
        unsafe {
            SetWindowTextW(hwnd, PCWSTR(value.as_ptr())).ok();
        }
    }

    fn set_edit_cue(hwnd: HWND, value: &str) {
        let value = wide(value);
        unsafe {
            let _ = SendMessageW(
                hwnd,
                EM_SETCUEBANNER,
                Some(WPARAM(1)),
                Some(LPARAM(value.as_ptr() as isize)),
            );
        }
    }

    fn read_text(hwnd: HWND) -> String {
        let length = unsafe { GetWindowTextLengthW(hwnd) } as usize;
        let mut value = vec![0u16; length.saturating_add(1)];
        let written = unsafe { GetWindowTextW(hwnd, &mut value) } as usize;
        String::from_utf16_lossy(&value[..written.min(value.len())])
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }

    #[cfg(test)]
    mod tests {
        use super::{
            apply_prompt, config_refresh_status, config_with_manager_language,
            credential_refresh_status, format_history_row, history_count_text,
            history_source_for_index, layout_history, layout_prompts, layout_settings,
            parse_optional_f32, parse_optional_u32, profile_id_for_index, profile_option_label,
            resident_start_status, status_text, ui_text, valid_history_index, visibility_style,
            ManagerLayout, Slot, StatusEvent, StatusOperation, View, ALL_TEXT_KEYS,
        };
        use selection_core::{AppConfig, ExtractionSource, PromptConfig, UiLanguage};
        use selection_platform_windows::app::{RefreshOutcome, ResidentStartOutcome};
        use selection_storage::HistoryEntry;
        use windows::Win32::UI::WindowsAndMessaging::WS_VISIBLE;

        #[test]
        fn optional_numbers_are_strict_and_bounded() {
            assert_eq!(parse_optional_f32("", UiLanguage::English).unwrap(), None);
            assert_eq!(
                parse_optional_f32(" 0.5 ", UiLanguage::English).unwrap(),
                Some(0.5)
            );
            assert!(parse_optional_f32("2.1", UiLanguage::English).is_err());
            assert_eq!(
                parse_optional_u32("512", UiLanguage::English).unwrap(),
                Some(512)
            );
            assert!(parse_optional_u32("0", UiLanguage::English).is_err());
            assert!(parse_optional_f32("bad", UiLanguage::SimplifiedChinese)
                .unwrap_err()
                .contains("温度"));
        }

        #[test]
        fn page_children_are_visible_inside_hidden_containers() {
            assert_eq!(visibility_style(None), WS_VISIBLE);
            assert_eq!(visibility_style(Some(View::Settings)), WS_VISIBLE);
            assert_eq!(visibility_style(Some(View::Prompts)), WS_VISIBLE);
            assert_eq!(visibility_style(Some(View::History)), WS_VISIBLE);
        }

        #[test]
        fn page_visibility_is_exclusive() {
            for selected in [View::Settings, View::Prompts, View::History] {
                for page in [View::Settings, View::Prompts, View::History] {
                    assert_eq!(super::page_is_visible(page, selected), page == selected);
                }
            }
        }

        #[test]
        fn manager_layout_has_horizontal_tabs_and_content_below() {
            let layout = ManagerLayout::for_client(980, 756);
            assert_eq!(layout.title_bar.bottom, 36);
            assert_eq!(layout.tab_bar.top, 36);
            assert_eq!(layout.tab_bar.bottom, 81);
            assert_eq!(layout.page.top, 81);
            assert_eq!(layout.page.bottom, 756);
            for pair in layout.tab_buttons.windows(2) {
                assert!(pair[0].right <= pair[1].left);
            }
            assert!(layout.tab_buttons[2].right < layout.status_text.left);
            assert!(layout.status_text.right <= 980);
            assert!(layout.status_dot.left >= layout.status_text.right - 40);
        }

        #[test]
        fn settings_layout_stacks_field_groups() {
            let layout = layout_settings(940, 675);
            let provider = layout.groups[0];
            let credentials = layout.groups[1];
            let defaults = layout.groups[2];
            assert!(provider.bottom < credentials.top);
            assert!(credentials.bottom < defaults.top);
            let save = layout.rect_for(Slot::SettingsSave).unwrap();
            assert!(save.top >= defaults.bottom);
            assert!(layout.rect_for(Slot::SettingsEndpoint).is_some());
            assert!(layout.rect_for(Slot::SettingsCredentialHint).is_some());
        }

        #[test]
        fn prompts_editors_fill_remaining_height() {
            let layout = layout_prompts(940, 675);
            let system = layout.rect_for(Slot::PromptsSystemWell).unwrap();
            let user = layout.rect_for(Slot::PromptsUserWell).unwrap();
            let save = layout.rect_for(Slot::PromptsSave).unwrap();
            assert!(save.bottom < system.top);
            assert_eq!(system.left, layout.groups[0].left + 10);
            assert!(user.left > system.right);
            assert!(system.bottom > 400, "system well must flex-fill");
            assert!(user.bottom > 400, "user well must flex-fill");
            // No pager / defaults slots on the prompts page.
            assert!(layout.rect_for(Slot::SettingsSelectionDefault).is_none());
        }

        #[test]
        fn history_layout_splits_list_and_flex_output() {
            let layout = layout_history(940, 675);
            let list = layout.rect_for(Slot::HistoryList).unwrap();
            let output = layout.rect_for(Slot::HistoryOutput).unwrap();
            let target = layout.rect_for(Slot::HistoryTarget).unwrap();
            assert_eq!(list.right - list.left, 280);
            assert!(list.right < output.left);
            assert!(output.bottom > list.bottom - 80);
            assert!(target.bottom - target.top <= 48);
            assert!(layout.rect_for(Slot::HistoryDelete).is_some());
        }

        #[test]
        fn both_localization_catalogs_cover_every_static_key() {
            for &key in ALL_TEXT_KEYS {
                assert!(!ui_text(UiLanguage::English, key).trim().is_empty());
                assert!(!ui_text(UiLanguage::SimplifiedChinese, key)
                    .trim()
                    .is_empty());
            }
        }

        #[test]
        fn every_status_event_has_complete_english_and_chinese_text() {
            let events = vec![
                StatusEvent::ManagerInitializationFailed {
                    detail: "opaque-error",
                },
                StatusEvent::ConfigLoadFailed {
                    detail: "opaque-error",
                },
                StatusEvent::LocalAppDataUnavailable {
                    operation: StatusOperation::Save,
                },
                StatusEvent::LocalAppDataUnavailable {
                    operation: StatusOperation::History,
                },
                StatusEvent::SaveInterfaceLanguageFailed {
                    detail: "opaque-error",
                },
                StatusEvent::InterfaceLanguageSaved,
                StatusEvent::HistoryRefreshed,
                StatusEvent::HistoryUnavailable {
                    detail: "opaque-error",
                },
                StatusEvent::SelectHistoryEntry,
                StatusEvent::HistoryEntryUnavailable,
                StatusEvent::OutputCopied,
                StatusEvent::CopyOutputFailed {
                    detail: "opaque-error",
                },
                StatusEvent::DeleteHistoryConfirm {
                    target: "user text",
                },
                StatusEvent::DeletionCancelled,
                StatusEvent::HistoryEntryDeleted,
                StatusEvent::HistoryEntryAlreadyDeleted,
                StatusEvent::DeleteHistoryFailed {
                    detail: "opaque-error",
                },
                StatusEvent::OutputTooLarge,
                StatusEvent::ClipboardMemoryLockFailed,
                StatusEvent::ResidentStart(ResidentStartOutcome::AlreadyRunning),
                StatusEvent::ResidentStart(ResidentStartOutcome::Started),
                StatusEvent::ResidentStart(ResidentStartOutcome::Unavailable),
                StatusEvent::ConfigRefresh(RefreshOutcome::Acknowledged),
                StatusEvent::ConfigRefresh(RefreshOutcome::ResidentAbsent),
                StatusEvent::ConfigRefresh(RefreshOutcome::Unacknowledged),
                StatusEvent::ConfigRefresh(RefreshOutcome::Rejected),
                StatusEvent::CredentialRefresh {
                    outcome: RefreshOutcome::Acknowledged,
                    deleted: false,
                },
                StatusEvent::CredentialRefresh {
                    outcome: RefreshOutcome::Acknowledged,
                    deleted: true,
                },
                StatusEvent::CredentialRefresh {
                    outcome: RefreshOutcome::ResidentAbsent,
                    deleted: false,
                },
                StatusEvent::CredentialRefresh {
                    outcome: RefreshOutcome::ResidentAbsent,
                    deleted: true,
                },
                StatusEvent::CredentialRefresh {
                    outcome: RefreshOutcome::Unacknowledged,
                    deleted: false,
                },
                StatusEvent::CredentialRefresh {
                    outcome: RefreshOutcome::Rejected,
                    deleted: true,
                },
                StatusEvent::CannotSaveSettings {
                    detail: "opaque-error",
                },
                StatusEvent::EnterApiKey,
                StatusEvent::ApiKeySavedToCredentialManager,
                StatusEvent::ApiKeyInactiveTargetSaved,
                StatusEvent::SaveApiKeyFailed {
                    detail: "opaque-error",
                },
                StatusEvent::NoSavedApiKey,
                StatusEvent::ApiKeyInactiveTargetDeleted,
                StatusEvent::DeleteApiKeyFailed {
                    detail: "opaque-error",
                },
                StatusEvent::NoPromptProfile,
                StatusEvent::CannotSavePrompt {
                    detail: "opaque-error",
                },
                StatusEvent::PromptInvalid {
                    detail: "opaque-error",
                },
                StatusEvent::InvalidTemperature,
                StatusEvent::InvalidMaxOutputTokens,
                StatusEvent::ConfigPathUnavailable,
                StatusEvent::CredentialStatusPresent,
                StatusEvent::CredentialStatusAbsent,
                StatusEvent::CredentialStatusUnavailable {
                    detail: "opaque-error",
                },
                StatusEvent::HistoryCount { count: 2 },
                StatusEvent::NoContext,
            ];
            assert!(events.len() >= 48);
            for event in events {
                let english = status_text(UiLanguage::English, event);
                let chinese = status_text(UiLanguage::SimplifiedChinese, event);
                assert!(!english.trim().is_empty());
                assert!(!chinese.trim().is_empty());
                if english.contains("opaque-error") {
                    assert!(chinese.contains("opaque-error"));
                }
            }
        }

        #[test]
        fn credential_language_relabel_is_pure_and_preserves_opaque_detail() {
            let status = super::CredentialStatusState::Unavailable("vault-error".to_owned());
            let event = super::credential_status_event(&status);
            assert_eq!(
                status,
                super::CredentialStatusState::Unavailable("vault-error".to_owned())
            );
            assert!(status_text(UiLanguage::English, event).contains("vault-error"));
            assert!(status_text(UiLanguage::SimplifiedChinese, event).contains("vault-error"));
        }

        #[test]
        fn language_only_update_preserves_every_other_config_field() {
            let config = AppConfig::default();
            let mut expected = config.clone();
            expected.ui.manager_language = UiLanguage::SimplifiedChinese;
            let changed = config_with_manager_language(&config, UiLanguage::SimplifiedChinese);
            assert_eq!(changed, expected);
            assert_eq!(config.ui.manager_language, UiLanguage::English);
        }

        #[test]
        fn default_profile_selectors_show_names_but_save_stable_ids() {
            let mut first = PromptConfig::new("translate");
            first.name = "  Translate  ".to_owned();
            let mut second = PromptConfig::new("explain");
            second.name = "解释".to_owned();
            let profiles = vec![first.clone(), second.clone()];

            assert_eq!(profile_option_label(&first), "Translate — translate");
            assert_eq!(profile_option_label(&second), "解释 — explain");
            assert_eq!(
                profile_id_for_index(Some(0), &profiles),
                Some("translate".to_owned())
            );
            assert_eq!(
                profile_id_for_index(Some(1), &profiles),
                Some("explain".to_owned())
            );
            assert_eq!(profile_id_for_index(Some(2), &profiles), None);
            assert_eq!(profile_id_for_index(None, &profiles), None);
        }

        #[test]
        fn localized_counts_and_statuses_have_chinese_variants() {
            assert_eq!(history_count_text(UiLanguage::English, 1), "1 entry");
            assert_eq!(history_count_text(UiLanguage::English, 2), "2 entries");
            assert_eq!(
                history_count_text(UiLanguage::SimplifiedChinese, 2),
                "2 条记录"
            );
            assert!(resident_start_status(
                UiLanguage::SimplifiedChinese,
                ResidentStartOutcome::Started
            )
            .contains("驻留程序"));
            assert!(config_refresh_status(
                UiLanguage::SimplifiedChinese,
                RefreshOutcome::Acknowledged
            )
            .contains("已保存"));
        }

        #[test]
        fn draft_commit_does_not_mutate_saved_config_until_commit() {
            let config = AppConfig::default();
            let original_count = config.profiles.len();
            let draft = PromptConfig::new("draft");
            let committed = apply_prompt(&config, None, None, draft);
            assert_eq!(config.profiles.len(), original_count);
            assert_eq!(committed.profiles.len(), original_count + 1);
            assert!(config.profile("draft").is_none());
            assert!(committed.profile("draft").is_some());
        }

        #[test]
        fn history_source_filter_maps_all_and_each_source() {
            assert_eq!(history_source_for_index(None), None);
            assert_eq!(history_source_for_index(Some(0)), None);
            assert_eq!(
                history_source_for_index(Some(1)),
                Some(ExtractionSource::UiaSelection)
            );
            assert_eq!(
                history_source_for_index(Some(2)),
                Some(ExtractionSource::UiaPoint)
            );
            assert_eq!(
                history_source_for_index(Some(3)),
                Some(ExtractionSource::Clipboard)
            );
            assert_eq!(
                history_source_for_index(Some(4)),
                Some(ExtractionSource::Ocr)
            );
            assert_eq!(history_source_for_index(Some(5)), None);
        }

        #[test]
        fn history_row_is_single_line_and_keeps_target_and_output() {
            let entry = HistoryEntry {
                id: 7,
                created_at_utc: "2026-08-19T12:00:00Z".to_owned(),
                source: ExtractionSource::UiaSelection,
                target: "hello\nworld".to_owned(),
                context: None,
                output: "你好\t世界".to_owned(),
                prompt_id: "translate".to_owned(),
                model: "test".to_owned(),
                served_from_cache: false,
            };
            assert_eq!(format_history_row(&entry), "hello world  |  你好 世界");
        }

        #[test]
        fn history_selection_rejects_negative_and_out_of_range_values() {
            assert_eq!(valid_history_index(-1, 2), None);
            assert_eq!(valid_history_index(0, 2), Some(0));
            assert_eq!(valid_history_index(1, 2), Some(1));
            assert_eq!(valid_history_index(2, 2), None);
        }

        #[test]
        fn resident_and_refresh_statuses_never_claim_unacknowledged_success() {
            assert!(
                resident_start_status(UiLanguage::English, ResidentStartOutcome::Started)
                    .contains("ready")
            );
            assert!(
                resident_start_status(UiLanguage::English, ResidentStartOutcome::Unavailable)
                    .contains("translation is unavailable")
            );
            assert!(
                config_refresh_status(UiLanguage::English, RefreshOutcome::Acknowledged)
                    .contains("confirmed")
            );
            assert!(
                !config_refresh_status(UiLanguage::English, RefreshOutcome::ResidentAbsent)
                    .contains("confirmed")
            );
            assert!(
                !config_refresh_status(UiLanguage::English, RefreshOutcome::Unacknowledged)
                    .contains("confirmed")
            );
            assert!(
                !config_refresh_status(UiLanguage::English, RefreshOutcome::Rejected)
                    .contains("confirmed")
            );
            assert!(credential_refresh_status(
                UiLanguage::English,
                RefreshOutcome::Acknowledged,
                false
            )
            .contains("confirmed"));
            assert!(!credential_refresh_status(
                UiLanguage::English,
                RefreshOutcome::ResidentAbsent,
                true
            )
            .contains("confirmed"));
        }

        #[test]
        fn develop_guide_copy_strings_match_english_catalog() {
            assert_eq!(
                ui_text(UiLanguage::English, super::TextKey::WindowTitle),
                "Selection Translate — Manager"
            );
            assert_eq!(
                ui_text(UiLanguage::English, super::TextKey::Endpoint),
                "Endpoint"
            );
            assert_eq!(
                ui_text(UiLanguage::English, super::TextKey::SelectionProfile),
                "Selection profile"
            );
            assert_eq!(
                ui_text(UiLanguage::English, super::TextKey::MaxTokens),
                "Max tokens"
            );
            assert_eq!(
                ui_text(UiLanguage::English, super::TextKey::SystemPrompt),
                "System prompt"
            );
            assert_eq!(
                ui_text(UiLanguage::English, super::TextKey::UserTemplate),
                "User template"
            );
            assert_eq!(
                ui_text(UiLanguage::English, super::TextKey::Selection),
                "Selection"
            );
            assert_eq!(
                ui_text(UiLanguage::English, super::TextKey::GroupProvider),
                "Provider"
            );
            assert_eq!(
                ui_text(UiLanguage::English, super::TextKey::KeyPresentValueHidden),
                "Key present · value hidden"
            );
            // Captions render uppercase at paint time (DEVELOP_GUIDE field captions).
            assert_eq!(
                ui_text(UiLanguage::English, super::TextKey::SystemPrompt).to_uppercase(),
                "SYSTEM PROMPT"
            );
            assert_eq!(
                ui_text(
                    UiLanguage::SimplifiedChinese,
                    super::TextKey::KeyPresentValueHidden
                ),
                "密钥已保存 · 内容已隐藏"
            );
        }
    }
}

#[cfg(windows)]
fn main() {
    if let Err(error) = windows_app::run() {
        eprintln!("selection translate manager failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {}
