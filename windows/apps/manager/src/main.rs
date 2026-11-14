#![cfg_attr(windows, windows_subsystem = "windows")]

//! The on-demand manager. It is a plain Win32 window: the resident process
//! never loads this executable, and no WebView/runtime is needed for editing
//! the small TOML configuration.

#[cfg(windows)]
mod windows_app {
    use lingosnap_core::{
        default_config_path, save_atomic, AppConfig, ExtractionSource, PromptConfig, UiLanguage,
        UiTheme,
    };
    use lingosnap_platform_windows::{
        app::{
            ensure_resident_running, notify_config_changed, notify_credentials_changed,
            RefreshOutcome, ResidentStartOutcome,
        },
        credentials, webview_ui,
    };
    use lingosnap_storage::{
        default_history_path, HistoryDatabase, HistoryEntry, HistoryOrder, HistoryQuery,
    };
    use std::ffi::c_void;
    use std::path::PathBuf;
    use windows::core::{w, Error, PCWSTR};
    use windows::Win32::Foundation::{
        COLORREF, HANDLE, HGLOBAL, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM,
    };
    use windows::Win32::Graphics::Gdi::{
        BeginPaint, CreateFontW, CreatePen, CreateRoundRectRgn, CreateSolidBrush, DeleteObject,
        DrawFocusRect, DrawTextW, Ellipse, EndPaint, FillRect, FillRgn, FrameRect, FrameRgn,
        GetTextExtentPoint32W, InvalidateRect, SelectObject, SetBkColor, SetBkMode, SetTextColor,
        UpdateWindow, BACKGROUND_MODE, DRAW_TEXT_FORMAT, FONT_CHARSET, FONT_CLIP_PRECISION,
        FONT_OUTPUT_PRECISION, FONT_QUALITY, HBRUSH, HDC, HFONT, HGDIOBJ, PS_NULL, TRANSPARENT,
    };
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::LibraryLoader::LoadLibraryW;
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::System::Ole::CF_UNICODETEXT;
    use windows::Win32::System::Threading::CreateMutexW;
    use windows::Win32::UI::Controls::RichEdit::{
        CFE_EFFECTS, CFM_COLOR, CHARFORMATW, EM_SETCHARFORMAT, SCF_ALL,
    };
    use windows::Win32::UI::Controls::{
        SetScrollInfo, SetWindowTheme, ShowScrollBar, DRAWITEMSTRUCT, MEASUREITEMSTRUCT,
        ODT_BUTTON, ODT_LISTBOX,
    };
    use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetDpiForWindow};
    use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, FindWindowW,
        GetClientRect, GetDlgCtrlID, GetMessageW, GetParent, GetWindowLongPtrW, GetWindowRect,
        GetWindowTextLengthW, GetWindowTextW, IsDialogMessageW, KillTimer, MessageBoxW,
        PostQuitMessage, RegisterClassW, SendMessageW, SetTimer, SetWindowLongPtrW, SetWindowPos,
        SetWindowTextW, ShowWindow, TranslateMessage, BS_OWNERDRAW, BS_PUSHBUTTON, CREATESTRUCTW,
        ES_AUTOHSCROLL, ES_AUTOVSCROLL, ES_MULTILINE, ES_PASSWORD, GWLP_USERDATA, IDYES,
        MB_DEFBUTTON2, MB_ICONWARNING, MB_YESNO, MINMAXINFO, SWP_NOACTIVATE, SWP_NOMOVE,
        SWP_NOZORDER, SW_HIDE, SW_SHOW, WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_DESTROY,
        WM_DPICHANGED, WM_DRAWITEM, WM_ERASEBKGND, WM_GETMINMAXINFO, WM_MEASUREITEM, WM_MOUSEWHEEL,
        WM_NOTIFY, WM_PAINT, WM_SETFONT, WM_SIZE, WM_TIMER, WM_VSCROLL, WNDCLASSW, WS_BORDER,
        WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_EX_CONTROLPARENT, WS_OVERLAPPED,
        WS_SYSMENU, WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        SB_LINEDOWN, SB_LINEUP, SB_PAGEDOWN, SB_PAGEUP, SB_THUMBPOSITION, SB_VERT, SCROLLINFO,
        SIF_ALL, SIF_POS, SIF_RANGE,
    };

    const CLASS_NAME: PCWSTR = w!("LingoSnapManager");
    const PAGE_CLASS: PCWSTR = w!("LingoSnapManagerPage");
    const ID_SETTINGS_TAB: usize = 100;
    const ID_PROMPTS_TAB: usize = 101;
    const ID_HISTORY_TAB: usize = 102;
    const ID_SAVE_SETTINGS: usize = 110;
    const ID_SAVE_KEY: usize = 111;
    const ID_DELETE_KEY: usize = 112;
    const ID_SAVE_PROMPT: usize = 120;
    const ID_NEW_PROMPT: usize = 121;
    const ID_DELETE_PROMPT: usize = 122;
    const ID_CLOSE: usize = 130;
    const ID_HISTORY_PROMPT: usize = 141;
    const ID_HISTORY_SOURCE: usize = 142;
    const ID_HISTORY_ORDER: usize = 143;
    const ID_HISTORY_LIST: usize = 144;
    const ID_HISTORY_REFRESH: usize = 145;
    const ID_HISTORY_COPY: usize = 146;
    const ID_HISTORY_DELETE: usize = 147;
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
    const ID_HOTKEY_SELECTION: usize = 190;
    const ID_HOTKEY_HOVER: usize = 191;
    const ID_HOTKEY_PAUSE: usize = 192;
    const ID_HOTKEY_OPEN_POPUP: usize = 193;
    // Profile reorder buttons: two per row (up/down), allocated in pairs.
    // Owned by dynamically created rows on the Settings page; the pair for
    // row `index` lives at ID_PROFILE_MOVE_BASE + index * 2 (+1 for down).
    const ID_PROFILE_MOVE_BASE: usize = 180;
    const ID_PROFILE_MOVE_SLOTS: usize = 64;
    // Mirrors the resident's chooser rail inline limit: the first N profiles
    // in this order sit on the chooser rail, the rest fold under More….
    const RAIL_INLINE_LIMIT: usize = 4;

    const DEFAULT_DPI: u32 = 96;
    // Redesign mockup metrics (logical px): a horizontal tab bar over
    // full-width pages whose content is capped at 720px and centered.
    const TAB_BAR_HEIGHT: i32 = 46;
    const PAGE_HEADER_HEIGHT: i32 = 64;
    const CONTENT_PADDING: i32 = 32;
    const CONTENT_MAX_WIDTH: i32 = 720;
    const MIN_CONTENT_WIDTH: i32 = CONTENT_MAX_WIDTH + CONTENT_PADDING * 2;
    const MIN_CLIENT_HEIGHT: i32 = 706;
    const TAB_BUTTON_WIDTH: i32 = 92;
    const TAB_BUTTON_HEIGHT: i32 = 32;
    // Tokens from the redesign mockup. COLORREF is 0x00BBGGRR.
    const MANAGER_BG: COLORREF = COLORREF(0x0013_0E0B); // --void #0B0E13
    const TAB_BG: COLORREF = COLORREF(0x001F_1712); // --surface #12171F
    const SURFACE_BG: COLORREF = COLORREF(0x0030_221A); // --raised #1A2230 (cards)
    const SURFACE_HOVER: COLORREF = COLORREF(0x003F_2D23); // raised hover #232D3F
    const TAB_HOT: COLORREF = COLORREF(0x003E_2C23); // accent 14% over surface #232C3E
    const BORDER: COLORREF = COLORREF(0x0046_362C); // --line #2C3646
    const TEXT: COLORREF = COLORREF(0x00FA_F4F0); // --ink #F0F4FA
    const MUTED: COLORREF = COLORREF(0x00A8_978B); // --muted #8B97A8
    const ACCENT: COLORREF = COLORREF(0x00FF_AC8B); // --accent #8BACFF
    #[allow(dead_code)]
    const GOLD: COLORREF = COLORREF(0x006A_B5D4); // --lex-gold #D4B56A
    const OK: COLORREF = COLORREF(0x00A5_D14F); // --ok #4FD1A5
    const DANGER: COLORREF = COLORREF(0x0078_71F0); // --err #F07178
    const DANGER_BG: COLORREF = COLORREF(0x0036_282B); // err 8% over raised
    const DANGER_BORDER: COLORREF = COLORREF(0x0049_3E65); // err 35% over raised
    const PRIMARY_INK: COLORREF = COLORREF(0x0013_0E0B); // --void text on accent

    const LB_ADDSTRING: u32 = 0x0180;
    const LB_RESETCONTENT: u32 = 0x0184;
    const LB_GETCURSEL: u32 = 0x0188;
    const LBN_SELCHANGE: usize = 1;
    const EM_SETBKGNDCOLOR: u32 = 0x0443;

    /// msftedit.dll hosts the RICHEDIT50W class used by the markdown-aware
    /// fields. Loaded once per process; when missing those fields fall back
    /// to plain EDIT controls and render as raw text.
    static RICH_EDIT_MODULE: std::sync::OnceLock<isize> = std::sync::OnceLock::new();

    /// RichEdit ignores WM_CTLCOLOREDIT and resets character formatting on
    /// WM_SETFONT/WM_SETTEXT, so the dark surface and light ink must be
    /// reapplied after creation, font changes, and every text fill.
    fn apply_rich_ink(control: HWND) {
        unsafe {
            let _ = SendMessageW(
                control,
                EM_SETBKGNDCOLOR,
                Some(WPARAM(0)),
                Some(LPARAM(MANAGER_BG.0 as isize)),
            );
            let ink = CHARFORMATW {
                cbSize: std::mem::size_of::<CHARFORMATW>() as u32,
                dwMask: CFM_COLOR,
                dwEffects: CFE_EFFECTS(0),
                crTextColor: TEXT,
                ..Default::default()
            };
            let _ = SendMessageW(
                control,
                EM_SETCHARFORMAT,
                Some(WPARAM(SCF_ALL as usize)),
                Some(LPARAM(&ink as *const CHARFORMATW as isize)),
            );
        }
    }

    fn rich_edit_class() -> windows::core::PCWSTR {
        use windows::core::w;
        if RICH_EDIT_MODULE.get().copied().unwrap_or(0) != 0 {
            w!("RICHEDIT50W")
        } else {
            w!("EDIT")
        }
    }
    const CBN_SELCHANGE: usize = 1;
    const CBN_DROPDOWN: usize = 7;
    const CBN_CLOSEUP: usize = 8;
    const CB_ADDSTRING: u32 = 0x0143;
    const CB_RESETCONTENT: u32 = 0x014B;
    const CB_SETCURSEL: u32 = 0x014E;
    const CB_GETCURSEL: u32 = 0x0147;
    const CB_GETCOUNT: u32 = 0x0146;
    const CB_GETITEMHEIGHT: u32 = 0x0154;
    const LBS_NOTIFY: u32 = 0x0001;
    const LBS_OWNERDRAWFIXED: u32 = 0x0010;
    const LBS_NOINTEGRALHEIGHT: u32 = 0x0100;
    const CBS_DROPDOWNLIST: u32 = 0x0003;
    const EM_SETREADONLY: u32 = 0x00CF;

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

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum TextKey {
        WindowTitle,
        Brand,
        Manager,
        Settings,
        Prompts,
        History,
        Close,
        SettingsSubtitle,
        PromptsSubtitle,
        HistorySubtitle,
        InterfaceLanguage,
        ProviderCard,
        CredentialsCard,
        DefaultsCard,
        HotkeyCard,
        HotkeySelectionLabel,
        HotkeyHoverLabel,
        HotkeyPauseLabel,
        HotkeyOpenPopupLabel,
        ProviderEndpoint,
        Model,
        CredentialTarget,
        ApiKey,
        SaveKey,
        DeleteSavedKey,
        SelectionDefaultProfile,
        HoverDefaultProfile,
        SaveSettings,
        CredentialPrivacy,
        New,
        Id,
        Name,
        SystemPrompt,
        UserTemplate,
        ModelOverride,
        Temperature,
        MaxOutputTokens,
        SavePrompt,
        DeleteProfile,
        DeletedProfile,
        PromptHint,
        HintModelOverride,
        HintTemperature,
        HintMaxTokens,
        SearchCard,
        EntriesCard,
        SearchTarget,
        Refresh,
        CopyOutput,
        Prompt,
        Source,
        Order,
        Target,
        Context,
        Output,
        DeleteSelected,
        HistoryPrivacy,
        AllPrompts,
        AllSources,
        Selection,
        Hover,
        Clipboard,
        Ocr,
        Newest,
        Oldest,
        ProfilesCard,
        ProfilesBeneathDivider,
        ProfilesRailHint,
        English,
        SimplifiedChinese,
    }

    const ALL_TEXT_KEYS: &[TextKey] = &[
        TextKey::WindowTitle,
        TextKey::Brand,
        TextKey::Manager,
        TextKey::Settings,
        TextKey::Prompts,
        TextKey::History,
        TextKey::Close,
        TextKey::SettingsSubtitle,
        TextKey::PromptsSubtitle,
        TextKey::HistorySubtitle,
        TextKey::InterfaceLanguage,
        TextKey::ProviderCard,
        TextKey::CredentialsCard,
        TextKey::DefaultsCard,
        TextKey::HotkeyCard,
        TextKey::HotkeySelectionLabel,
        TextKey::HotkeyHoverLabel,
        TextKey::HotkeyPauseLabel,
        TextKey::HotkeyOpenPopupLabel,
        TextKey::ProviderEndpoint,
        TextKey::Model,
        TextKey::CredentialTarget,
        TextKey::ApiKey,
        TextKey::SaveKey,
        TextKey::DeleteSavedKey,
        TextKey::SelectionDefaultProfile,
        TextKey::HoverDefaultProfile,
        TextKey::SaveSettings,
        TextKey::CredentialPrivacy,
        TextKey::New,
        TextKey::Id,
        TextKey::Name,
        TextKey::SystemPrompt,
        TextKey::UserTemplate,
        TextKey::ModelOverride,
        TextKey::Temperature,
        TextKey::MaxOutputTokens,
        TextKey::SavePrompt,
        TextKey::DeleteProfile,
        TextKey::DeletedProfile,
        TextKey::PromptHint,
        TextKey::HintModelOverride,
        TextKey::HintTemperature,
        TextKey::HintMaxTokens,
        TextKey::SearchCard,
        TextKey::EntriesCard,
        TextKey::SearchTarget,
        TextKey::Refresh,
        TextKey::CopyOutput,
        TextKey::Prompt,
        TextKey::Source,
        TextKey::Order,
        TextKey::Target,
        TextKey::Context,
        TextKey::Output,
        TextKey::DeleteSelected,
        TextKey::HistoryPrivacy,
        TextKey::AllPrompts,
        TextKey::AllSources,
        TextKey::Selection,
        TextKey::Hover,
        TextKey::Clipboard,
        TextKey::Ocr,
        TextKey::Newest,
        TextKey::Oldest,
        TextKey::ProfilesCard,
        TextKey::ProfilesBeneathDivider,
        TextKey::ProfilesRailHint,
        TextKey::English,
        TextKey::SimplifiedChinese,
    ];

    fn ui_text(language: UiLanguage, key: TextKey) -> &'static str {
        use TextKey::*;
        match (language, key) {
            (UiLanguage::English, WindowTitle) => "LingoSnap — Manager",
            (UiLanguage::English, Brand) => "LingoSnap",
            (UiLanguage::English, Manager) => "Manager",
            (UiLanguage::English, Settings) => "Settings",
            (UiLanguage::English, Prompts) => "Profiles",
            (UiLanguage::English, History) => "History",
            (UiLanguage::English, Close) => "Close",
            (UiLanguage::English, SettingsSubtitle) => "Provider, credentials and default profiles",
            (UiLanguage::English, PromptsSubtitle) => "Create and tune reusable profiles",
            (UiLanguage::English, HistorySubtitle) => {
                "Search recent translations without keeping the database open"
            }
            (UiLanguage::English, InterfaceLanguage) => "Interface language",
            (UiLanguage::English, ProviderCard) => "Provider",
            (UiLanguage::English, CredentialsCard) => "Credentials",
            (UiLanguage::English, DefaultsCard) => "Defaults",
            (UiLanguage::English, HotkeyCard) => "Hot key",
            (UiLanguage::English, HotkeySelectionLabel) => "Selection translate",
            (UiLanguage::English, HotkeyHoverLabel) => "Hover translate",
            (UiLanguage::English, HotkeyPauseLabel) => "Pause / resume",
            (UiLanguage::English, HotkeyOpenPopupLabel) => "Open popup",
            (UiLanguage::English, ProfilesCard) => "Profiles",
            (UiLanguage::English, ProfilesBeneathDivider) => "BENEATH MORE…",
            (UiLanguage::English, ProfilesRailHint) => {
                "The order here sets the chooser rail — the first four sit on the rail, the rest beneath More…"
            }
            (UiLanguage::English, ProviderEndpoint) => "Provider endpoint",
            (UiLanguage::English, Model) => "Model",
            (UiLanguage::English, CredentialTarget) => "Credential target",
            (UiLanguage::English, ApiKey) => "API key",
            (UiLanguage::English, SaveKey) => "Save key",
            (UiLanguage::English, DeleteSavedKey) => "Delete saved key",
            (UiLanguage::English, SelectionDefaultProfile) => "Selection profile",
            (UiLanguage::English, HoverDefaultProfile) => "Hover profile",
            (UiLanguage::English, SaveSettings) => "Save settings",
            (UiLanguage::English, CredentialPrivacy) => {
                "Keys are held by Windows Credential Manager; they never enter config.toml."
            }
            (UiLanguage::English, New) => "New",
            (UiLanguage::English, Id) => "Profile",
            (UiLanguage::English, Name) => "Name",
            (UiLanguage::English, SystemPrompt) => "System prompt",
            (UiLanguage::English, UserTemplate) => "User template",
            (UiLanguage::English, ModelOverride) => "Model override",
            (UiLanguage::English, Temperature) => "Temperature",
            (UiLanguage::English, MaxOutputTokens) => "Max tokens",
            (UiLanguage::English, SavePrompt) => "Save profile",
            (UiLanguage::English, DeleteProfile) => "Delete profile",
            (UiLanguage::English, DeletedProfile) => "Deleted profile",
            (UiLanguage::English, PromptHint) => {
                "Use {target}, {context}, and {source}; every user template needs {target}."
            }
            (UiLanguage::English, HintModelOverride) => {
                "Model override: leave empty to use the model set in Settings."
            }
            (UiLanguage::English, HintTemperature) => {
                "Temperature: 0–2. Lower gives steadier answers, higher gives more varied ones. Empty uses the provider default."
            }
            (UiLanguage::English, HintMaxTokens) => {
                "Max tokens: the longest reply allowed, in tokens. Empty uses the provider default."
            }
            (UiLanguage::English, SearchCard) => "Search",
            (UiLanguage::English, EntriesCard) => "Entries",
            (UiLanguage::English, SearchTarget) => "Search targets",
            (UiLanguage::English, Refresh) => "Refresh",
            (UiLanguage::English, CopyOutput) => "Copy output",
            (UiLanguage::English, Prompt) => "Profile",
            (UiLanguage::English, Source) => "Source",
            (UiLanguage::English, Order) => "Order",
            (UiLanguage::English, Target) => "Target",
            (UiLanguage::English, Context) => "Context",
            (UiLanguage::English, Output) => "Output",
            (UiLanguage::English, DeleteSelected) => "Delete selected",
            (UiLanguage::English, HistoryPrivacy) => {
                "History is loaded only while this tab is open; the database is never held open."
            }
            (UiLanguage::English, AllPrompts) => "All profiles",
            (UiLanguage::English, AllSources) => "All sources",
            (UiLanguage::English, Selection) => "Selection",
            (UiLanguage::English, Hover) => "Hover",
            (UiLanguage::English, Clipboard) => "Clipboard",
            (UiLanguage::English, Ocr) => "OCR",
            (UiLanguage::English, Newest) => "Newest",
            (UiLanguage::English, Oldest) => "Oldest",
            (UiLanguage::English, English) => "English",
            (UiLanguage::English, SimplifiedChinese) => "Simplified Chinese",
            (UiLanguage::SimplifiedChinese, WindowTitle) => "LingoSnap — 管理器",
            (UiLanguage::SimplifiedChinese, Brand) => "LingoSnap",
            (UiLanguage::SimplifiedChinese, Manager) => "管理器",
            (UiLanguage::SimplifiedChinese, Settings) => "设置",
            (UiLanguage::SimplifiedChinese, Prompts) => "配置档",
            (UiLanguage::SimplifiedChinese, History) => "历史记录",
            (UiLanguage::SimplifiedChinese, Close) => "关闭",
            (UiLanguage::SimplifiedChinese, SettingsSubtitle) => "服务商、凭据和默认配置",
            (UiLanguage::SimplifiedChinese, PromptsSubtitle) => "创建和调整可复用的配置档",
            (UiLanguage::SimplifiedChinese, HistorySubtitle) => "搜索最近结果；数据库仅按需打开",
            (UiLanguage::SimplifiedChinese, InterfaceLanguage) => "界面语言",
            (UiLanguage::SimplifiedChinese, ProviderCard) => "服务商",
            (UiLanguage::SimplifiedChinese, CredentialsCard) => "凭据",
            (UiLanguage::SimplifiedChinese, DefaultsCard) => "默认配置档",
            (UiLanguage::SimplifiedChinese, HotkeyCard) => "热键",
            (UiLanguage::SimplifiedChinese, HotkeySelectionLabel) => "划词翻译",
            (UiLanguage::SimplifiedChinese, HotkeyHoverLabel) => "悬停翻译",
            (UiLanguage::SimplifiedChinese, HotkeyPauseLabel) => "暂停/恢复",
            (UiLanguage::SimplifiedChinese, HotkeyOpenPopupLabel) => "打开弹窗",
            (UiLanguage::SimplifiedChinese, ProfilesCard) => "配置档",
            (UiLanguage::SimplifiedChinese, ProfilesBeneathDivider) => "MORE… 以下",
            (UiLanguage::SimplifiedChinese, ProfilesRailHint) => {
                "这里的顺序即翻译选择栏的顺序 — 前四个显示在栏上，其余收在 More… 下。"
            }
            (UiLanguage::SimplifiedChinese, ProviderEndpoint) => "服务端点",
            (UiLanguage::SimplifiedChinese, Model) => "模型",
            (UiLanguage::SimplifiedChinese, CredentialTarget) => "凭据目标",
            (UiLanguage::SimplifiedChinese, ApiKey) => "API 密钥",
            (UiLanguage::SimplifiedChinese, SaveKey) => "保存密钥",
            (UiLanguage::SimplifiedChinese, DeleteSavedKey) => "删除已存密钥",
            (UiLanguage::SimplifiedChinese, SelectionDefaultProfile) => "划词配置档",
            (UiLanguage::SimplifiedChinese, HoverDefaultProfile) => "悬停配置档",
            (UiLanguage::SimplifiedChinese, SaveSettings) => "保存设置",
            (UiLanguage::SimplifiedChinese, CredentialPrivacy) => {
                "密钥保存在 Windows 凭据管理器中，绝不会写入 config.toml。"
            }
            (UiLanguage::SimplifiedChinese, New) => "新建",
            (UiLanguage::SimplifiedChinese, Id) => "ID",
            (UiLanguage::SimplifiedChinese, Name) => "名称",
            (UiLanguage::SimplifiedChinese, SystemPrompt) => "系统提示词",
            (UiLanguage::SimplifiedChinese, UserTemplate) => "用户模板",
            (UiLanguage::SimplifiedChinese, ModelOverride) => "模型覆盖",
            (UiLanguage::SimplifiedChinese, Temperature) => "温度",
            (UiLanguage::SimplifiedChinese, MaxOutputTokens) => "最大令牌数",
            (UiLanguage::SimplifiedChinese, SavePrompt) => "保存配置档",
            (UiLanguage::SimplifiedChinese, DeleteProfile) => "删除配置档",
            (UiLanguage::SimplifiedChinese, DeletedProfile) => "已删除的配置档",
            (UiLanguage::SimplifiedChinese, PromptHint) => {
                "可使用 {target}、{context} 和 {source}；用户模板必须包含 {target}。"
            }
            (UiLanguage::SimplifiedChinese, HintModelOverride) => {
                "模型覆盖：留空则使用设置中的模型。"
            }
            (UiLanguage::SimplifiedChinese, HintTemperature) => {
                "温度：0–2。越低越稳定，越高越多样。留空使用服务商默认值。"
            }
            (UiLanguage::SimplifiedChinese, HintMaxTokens) => {
                "最大令牌数：回复的最大长度（令牌数）。留空使用服务商默认值。"
            }
            (UiLanguage::SimplifiedChinese, SearchCard) => "搜索",
            (UiLanguage::SimplifiedChinese, EntriesCard) => "记录",
            (UiLanguage::SimplifiedChinese, SearchTarget) => "搜索目标文本",
            (UiLanguage::SimplifiedChinese, Refresh) => "刷新",
            (UiLanguage::SimplifiedChinese, CopyOutput) => "复制输出",
            (UiLanguage::SimplifiedChinese, Prompt) => "配置档",
            (UiLanguage::SimplifiedChinese, Source) => "来源",
            (UiLanguage::SimplifiedChinese, Order) => "排序",
            (UiLanguage::SimplifiedChinese, Target) => "目标",
            (UiLanguage::SimplifiedChinese, Context) => "上下文",
            (UiLanguage::SimplifiedChinese, Output) => "输出",
            (UiLanguage::SimplifiedChinese, DeleteSelected) => "删除所选项",
            (UiLanguage::SimplifiedChinese, HistoryPrivacy) => {
                "仅在此页面打开历史数据库，离开后立即关闭。"
            }
            (UiLanguage::SimplifiedChinese, AllPrompts) => "全部配置档",
            (UiLanguage::SimplifiedChinese, AllSources) => "全部来源",
            (UiLanguage::SimplifiedChinese, Selection) => "划词",
            (UiLanguage::SimplifiedChinese, Hover) => "悬停",
            (UiLanguage::SimplifiedChinese, Clipboard) => "剪贴板",
            (UiLanguage::SimplifiedChinese, Ocr) => "OCR",
            (UiLanguage::SimplifiedChinese, Newest) => "最新优先",
            (UiLanguage::SimplifiedChinese, Oldest) => "最早优先",
            (UiLanguage::SimplifiedChinese, English) => "English",
            (UiLanguage::SimplifiedChinese, SimplifiedChinese) => "简体中文",
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
        SaveThemeFailed {
            detail: &'a str,
        },
        DeleteProfileConfirm {
            name: &'a str,
        },
        ProfileDeleted {
            name: &'a str,
        },
        DeleteBlockedLast,
        DeleteBlockedSelectionDefault,
        DeleteBlockedHoverDefault,
        DeleteProfileFailed {
            detail: &'a str,
        },
        DuplicateProfileName {
            name: &'a str,
        },
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
        NewPromptUnsaved,
        UnsavedPromptDiscarded,
        PromptInvalid {
            detail: &'a str,
        },
        InvalidTemperature,
        InvalidMaxOutputTokens,
        ConfigPathUnavailable,
        ConfigExistsNotOverwritten,
        CredentialStatusPresent,
        CredentialStatusAbsent,
        CredentialStatusUnavailable {
            detail: &'a str,
        },
        HistoryCount {
            count: usize,
        },
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
            SaveThemeFailed { detail } => match language {
                UiLanguage::English => format!("Could not save theme: {detail}"),
                UiLanguage::SimplifiedChinese => format!("无法保存主题：{detail}"),
            },
            DeleteProfileConfirm { name } => match language {
                UiLanguage::English => format!("Delete profile “{name}”? This cannot be undone. Existing history entries keep their results."),
                UiLanguage::SimplifiedChinese => format!("删除配置档“{name}”？此操作无法撤销，已有历史记录会保留。"),
            },
            ProfileDeleted { name } => match language {
                UiLanguage::English => format!("Profile “{name}” deleted."),
                UiLanguage::SimplifiedChinese => format!("已删除配置档“{name}”。"),
            },
            DeleteBlockedLast => match language {
                UiLanguage::English => "The last profile cannot be deleted.".to_owned(),
                UiLanguage::SimplifiedChinese => "无法删除最后一个配置档。".to_owned(),
            },
            DeleteBlockedSelectionDefault => match language {
                UiLanguage::English => "This is the Selection profile. Choose a different Selection profile in Settings first.".to_owned(),
                UiLanguage::SimplifiedChinese => "这是划词配置档。请先在设置中选择其他划词配置档。".to_owned(),
            },
            DeleteBlockedHoverDefault => match language {
                UiLanguage::English => "This is the Hover profile. Choose a different Hover profile in Settings first.".to_owned(),
                UiLanguage::SimplifiedChinese => "这是悬停配置档。请先在设置中选择其他悬停配置档。".to_owned(),
            },
            DeleteProfileFailed { detail } => match language {
                UiLanguage::English => format!("Could not delete profile: {detail}"),
                UiLanguage::SimplifiedChinese => format!("无法删除配置档：{detail}"),
            },
            DuplicateProfileName { name } => match language {
                UiLanguage::English => format!("Another profile is already named “{name}”. Choose a different name."),
                UiLanguage::SimplifiedChinese => format!("已有名为“{name}”的配置档，请换一个名称。"),
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
            NoPromptProfile => match language { UiLanguage::English => "There is no profile to save.".to_owned(), UiLanguage::SimplifiedChinese => "没有可保存的配置档。".to_owned() },
            CannotSavePrompt { detail } => match language { UiLanguage::English => format!("Cannot save profile: {detail}"), UiLanguage::SimplifiedChinese => format!("无法保存配置档：{detail}") },
            NewPromptUnsaved => match language { UiLanguage::English => "New profile is unsaved. Edit it, then choose Save profile.".to_owned(), UiLanguage::SimplifiedChinese => "新配置档尚未保存。编辑后请选择“保存配置档”。".to_owned() },
            UnsavedPromptDiscarded => match language { UiLanguage::English => "Unsaved new profile discarded; the saved settings were unchanged.".to_owned(), UiLanguage::SimplifiedChinese => "已丢弃未保存的新配置档；已保存的设置未更改。".to_owned() },
            PromptInvalid { detail } => match language { UiLanguage::English => format!("Profile is invalid: {detail}"), UiLanguage::SimplifiedChinese => format!("配置档无效：{detail}") },
            InvalidTemperature => match language { UiLanguage::English => "Temperature must be a number from 0 to 2.".to_owned(), UiLanguage::SimplifiedChinese => "温度必须是 0 到 2 之间的数字。".to_owned() },
            InvalidMaxOutputTokens => match language { UiLanguage::English => "Max tokens must be a positive integer.".to_owned(), UiLanguage::SimplifiedChinese => "最大令牌数必须是正整数。".to_owned() },
            ConfigPathUnavailable => match language { UiLanguage::English => "LOCALAPPDATA is not available".to_owned(), UiLanguage::SimplifiedChinese => "LOCALAPPDATA 不可用".to_owned() },
            ConfigExistsNotOverwritten => match language { UiLanguage::English => "A configuration file already exists at the expected path; it was not overwritten. Restart the Manager to load it.".to_owned(), UiLanguage::SimplifiedChinese => "预期路径已存在配置文件，未覆盖。请重新打开管理器以加载该文件。".to_owned() },
            CredentialStatusPresent => match language { UiLanguage::English => "A saved key is present (value hidden).".to_owned(), UiLanguage::SimplifiedChinese => "存在已保存的密钥（值已隐藏）。".to_owned() },
            CredentialStatusAbsent => match language { UiLanguage::English => "No saved key for this target.".to_owned(), UiLanguage::SimplifiedChinese => "此目标没有已保存的密钥。".to_owned() },
            CredentialStatusUnavailable { detail } => match language { UiLanguage::English => format!("Credential status unavailable: {detail}"), UiLanguage::SimplifiedChinese => format!("凭据状态不可用：{detail}") },
            HistoryCount { count } => match language { UiLanguage::English if count == 1 => "1 entry".to_owned(), UiLanguage::English => format!("{count} entries"), UiLanguage::SimplifiedChinese => format!("{count} 条记录") },
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
        tab_bar: RECT,
        page: RECT,
        status: RECT,
        tabs: [RECT; 3],
    }

    impl ManagerLayout {
        fn for_client(width: i32, height: i32, dpi: u32) -> Self {
            let logical_width = unscale_from_dpi(width.max(1), dpi);
            let logical_height = unscale_from_dpi(height.max(1), dpi);
            let tab = |index: i32| RECT {
                left: 12 + index * (TAB_BUTTON_WIDTH + 4),
                top: (TAB_BAR_HEIGHT - TAB_BUTTON_HEIGHT) / 2,
                right: 12 + index * (TAB_BUTTON_WIDTH + 4) + TAB_BUTTON_WIDTH,
                bottom: (TAB_BAR_HEIGHT - TAB_BUTTON_HEIGHT) / 2 + TAB_BUTTON_HEIGHT,
            };
            Self {
                tab_bar: RECT {
                    left: 0,
                    top: 0,
                    right: logical_width,
                    bottom: TAB_BAR_HEIGHT,
                },
                page: RECT {
                    left: 0,
                    top: TAB_BAR_HEIGHT,
                    right: logical_width,
                    bottom: logical_height,
                },
                status: RECT {
                    left: (logical_width - 16 - 380).max(310),
                    top: (TAB_BAR_HEIGHT - 20) / 2,
                    right: logical_width - 16,
                    bottom: (TAB_BAR_HEIGHT - 20) / 2 + 20,
                },
                tabs: [tab(0), tab(1), tab(2)],
            }
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
        card: HBRUSH,
        body_font: HFONT,
        title_font: HFONT,
        caps_font: HFONT,
        small_font: HFONT,
        note_font: HFONT,
        mono_font: HFONT,
    }

    impl ThemeResources {
        fn new(dpi: u32) -> Self {
            let face = w!("Segoe UI");
            let mono_face = w!("Consolas");
            let font = |size: i32, weight: i32, face: PCWSTR| unsafe {
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
            Self {
                background: unsafe { CreateSolidBrush(MANAGER_BG) },
                surface: unsafe { CreateSolidBrush(TAB_BG) },
                card: unsafe { CreateSolidBrush(SURFACE_BG) },
                body_font: font(13, 400, face),
                title_font: font(26, 600, face),
                caps_font: font(10, 600, face),
                small_font: font(11, 400, face),
                note_font: font(12, 400, face),
                mono_font: font(12, 400, mono_face),
            }
        }
    }

    impl Drop for ThemeResources {
        fn drop(&mut self) {
            unsafe {
                for object in [
                    HGDIOBJ(self.background.0),
                    HGDIOBJ(self.surface.0),
                    HGDIOBJ(self.card.0),
                    HGDIOBJ(self.body_font.0),
                    HGDIOBJ(self.title_font.0),
                    HGDIOBJ(self.caps_font.0),
                    HGDIOBJ(self.small_font.0),
                    HGDIOBJ(self.note_font.0),
                    HGDIOBJ(self.mono_font.0),
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
        /// Dynamically created profile reorder rows (one per profile), the
        /// rail divider after the inline limit, and the rail hint line.
        profile_rows: Vec<ProfileOrderRow>,
        profiles_divider: HWND,
        profiles_hint: HWND,
        endpoint: HWND,
        model: HWND,
        credential_target: HWND,
        api_key: HWND,
        credential_status: HWND,
        hotkey_selection: HWND,
        hotkey_hover: HWND,
        hotkey_pause: HWND,
        hotkey_open_popup: HWND,
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
                profile_rows: Vec::new(),
                profiles_divider: null,
                profiles_hint: null,
                endpoint: null,
                model: null,
                credential_target: null,
                api_key: null,
                credential_status: null,
                hotkey_selection: null,
                hotkey_hover: null,
                hotkey_pause: null,
                hotkey_open_popup: null,
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
                status: null,
                placements: Vec::new(),
                localized: Vec::new(),
            }
        }
    }

    struct ProfileOrderRow {
        label: HWND,
        up: HWND,
        down: HWND,
    }

    struct ManagerState {
        config: AppConfig,
        config_path: Option<PathBuf>,
        /// Last seen mtime of config.toml: a tray skin switch saves the file,
        /// and an open manager must follow the popup/manager skin change.
        config_skin_mtime: Option<std::time::SystemTime>,
        load_error: Option<String>,
        /// True when `config` was loaded from an existing, valid file. When
        /// false it is only defaults, and saves must not clobber a config
        /// file that exists on disk.
        loaded_from_disk: bool,
        handles: Handles,
        view: View,
        profile_index: usize,
        /// A new profile is kept outside `config` until its explicit Save
        /// action succeeds. This prevents Settings saves or navigation from
        /// accidentally persisting an unfinished draft.
        draft_prompt: Option<PromptConfig>,
        history_entries: Vec<HistoryEntry>,
        history_loaded: bool,
        /// True when the selected history entry carries sentence context;
        /// drives the target/context pane split in the Selection well.
        history_has_context: bool,
        /// Last laid-out Selection well bands (target, context) in window
        /// pixels, so selection changes can re-split without recomputing
        /// page geometry.
        history_band_rects: Option<(RECT, RECT)>,
        /// Compact (closed) heights of comboboxes while their dropdown list
        /// is expanded, keyed by control id. The responsive layout gives
        /// combos only the closed selection-field height, and Windows derives
        /// the dropped-list height from the window height, so the list must
        /// be given room while it is open.
        combo_compact_heights: Vec<(i32, i32)>,
        resident_start: ResidentStartOutcome,
        credential_status: CredentialStatusState,
        /// Fair-copy manager view companion. The native window stays the
        /// source of truth; the page mirrors it and posts edits back.
        fair_copy: Option<webview_ui::FairCopyUi>,
        /// The page reported it loaded: from then on it IS the manager
        /// window, so its exit closes the manager.
        fair_copy_ready: bool,
        theme: ThemeResources,
        dpi: u32,
        /// Vertical scroll offset of the Settings page in logical pixels.
        /// The Profiles order card grows with the profile count, so a tall
        /// stack overflows the container and the page scrolls.
        settings_scroll: i32,
        settings_scroll_max: i32,
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
        // Single instance: a second manager process would race the first one
        // for the single-instance fair-copy pipe and fall back to the native
        // UI, so instead surface the existing manager window.
        // Test harnesses may name their own instance so a probe can run next
        // to the user's manager; the default name is the product contract.
        let instance_name: Vec<u16> = std::env::var("LINGOSNAP_MANAGER_INSTANCE")
            .unwrap_or_else(|_| "LingoSnapManagerInstance".to_owned())
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let _single_instance =
            unsafe { CreateMutexW(None, false, PCWSTR(instance_name.as_ptr())) }?;
        let already_running = unsafe { windows::Win32::Foundation::GetLastError() }
            == windows::Win32::Foundation::ERROR_ALREADY_EXISTS;
        if already_running {
            // Surface whichever window presents the running manager: the
            // fair-copy page when it is up (the native window is hidden
            // behind it and must stay hidden), otherwise the native window.
            let page = unsafe {
                FindWindowW(windows::core::w!("LingoSnapManagerUi"), PCWSTR::null()).ok()
            }
            .filter(|hwnd| {
                unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(*hwnd) }.as_bool()
            });
            let native =
                unsafe { FindWindowW(windows::core::w!("LingoSnapManager"), PCWSTR::null()).ok() };
            if let Some(hwnd) = page.or(native) {
                unsafe {
                    if windows::Win32::UI::WindowsAndMessaging::IsIconic(hwnd).as_bool() {
                        let _ =
                            ShowWindow(hwnd, windows::Win32::UI::WindowsAndMessaging::SW_RESTORE);
                    } else {
                        let _ = ShowWindow(hwnd, SW_SHOW);
                    }
                    let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd);
                }
            }
            return Ok(());
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
        if let Ok(module) = unsafe { LoadLibraryW(w!("msftedit.dll")) } {
            let _ = RICH_EDIT_MODULE.set(module.0 as isize);
        }
        let (config, load_error, loaded_from_disk) = load_config();
        let initial_language = config.ui.manager_language;
        let window_title = wide(ui_text(initial_language, TextKey::WindowTitle));
        let dpi = unsafe { GetDpiForSystem() }.max(DEFAULT_DPI);
        let state = Box::new(ManagerState {
            config,
            config_path: default_config_path(),
            config_skin_mtime: None,
            load_error,
            loaded_from_disk,
            handles: Handles::default(),
            view: View::Settings,
            profile_index: 0,
            draft_prompt: None,
            history_entries: Vec::new(),
            history_loaded: false,
            history_has_context: false,
            history_band_rects: None,
            combo_compact_heights: Vec::new(),
            resident_start,
            credential_status: CredentialStatusState::Absent,
            fair_copy: None,
            fair_copy_ready: false,
            theme: ThemeResources::new(dpi),
            dpi,
            settings_scroll: 0,
            settings_scroll_max: 0,
        });
        let state_ptr = Box::into_raw(state);
        let hwnd = unsafe {
            match CreateWindowExW(
                WS_EX_CONTROLPARENT,
                CLASS_NAME,
                PCWSTR(window_title.as_ptr()),
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN,
                windows::Win32::UI::WindowsAndMessaging::CW_USEDEFAULT,
                windows::Win32::UI::WindowsAndMessaging::CW_USEDEFAULT,
                scale_for_dpi(980, dpi),
                scale_for_dpi(760, dpi),
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
        set_window_icons(hwnd);
        unsafe {
            let window_dpi = GetDpiForWindow(hwnd).max(DEFAULT_DPI);
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                scale_for_dpi(980, window_dpi),
                scale_for_dpi(760, window_dpi),
                SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOMOVE,
            );
            if !FAIR_COPY_ACTIVE.load(std::sync::atomic::Ordering::Relaxed) {
                let _ = ShowWindow(
                    hwnd,
                    windows::Win32::UI::WindowsAndMessaging::SW_SHOWDEFAULT,
                );
            }
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

    /// Loads the on-disk config. The third element reports whether the
    /// values came from an existing, valid file — when false, the in-memory
    /// config is only defaults and must never overwrite a file that appears.
    fn load_config() -> (AppConfig, Option<String>, bool) {
        let Some(path) = default_config_path() else {
            return (
                AppConfig::default(),
                Some(status_text(
                    UiLanguage::English,
                    StatusEvent::LocalAppDataUnavailable {
                        operation: StatusOperation::Save,
                    },
                )),
                false,
            );
        };
        if !path.exists() {
            return (AppConfig::default(), None, false);
        }
        match AppConfig::load(&path) {
            Ok(config) => (config, None, true),
            Err(error) => (
                AppConfig::default(),
                Some(status_text(
                    UiLanguage::English,
                    StatusEvent::ConfigLoadFailed {
                        detail: &error.to_string(),
                    },
                )),
                false,
            ),
        }
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
                scale_for_dpi(980, window_dpi),
                scale_for_dpi(760, window_dpi),
                SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOMOVE,
            );
            apply_manager_layout(hwnd, &mut *state);
            start_fair_copy_view(hwnd, &mut *state);
            let _ = SetTimer(Some(hwnd), CONFIG_POLL_TIMER_ID, 1000, None);
            return LRESULT(0);
        }
        let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ManagerState;
        if state.is_null() {
            return DefWindowProcW(hwnd, message, wparam, lparam);
        }
        match message {
            WM_COMMAND => handle_command(hwnd, &mut *state, wparam.0, HWND(lparam.0 as *mut _)),
            webview_ui::FAIR_COPY_COMMAND_MESSAGE => {
                let raw = lparam.0 as *mut String;
                if !raw.is_null() {
                    let line = unsafe { Box::from_raw(raw) };
                    handle_fair_copy_command(hwnd, &mut *state, &line);
                }
                LRESULT(0)
            }
            WM_TIMER if wparam.0 == CONFIG_POLL_TIMER_ID => {
                // A tray skin switch saves config.toml; an open manager must
                // follow the popup/manager skin change live.
                if let Some(path) = &(*state).config_path {
                    if let Ok(meta) = std::fs::metadata(path) {
                        let mtime = meta.modified().ok();
                        if mtime != (*state).config_skin_mtime {
                            (*state).config_skin_mtime = mtime;
                            if let Ok(current) = lingosnap_core::AppConfig::load(path) {
                                let skin = current.ui.popup_skin;
                                (*state).config.ui.popup_skin = skin;
                                // Follow a tray skin switch: start the page
                                // for fair-copy, hand back to the native
                                // window for classic.
                                if skin == lingosnap_core::config::PopupSkin::FairCopy {
                                    start_fair_copy_view(hwnd, &mut *state);
                                } else if (*state).fair_copy.is_some() {
                                    if let Some(mut ui) = (*state).fair_copy.take() {
                                        (*state).fair_copy_ready = false;
                                        FAIR_COPY_ACTIVE
                                            .store(false, std::sync::atomic::Ordering::Relaxed);
                                        ui.shutdown();
                                        let _ = KillTimer(Some(hwnd), FAIR_COPY_POLL_TIMER_ID);
                                        let _ = ShowWindow(hwnd, SW_SHOW);
                                    }
                                }
                            }
                        }
                    }
                }
                LRESULT(0)
            }
            WM_TIMER if wparam.0 == FAIR_COPY_POLL_TIMER_ID => {
                let exited = (*state)
                    .fair_copy
                    .as_mut()
                    .map(|ui| ui.child_exited())
                    .unwrap_or(true);
                if exited {
                    (*state).fair_copy = None;
                    unsafe {
                        let _ = KillTimer(Some(hwnd), FAIR_COPY_POLL_TIMER_ID);
                    };
                    let ready = std::mem::take(&mut (*state).fair_copy_ready);
                    if ready
                        && (*state).config.ui.popup_skin
                            == lingosnap_core::config::PopupSkin::FairCopy
                    {
                        // The user closed the fair-copy manager window: that
                        // closes the manager, exactly like closing the native
                        // window. Revealing the hidden classic UI here is
                        // what made the two skins tangle.
                        FAIR_COPY_ACTIVE.store(false, std::sync::atomic::Ordering::Relaxed);
                        let _ = DestroyWindow(hwnd);
                    } else {
                        // The page never came up (missing runtime, load
                        // failure): the native manager is the fallback.
                        FAIR_COPY_ACTIVE.store(false, std::sync::atomic::Ordering::Relaxed);
                        let _ = ShowWindow(hwnd, SW_SHOW);
                    }
                }
                LRESULT(0)
            }
            WM_VSCROLL
                if lparam.0 == 0
                    || lparam.0 as usize == (*state).handles.settings_page.0 as usize =>
            {
                handle_settings_scroll(hwnd, &mut *state, wparam);
                LRESULT(0)
            }
            WM_MOUSEWHEEL => {
                if (*state).view == View::Settings {
                    // Each notch scrolls three reorder rows.
                    let delta = ((wparam.0 >> 16) as u16 as i16) as i32;
                    let lines = -delta / 120 * 3;
                    if lines != 0 {
                        (*state).settings_scroll = ((*state).settings_scroll + lines * 32)
                            .clamp(0, (*state).settings_scroll_max);
                        apply_manager_layout(hwnd, &mut *state);
                    }
                    LRESULT(0)
                } else {
                    DefWindowProcW(hwnd, message, wparam, lparam)
                }
            }
            WM_SIZE => {
                apply_manager_layout(hwnd, &mut *state);
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
                apply_manager_layout(hwnd, &mut *state);
                let _ = InvalidateRect(Some(hwnd), None, true);
                LRESULT(0)
            }
            WM_GETMINMAXINFO => {
                let limits = &mut *(lparam.0 as *mut MINMAXINFO);
                limits.ptMinTrackSize.x = scale_for_dpi(MIN_CONTENT_WIDTH, (*state).dpi);
                limits.ptMinTrackSize.y = scale_for_dpi(MIN_CLIENT_HEIGHT + 40, (*state).dpi);
                LRESULT(0)
            }
            WM_PAINT => {
                paint_manager(hwnd, &*state);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_DRAWITEM => {
                if lparam.0 != 0 {
                    let item = &*(lparam.0 as *const DRAWITEMSTRUCT);
                    if item.CtlType == ODT_LISTBOX {
                        draw_history_list_item(item, &*state);
                    } else {
                        draw_manager_button(item, &*state);
                    }
                }
                LRESULT(1)
            }
            WM_MEASUREITEM => {
                if lparam.0 != 0 {
                    let measure = &mut *(lparam.0 as *mut MEASUREITEMSTRUCT);
                    if measure.CtlType == ODT_LISTBOX {
                        measure.itemHeight = scale_for_dpi(56, (*state).dpi).max(8) as u32;
                        return LRESULT(1);
                    }
                }
                DefWindowProcW(hwnd, message, wparam, lparam)
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
                // The page owns the Settings scrollbar; the scroll state and
                // layout live in the main window's ManagerState.
                | WM_VSCROLL
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

    /// The LingoSnap icon (resource ID 1 of this executable) at `size` px.
    fn load_app_icon(size: i32) -> Option<windows::Win32::UI::WindowsAndMessaging::HICON> {
        use windows::core::PCWSTR;
        use windows::Win32::Foundation::HINSTANCE;
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows::Win32::UI::WindowsAndMessaging::{
            LoadImageW, HICON, IMAGE_ICON, LR_DEFAULTCOLOR,
        };
        unsafe {
            let module = GetModuleHandleW(None).ok()?;
            let handle = LoadImageW(
                Some(HINSTANCE(module.0)),
                PCWSTR(std::ptr::without_provenance::<u16>(1)),
                IMAGE_ICON,
                size,
                size,
                LR_DEFAULTCOLOR,
            )
            .ok()?;
            Some(HICON(handle.0))
        }
    }

    /// Give a window the big (taskbar, Alt+Tab) and small (title bar) icons.
    fn set_window_icons(hwnd: HWND) {
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{
            GetSystemMetrics, SendMessageW, ICON_BIG, ICON_SMALL, SM_CXICON, SM_CXSMICON,
            WM_SETICON,
        };
        for (kind, metric) in [(ICON_BIG, SM_CXICON), (ICON_SMALL, SM_CXSMICON)] {
            if let Some(icon) = load_app_icon(unsafe { GetSystemMetrics(metric) }) {
                unsafe {
                    SendMessageW(
                        hwnd,
                        WM_SETICON,
                        Some(WPARAM(kind as usize)),
                        Some(LPARAM(icon.0 as isize)),
                    );
                }
            }
        }
    }

    fn initialize_controls(hwnd: HWND, state: &mut ManagerState) -> windows::core::Result<()> {
        let mut h = Handles::default();
        // Horizontal tab bar (Settings · Prompts · History); the live status
        // label sits on the right side of the same bar. Geometry comes from
        // ManagerLayout during apply_manager_layout.
        h.settings_nav = add_button(
            hwnd,
            &mut h,
            ID_SETTINGS_TAB,
            "Settings",
            None,
            Slot::SaveSettings,
        )?;
        h.prompts_nav = add_button(
            hwnd,
            &mut h,
            ID_PROMPTS_TAB,
            "Profiles",
            None,
            Slot::SaveSettings,
        )?;
        h.history_nav = add_button(
            hwnd,
            &mut h,
            ID_HISTORY_TAB,
            "History",
            None,
            Slot::SaveSettings,
        )?;

        // Page controls live under one dedicated child container each.  This
        // makes tab switching a single parent visibility operation; hidden
        // controls can never paint over the newly selected page.
        h.settings_page = create_page_container(hwnd, ID_SETTINGS_PAGE)?;
        h.prompts_page = create_page_container(hwnd, ID_PROMPTS_PAGE)?;
        h.history_page = create_page_container(hwnd, ID_HISTORY_PAGE)?;

        // —— Settings: grouped cards (Provider / Credentials / Defaults) ——
        add_label(
            hwnd,
            &mut h,
            "Provider endpoint",
            Some(View::Settings),
            Slot::EndpointLabel,
        )?;
        h.endpoint = add_edit_with_id(
            hwnd,
            &mut h,
            Some(View::Settings),
            Slot::Endpoint,
            false,
            false,
            ID_SETTINGS_ENDPOINT,
        )?;
        add_label(
            hwnd,
            &mut h,
            "Model",
            Some(View::Settings),
            Slot::ModelLabel,
        )?;
        h.model = add_edit_with_id(
            hwnd,
            &mut h,
            Some(View::Settings),
            Slot::Model,
            false,
            false,
            ID_SETTINGS_MODEL,
        )?;
        add_label(
            hwnd,
            &mut h,
            "Credential target",
            Some(View::Settings),
            Slot::CredentialTargetLabel,
        )?;
        h.credential_target = add_edit_with_id(
            hwnd,
            &mut h,
            Some(View::Settings),
            Slot::CredentialTarget,
            false,
            false,
            ID_SETTINGS_CREDENTIAL_TARGET,
        )?;

        add_label(
            hwnd,
            &mut h,
            "API key",
            Some(View::Settings),
            Slot::ApiKeyLabel,
        )?;
        h.api_key = add_edit_with_id(
            hwnd,
            &mut h,
            Some(View::Settings),
            Slot::ApiKey,
            true,
            false,
            ID_SETTINGS_API_KEY,
        )?;
        add_button(
            hwnd,
            &mut h,
            ID_SAVE_KEY,
            "Save key",
            Some(View::Settings),
            Slot::SaveKey,
        )?;
        add_button(
            hwnd,
            &mut h,
            ID_DELETE_KEY,
            "Delete saved key",
            Some(View::Settings),
            Slot::DeleteKey,
        )?;
        h.credential_status = add_label(
            hwnd,
            &mut h,
            "",
            Some(View::Settings),
            Slot::CredentialStatus,
        )?;
        add_label(
            hwnd,
            &mut h,
            "Keys are held by Windows Credential Manager; they never enter config.toml.",
            Some(View::Settings),
            Slot::CredentialHint,
        )?;

        add_label(
            hwnd,
            &mut h,
            "Selection profile",
            Some(View::Settings),
            Slot::SelectionLabel,
        )?;
        h.settings_selection_default = add_combo(
            hwnd,
            &mut h,
            ID_SETTINGS_SELECTION_DEFAULT,
            Some(View::Settings),
            Slot::SelectionDefault,
        )?;
        add_label(
            hwnd,
            &mut h,
            "Hover profile",
            Some(View::Settings),
            Slot::HoverLabel,
        )?;
        h.settings_hover_default = add_combo(
            hwnd,
            &mut h,
            ID_SETTINGS_HOVER_DEFAULT,
            Some(View::Settings),
            Slot::HoverDefault,
        )?;
        add_label(
            hwnd,
            &mut h,
            "Interface language",
            Some(View::Settings),
            Slot::LangLabel,
        )?;
        h.language = add_combo(
            hwnd,
            &mut h,
            ID_LANGUAGE,
            Some(View::Settings),
            Slot::Language,
        )?;

        add_label(
            hwnd,
            &mut h,
            "Selection translate",
            Some(View::Settings),
            Slot::HotkeySelectionLabel,
        )?;
        h.hotkey_selection = add_edit_with_id(
            hwnd,
            &mut h,
            Some(View::Settings),
            Slot::HotkeySelection,
            false,
            false,
            ID_HOTKEY_SELECTION,
        )?;
        add_label(
            hwnd,
            &mut h,
            "Hover translate",
            Some(View::Settings),
            Slot::HotkeyHoverLabel,
        )?;
        h.hotkey_hover = add_edit_with_id(
            hwnd,
            &mut h,
            Some(View::Settings),
            Slot::HotkeyHover,
            false,
            false,
            ID_HOTKEY_HOVER,
        )?;
        add_label(
            hwnd,
            &mut h,
            "Pause / resume",
            Some(View::Settings),
            Slot::HotkeyPauseLabel,
        )?;
        h.hotkey_pause = add_edit_with_id(
            hwnd,
            &mut h,
            Some(View::Settings),
            Slot::HotkeyPause,
            false,
            false,
            ID_HOTKEY_PAUSE,
        )?;
        add_label(
            hwnd,
            &mut h,
            "Open popup",
            Some(View::Settings),
            Slot::HotkeyOpenPopupLabel,
        )?;
        h.hotkey_open_popup = add_edit_with_id(
            hwnd,
            &mut h,
            Some(View::Settings),
            Slot::HotkeyOpenPopup,
            false,
            false,
            ID_HOTKEY_OPEN_POPUP,
        )?;
        for handle in [
            h.hotkey_selection,
            h.hotkey_hover,
            h.hotkey_pause,
            h.hotkey_open_popup,
        ] {
            // Empty field means the hotkey is off; the cue banner says so.
            const EM_SETCUEBANNER: u32 = 0x1501;
            let cue: Vec<u16> = "Off — no hot key\0".encode_utf16().collect();
            unsafe {
                let _ = SendMessageW(
                    handle,
                    EM_SETCUEBANNER,
                    Some(WPARAM(1)),
                    Some(LPARAM(cue.as_ptr() as isize)),
                );
            }
        }

        add_button(
            hwnd,
            &mut h,
            ID_SAVE_SETTINGS,
            "Save settings",
            Some(View::Settings),
            Slot::SaveSettings,
        )?;

        // —— Prompts: meta row on top, side-by-side editors below ——
        add_label(
            hwnd,
            &mut h,
            "Profile",
            Some(View::Prompts),
            Slot::PromptIdLabel,
        )?;
        h.profile_id = add_combo_with_kind(
            hwnd,
            &mut h,
            ID_PROMPT_ID,
            Some(View::Prompts),
            Slot::PromptId,
            CBS_DROPDOWNLIST,
        )?;
        add_label(
            hwnd,
            &mut h,
            "Name",
            Some(View::Prompts),
            Slot::PromptNameLabel,
        )?;
        h.profile_name = add_edit_with_id(
            hwnd,
            &mut h,
            Some(View::Prompts),
            Slot::PromptName,
            false,
            false,
            ID_PROMPT_NAME,
        )?;
        add_label(
            hwnd,
            &mut h,
            "Model override",
            Some(View::Prompts),
            Slot::PromptModelLabel,
        )?;
        h.profile_model = add_edit_with_id(
            hwnd,
            &mut h,
            Some(View::Prompts),
            Slot::PromptModel,
            false,
            false,
            ID_PROMPT_MODEL,
        )?;
        add_label(
            hwnd,
            &mut h,
            "Temperature",
            Some(View::Prompts),
            Slot::PromptTempLabel,
        )?;
        h.temperature = add_edit_with_id(
            hwnd,
            &mut h,
            Some(View::Prompts),
            Slot::PromptTemp,
            false,
            false,
            ID_PROMPT_TEMPERATURE,
        )?;
        add_label(
            hwnd,
            &mut h,
            "Max tokens",
            Some(View::Prompts),
            Slot::PromptTokensLabel,
        )?;
        h.max_tokens = add_edit_with_id(
            hwnd,
            &mut h,
            Some(View::Prompts),
            Slot::PromptTokens,
            false,
            false,
            ID_PROMPT_MAX_TOKENS,
        )?;
        add_button(
            hwnd,
            &mut h,
            ID_NEW_PROMPT,
            "New",
            Some(View::Prompts),
            Slot::NewPrompt,
        )?;
        add_button(
            hwnd,
            &mut h,
            ID_SAVE_PROMPT,
            "Save profile",
            Some(View::Prompts),
            Slot::SavePrompt,
        )?;
        add_button(
            hwnd,
            &mut h,
            ID_DELETE_PROMPT,
            "Delete profile",
            Some(View::Prompts),
            Slot::DeletePrompt,
        )?;
        add_label(
            hwnd,
            &mut h,
            "Use {target}, {context}, and {source}; every user template needs {target}.",
            Some(View::Prompts),
            Slot::PromptHint,
        )?;
        add_label(
            hwnd,
            &mut h,
            "Model override: leave empty to use the model set in Settings.",
            Some(View::Prompts),
            Slot::PromptModelHint,
        )?;
        add_label(
            hwnd,
            &mut h,
            "Temperature: 0–2. Lower gives steadier answers, higher gives more varied ones. Empty uses the provider default.",
            Some(View::Prompts),
            Slot::PromptTempHint,
        )?;
        add_label(
            hwnd,
            &mut h,
            "Max tokens: the longest reply allowed, in tokens. Empty uses the provider default.",
            Some(View::Prompts),
            Slot::PromptTokensHint,
        )?;
        h.system_prompt = add_rich_edit(
            hwnd,
            &mut h,
            Some(View::Prompts),
            Slot::SystemPrompt,
            true,
            ID_PROMPT_SYSTEM,
        )?;
        h.user_template = add_rich_edit(
            hwnd,
            &mut h,
            Some(View::Prompts),
            Slot::UserTemplate,
            true,
            ID_PROMPT_USER_TEMPLATE,
        )?;
        h.prompt_status = add_label(hwnd, &mut h, "", Some(View::Prompts), Slot::PromptStatus)?;

        // —— History: search card over a list/detail split ——
        h.history_search = add_edit_with_id(
            hwnd,
            &mut h,
            Some(View::History),
            Slot::HistorySearch,
            false,
            false,
            ID_HISTORY_SEARCH,
        )?;
        add_button(
            hwnd,
            &mut h,
            ID_HISTORY_REFRESH,
            "Refresh",
            Some(View::History),
            Slot::HistoryRefresh,
        )?;
        add_button(
            hwnd,
            &mut h,
            ID_HISTORY_COPY,
            "Copy output",
            Some(View::History),
            Slot::HistoryCopy,
        )?;
        h.history_prompt = add_combo(
            hwnd,
            &mut h,
            ID_HISTORY_PROMPT,
            Some(View::History),
            Slot::HistPromptFilter,
        )?;
        h.history_source = add_combo(
            hwnd,
            &mut h,
            ID_HISTORY_SOURCE,
            Some(View::History),
            Slot::HistSourceFilter,
        )?;
        h.history_order = add_combo(
            hwnd,
            &mut h,
            ID_HISTORY_ORDER,
            Some(View::History),
            Slot::HistOrderFilter,
        )?;
        h.history_list = add_list(hwnd, &mut h, Some(View::History), Slot::HistoryList)?;
        h.history_target =
            add_readonly_edit(hwnd, &mut h, Some(View::History), Slot::HistoryTarget, true)?;
        h.history_context = add_readonly_edit(
            hwnd,
            &mut h,
            Some(View::History),
            Slot::HistoryContext,
            true,
        )?;
        // The context pane starts hidden; apply_history_detail_split shows
        // it only while the selected entry carries sentence context.
        unsafe {
            let _ = ShowWindow(h.history_context, SW_HIDE);
        }
        h.history_output =
            add_readonly_rich_edit(hwnd, &mut h, Some(View::History), Slot::HistoryOutput, true)?;
        for rich in [h.system_prompt, h.user_template, h.history_output] {
            apply_rich_ink(rich);
        }
        add_label(
            hwnd,
            &mut h,
            "History is loaded only while this tab is open; the database is never held open.",
            Some(View::History),
            Slot::HistoryHint,
        )?;
        h.history_meta = add_label(hwnd, &mut h, "", Some(View::History), Slot::HistoryMeta)?;
        add_button(
            hwnd,
            &mut h,
            ID_HISTORY_DELETE,
            "Delete selected",
            Some(View::History),
            Slot::HistoryDelete,
        )?;

        h.status = add_label(hwnd, &mut h, "", None, Slot::PromptStatus)?;
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

    /// Identifies a control's role in the responsive page layout. Geometry
    /// is computed per slot in `page_geometry` from the live window size, so
    /// components stretch to use the whole window.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Slot {
        EndpointLabel,
        Endpoint,
        ModelLabel,
        Model,
        CredentialTargetLabel,
        CredentialTarget,
        ApiKeyLabel,
        ApiKey,
        SaveKey,
        DeleteKey,
        CredentialStatus,
        CredentialHint,
        SelectionLabel,
        SelectionDefault,
        HoverLabel,
        HoverDefault,
        LangLabel,
        Language,
        HotkeySelectionLabel,
        HotkeySelection,
        HotkeyHoverLabel,
        HotkeyHover,
        HotkeyPauseLabel,
        HotkeyPause,
        HotkeyOpenPopupLabel,
        HotkeyOpenPopup,
        SaveSettings,
        PromptIdLabel,
        PromptId,
        PromptNameLabel,
        PromptName,
        PromptModelLabel,
        PromptModel,
        PromptTempLabel,
        PromptTemp,
        PromptTokensLabel,
        PromptTokens,
        NewPrompt,
        DeletePrompt,
        SavePrompt,
        PromptHint,
        PromptModelHint,
        PromptTempHint,
        PromptTokensHint,
        SystemPrompt,
        UserTemplate,
        PromptStatus,
        HistorySearch,
        HistoryRefresh,
        HistoryCopy,
        HistPromptFilter,
        HistSourceFilter,
        HistOrderFilter,
        HistoryList,
        HistoryTarget,
        HistoryContext,
        HistoryOutput,
        HistoryHint,
        HistoryMeta,
        HistoryDelete,
        // Dynamic Settings-page controls (profile reorder rows, rail divider,
        // rail hint). They share one slot because geometry positions them
        // manually relative to the Profiles card; the slot only carries font,
        // theme, and card-background treatment.
        ProfilesDynamic,
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

    const PAGE_TOP: i32 = TAB_BAR_HEIGHT;

    fn create_page_container(parent: HWND, id: usize) -> windows::core::Result<HWND> {
        create_control_with_style(
            parent,
            PAGE_CLASS,
            "",
            WS_CHILD | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
            0,
            PAGE_TOP,
            1000,
            700,
            id,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn add_label(
        parent: HWND,
        h: &mut Handles,
        text: &str,
        view: Option<View>,
        slot: Slot,
    ) -> windows::core::Result<HWND> {
        let target = page_parent(parent, h, view);
        let hwnd = create_control(
            target,
            w!("STATIC"),
            text,
            WS_CHILD | visibility_style(view),
            0,
            0,
            100,
            20,
            0,
        )?;
        if let Some(key) = text_key_from_english(text) {
            h.localized.push(LocalizedControl { hwnd, key });
        }
        Ok(add_control(h, hwnd, view, slot))
    }

    fn add_button(
        parent: HWND,
        h: &mut Handles,
        id: usize,
        text: &str,
        view: Option<View>,
        slot: Slot,
    ) -> windows::core::Result<HWND> {
        let target = page_parent(parent, h, view);
        let hwnd = create_control(
            target,
            w!("BUTTON"),
            text,
            WS_CHILD
                | visibility_style(view)
                | WS_TABSTOP
                | WINDOW_STYLE(BS_PUSHBUTTON as u32 | BS_OWNERDRAW as u32),
            0,
            0,
            100,
            28,
            id,
        )?;
        if let Some(key) = text_key_from_english(text) {
            h.localized.push(LocalizedControl { hwnd, key });
        }
        Ok(add_control(h, hwnd, view, slot))
    }

    fn add_edit(
        parent: HWND,
        h: &mut Handles,
        view: Option<View>,
        slot: Slot,
        password: bool,
        multiline: bool,
    ) -> windows::core::Result<HWND> {
        add_edit_with_id(parent, h, view, slot, password, multiline, 0)
    }

    fn add_edit_with_id(
        parent: HWND,
        h: &mut Handles,
        view: Option<View>,
        slot: Slot,
        password: bool,
        multiline: bool,
        id: usize,
    ) -> windows::core::Result<HWND> {
        let target = page_parent(parent, h, view);
        let mut style =
            WS_CHILD | WS_CLIPSIBLINGS | visibility_style(view) | WS_TABSTOP | WS_BORDER;
        if multiline {
            // Word-wrapping editor: no ES_AUTOHSCROLL (it would disable wrap),
            // vertical scrollbar for overflow.
            style |= WINDOW_STYLE(ES_MULTILINE as u32)
                | WINDOW_STYLE(ES_AUTOVSCROLL as u32)
                | WS_VSCROLL;
        } else {
            style |= WINDOW_STYLE(ES_AUTOHSCROLL as u32);
        }
        if password {
            style |= WINDOW_STYLE(ES_PASSWORD as u32);
        }
        let hwnd = create_control(target, w!("EDIT"), "", style, 0, 0, 100, 24, id)?;
        Ok(add_control(h, hwnd, view, slot))
    }

    fn add_readonly_edit(
        parent: HWND,
        h: &mut Handles,
        view: Option<View>,
        slot: Slot,
        multiline: bool,
    ) -> windows::core::Result<HWND> {
        let hwnd = add_edit(parent, h, view, slot, false, multiline)?;
        unsafe {
            let _ = SendMessageW(hwnd, EM_SETREADONLY, Some(WPARAM(1)), Some(LPARAM(0)));
        }
        Ok(hwnd)
    }

    /// Rich (RICHEDIT50W) multiline control for markdown-aware surfaces.
    /// Falls back to a plain EDIT when msftedit.dll is unavailable.
    fn add_readonly_rich_edit(
        parent: HWND,
        h: &mut Handles,
        view: Option<View>,
        slot: Slot,
        multiline: bool,
    ) -> windows::core::Result<HWND> {
        let hwnd = add_edit_with_class(
            parent,
            h,
            view,
            slot,
            false,
            multiline,
            rich_edit_class(),
            0,
        )?;
        unsafe {
            let _ = SendMessageW(hwnd, EM_SETREADONLY, Some(WPARAM(1)), Some(LPARAM(0)));
        }
        Ok(hwnd)
    }

    fn add_rich_edit(
        parent: HWND,
        h: &mut Handles,
        view: Option<View>,
        slot: Slot,
        multiline: bool,
        id: usize,
    ) -> windows::core::Result<HWND> {
        add_edit_with_class(
            parent,
            h,
            view,
            slot,
            false,
            multiline,
            rich_edit_class(),
            id,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn add_edit_with_class(
        parent: HWND,
        h: &mut Handles,
        view: Option<View>,
        slot: Slot,
        password: bool,
        multiline: bool,
        class: windows::core::PCWSTR,
        id: usize,
    ) -> windows::core::Result<HWND> {
        let target = page_parent(parent, h, view);
        let mut style =
            WS_CHILD | WS_CLIPSIBLINGS | visibility_style(view) | WS_TABSTOP | WS_BORDER;
        if multiline {
            style |= WINDOW_STYLE(ES_MULTILINE as u32)
                | WINDOW_STYLE(ES_AUTOVSCROLL as u32)
                | WS_VSCROLL;
        } else {
            style |= WINDOW_STYLE(ES_AUTOHSCROLL as u32);
        }
        if password {
            style |= WINDOW_STYLE(ES_PASSWORD as u32);
        }
        let hwnd = create_control(target, class, "", style, 0, 0, 100, 24, id)?;
        Ok(add_control(h, hwnd, view, slot))
    }

    fn add_combo(
        parent: HWND,
        h: &mut Handles,
        id: usize,
        view: Option<View>,
        slot: Slot,
    ) -> windows::core::Result<HWND> {
        add_combo_with_kind(parent, h, id, view, slot, CBS_DROPDOWNLIST)
    }

    /// `kind` is the combo box style (`CBS_*`).
    fn add_combo_with_kind(
        parent: HWND,
        h: &mut Handles,
        id: usize,
        view: Option<View>,
        slot: Slot,
        kind: u32,
    ) -> windows::core::Result<HWND> {
        let target = page_parent(parent, h, view);
        let style = WS_CHILD
            | WS_CLIPSIBLINGS
            | visibility_style(view)
            | WS_TABSTOP
            | WS_VSCROLL
            | WINDOW_STYLE(kind);
        let hwnd =
            create_control_with_style(target, w!("COMBOBOX"), "", style, 0, 0, 100, 200, id)?;
        Ok(add_control(h, hwnd, view, slot))
    }

    fn add_list(
        parent: HWND,
        h: &mut Handles,
        view: Option<View>,
        slot: Slot,
    ) -> windows::core::Result<HWND> {
        let target = page_parent(parent, h, view);
        // Owner-draw rows render the mockup's three-line history items
        // (target, output preview, time · profile) against the void well.
        let style = WS_CHILD
            | WS_CLIPSIBLINGS
            | visibility_style(view)
            | WS_TABSTOP
            | WS_VSCROLL
            | WINDOW_STYLE(LBS_NOTIFY | LBS_OWNERDRAWFIXED | LBS_NOINTEGRALHEIGHT);
        let hwnd = create_control_with_style(
            target,
            w!("LISTBOX"),
            "",
            style,
            0,
            0,
            100,
            200,
            ID_HISTORY_LIST,
        )?;
        Ok(add_control(h, hwnd, view, slot))
    }
    #[allow(clippy::too_many_arguments)]
    fn create_control(
        parent: HWND,
        class: PCWSTR,
        text: &str,
        style: windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE,
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
            let hwnd = CreateWindowExW(
                Default::default(),
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
            let _ = SetWindowTheme(hwnd, w!("DarkMode_Explorer"), PCWSTR::null());
            Ok(hwnd)
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn create_control_with_style(
        parent: HWND,
        class: PCWSTR,
        text: &str,
        style: windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        id: usize,
    ) -> windows::core::Result<HWND> {
        let text = wide(text);
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
            let _ = SetWindowTheme(hwnd, w!("DarkMode_Explorer"), PCWSTR::null());
            Ok(hwnd)
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

    /// Stack the Selection well panes for the selected history entry: with
    /// sentence context the muted context editor takes the lower band;
    /// without context the mono target editor expands over the whole well
    /// so no dead strip is left behind.
    fn apply_history_detail_split(state: &ManagerState) {
        let Some((target_band, context_band)) = state.history_band_rects else {
            return;
        };
        let target = state.handles.history_target;
        let context = state.handles.history_context;
        if target.0.is_null() || context.0.is_null() {
            return;
        }
        let RECT {
            left: tl,
            top: tt,
            right: trr,
            bottom: tb,
        } = target_band;
        let RECT {
            left: cl,
            top: ct,
            right: crr,
            bottom: cb,
        } = context_band;
        unsafe {
            if state.history_has_context {
                let _ = ShowWindow(context, SW_SHOW);
                let _ = SetWindowPos(
                    target,
                    None,
                    tl,
                    tt,
                    (trr - tl).max(1),
                    (tb - tt).max(1),
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
                let _ = SetWindowPos(
                    context,
                    None,
                    cl,
                    ct,
                    (crr - cl).max(1),
                    (cb - ct).max(1),
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
            } else {
                let _ = ShowWindow(context, SW_HIDE);
                let _ = SetWindowPos(
                    target,
                    None,
                    tl,
                    tt,
                    (trr - tl).max(1),
                    (cb - tt).max(1),
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
            }
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

    fn apply_manager_layout(hwnd: HWND, state: &mut ManagerState) {
        let mut client = RECT::default();
        if unsafe { GetClientRect(hwnd, &mut client) }.is_err() {
            return;
        }
        let layout = ManagerLayout::for_client(client.right, client.bottom, state.dpi);
        for (control, rect) in [
            (state.handles.settings_nav, layout.tabs[0]),
            (state.handles.prompts_nav, layout.tabs[1]),
            (state.handles.history_nav, layout.tabs[2]),
            (state.handles.status, layout.status),
        ] {
            place_control(control, rect, state.dpi);
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
        // Responsive page geometry: every control takes its rect from the
        // same computation the painter uses, so components stretch to use
        // the whole window at any size.
        // Responsive page geometry: every control takes its rect from the
        // same computation the painter uses, so components stretch to use
        // the whole window at any size.
        // Measure the page the painter measures: its CLIENT width, which
        // excludes the Settings scrollbar. Using the window width laid the
        // controls out wider than the painted cards, so fields and the
        // reorder buttons ran past the cards' right edges.
        let active_page = match state.view {
            View::Settings => state.handles.settings_page,
            View::Prompts => state.handles.prompts_page,
            View::History => state.handles.history_page,
        };
        let mut page_client = RECT::default();
        let client_width = if unsafe { GetClientRect(active_page, &mut page_client) }.is_ok()
            && page_client.right > 0
        {
            page_client.right
        } else {
            page_rect.right - page_rect.left
        };
        let page_width = unscale_from_dpi(client_width, state.dpi);
        let container_height = unscale_from_dpi(page_rect.bottom - page_rect.top, state.dpi);
        let mut geometry = page_geometry(
            state.view,
            page_width,
            container_height,
            state.config.profiles.len(),
        );
        // Scroll plumbing for the Settings page: clamp the offset against the
        // freshly computed overflow, shift every rect, then keep the
        // scrollbar honest. Other pages never overflow.
        state.settings_scroll_max = if state.view == View::Settings {
            geometry.scroll_max
        } else {
            0
        };
        state.settings_scroll = state.settings_scroll.min(state.settings_scroll_max).max(0);
        if state.settings_scroll > 0 {
            shift_geometry(&mut geometry, state.settings_scroll);
        }
        update_settings_scrollbar(hwnd, state);
        for (slot, rect) in &geometry.slots {
            if let Some(placement) = state
                .handles
                .placements
                .iter()
                .find(|item| item.view == state.view && item.slot == *slot)
            {
                place_control(
                    placement.hwnd,
                    RECT {
                        left: CONTENT_PADDING + rect.left,
                        top: rect.top,
                        right: CONTENT_PADDING + rect.right,
                        bottom: rect.bottom,
                    },
                    state.dpi,
                );
            }
        }
        // The profile reorder rows, rail divider, and rail hint are dynamic
        // children of the Profiles card; they follow the card's scrolled rect.
        if state.view == View::Settings {
            if let Some((card, _)) = geometry
                .cards
                .iter()
                .find(|(_, key)| *key == TextKey::ProfilesCard)
            {
                sync_profile_order_rows(hwnd, state, *card);
            }
        }
        // Remember the laid-out Selection well bands so a selection change
        // can re-split target/context panes without recomputing geometry.
        if state.view == View::History {
            let band = |slot: Slot| {
                geometry.slots.iter().find_map(|(candidate, rect)| {
                    (*candidate == slot).then(|| {
                        scaled_rect(
                            RECT {
                                left: CONTENT_PADDING + rect.left,
                                top: rect.top,
                                right: CONTENT_PADDING + rect.right,
                                bottom: rect.bottom,
                            },
                            state.dpi,
                        )
                    })
                })
            };
            if let (Some(target), Some(context)) =
                (band(Slot::HistoryTarget), band(Slot::HistoryContext))
            {
                state.history_band_rects = Some((target, context));
            }
            apply_history_detail_split(state);
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

    /// Responsive geometry for one page. Everything is computed from the
    /// live container size so cards, fields, editors, and the history split
    /// stretch to use the whole window. Returned rects are logical units in
    /// page-container coordinates (content starts at PAGE_HEADER_HEIGHT).
    struct PageGeometry {
        cards: Vec<(RECT, TextKey)>,
        wells: Vec<RECT>,
        extra_caps: Vec<(RECT, TextKey)>,
        slots: Vec<(Slot, RECT)>,
        /// Logical pixels by which the page content exceeds the container
        /// height. Nonzero only on the Settings page, where the Profiles
        /// order card grows with the profile count; the page then scrolls.
        scroll_max: i32,
    }

    /// Evenly distribute `count` blocks of `block` logical pixels between
    /// `top` and `bottom`; leftover space becomes equal gaps, so growing a
    /// card breathes instead of leaving a dead strip at its bottom.
    fn distributed_tops(count: i32, top: i32, bottom: i32, block: i32) -> Vec<i32> {
        let count = count.max(1);
        let span = (bottom - top - block * count).max(0);
        let gap = span / (count + 1);
        (0..count)
            .map(|index| top + gap * (index + 1) + block * index)
            .collect()
    }

    fn page_geometry(
        view: View,
        page_width: i32,
        container_height: i32,
        profile_count: usize,
    ) -> PageGeometry {
        let content_width = (page_width - CONTENT_PADDING * 2).max(200);
        let content_top = PAGE_HEADER_HEIGHT;
        let content_height = (container_height - PAGE_HEADER_HEIGHT - 24).max(120);
        let mut slots: Vec<(Slot, RECT)> = Vec::new();
        let mut cards: Vec<(RECT, TextKey)> = Vec::new();
        let mut wells: Vec<RECT> = Vec::new();
        let mut extra_caps: Vec<(RECT, TextKey)> = Vec::new();
        let mut geometry_scroll_max = 0i32;
        let field_block = 44; // label 14 + 2 + input 28
        match view {
            View::Settings => {
                // Five cards: Provider, Credentials, Defaults, Hot key, and
                // the Profiles order card. The four field cards keep their
                // weights for spare vertical space; the Profiles card grows
                // with the profile count, so a tall stack overflows into a
                // page scrollbar instead of squeezing the fields.
                let row_h = 32; // reorder row: 30 + 2 gap
                let rows = profile_count as i32;
                let divider_h: i32 = if profile_count > RAIL_INLINE_LIMIT {
                    30
                } else {
                    0
                };
                let profiles_h = 28 + rows * row_h + divider_h + 14;
                let footnote_h = 16;
                let base = [184, 140, 184, 216];
                let weights = [3, 2, 3, 2];
                let min_stack = base[0] + base[1] + base[2] + base[3] + profiles_h + 4 * 10;
                let extra = (content_height - 46 - footnote_h - min_stack).max(0);
                let mut tops = [0, 0, 0, 0, 0];
                let mut heights = [0, 0, 0, 0, 0];
                let mut y = 0;
                for index in 0..5 {
                    heights[index] = if index < 4 {
                        base[index] + extra * weights[index] / 8
                    } else {
                        profiles_h
                    };
                    tops[index] = y;
                    y += heights[index] + 10;
                }
                let footnote_top = y - 10;
                let save_top = footnote_top + footnote_h + 8;
                // The scroll shift moves the whole geometry including the
                // header band, so the overflow is measured against the full
                // container height, not just the content area.
                let scroll_max = (content_top + save_top + 36 - container_height).max(0);
                let card_keys = [
                    TextKey::ProviderCard,
                    TextKey::CredentialsCard,
                    TextKey::DefaultsCard,
                    TextKey::HotkeyCard,
                    TextKey::ProfilesCard,
                ];
                for index in 0..5 {
                    cards.push((
                        RECT {
                            left: 0,
                            top: content_top + tops[index],
                            right: content_width,
                            bottom: content_top + tops[index] + heights[index],
                        },
                        card_keys[index],
                    ));
                }
                let inner_width = content_width - 24;
                // Field rows are computed in card-absolute coordinates so
                // every slot carries the same content_top offset as its card.
                let rows = distributed_tops(
                    3,
                    content_top + tops[0] + 28,
                    content_top + tops[0] + heights[0] - 12,
                    field_block,
                );
                for (label_slot, input_slot, row) in [
                    (Slot::EndpointLabel, Slot::Endpoint, rows[0]),
                    (Slot::ModelLabel, Slot::Model, rows[1]),
                    (Slot::CredentialTargetLabel, Slot::CredentialTarget, rows[2]),
                ] {
                    slots.push((
                        label_slot,
                        RECT {
                            left: 12,
                            top: row,
                            right: 12 + inner_width,
                            bottom: row + 14,
                        },
                    ));
                    slots.push((
                        input_slot,
                        RECT {
                            left: 12,
                            top: row + 16,
                            right: 12 + inner_width,
                            bottom: row + 44,
                        },
                    ));
                }
                // Credentials: key row, action row, hint. The rows have
                // different heights (field 44, buttons 28, hint 16), so they
                // distribute with their real blocks; uniform 44px blocks
                // would pool the hint's slack below it and stack the action
                // row directly against the field.
                let cred_blocks = [field_block, 28, 16];
                let cred_used: i32 = cred_blocks.iter().sum();
                let cred_top = content_top + tops[1] + 28;
                let cred_bottom = content_top + tops[1] + heights[1] - 12;
                let cred_gap = (cred_bottom - cred_top - cred_used).max(0) / 4;
                let mut cred_rows = [0i32; 3];
                let mut cred_offset = 0;
                for (index, block) in cred_blocks.iter().enumerate() {
                    cred_rows[index] = cred_top + cred_gap * (index as i32 + 1) + cred_offset;
                    cred_offset += block;
                }
                slots.push((
                    Slot::ApiKeyLabel,
                    RECT {
                        left: 12,
                        top: cred_rows[0],
                        right: 12 + inner_width,
                        bottom: cred_rows[0] + 14,
                    },
                ));
                slots.push((
                    Slot::ApiKey,
                    RECT {
                        left: 12,
                        top: cred_rows[0] + 16,
                        right: 12 + inner_width,
                        bottom: cred_rows[0] + 44,
                    },
                ));
                slots.push((
                    Slot::SaveKey,
                    RECT {
                        left: 12,
                        top: cred_rows[1],
                        right: 122,
                        bottom: cred_rows[1] + 28,
                    },
                ));
                slots.push((
                    Slot::DeleteKey,
                    RECT {
                        left: 132,
                        top: cred_rows[1],
                        right: 282,
                        bottom: cred_rows[1] + 28,
                    },
                ));
                slots.push((
                    Slot::CredentialStatus,
                    RECT {
                        left: 294,
                        top: cred_rows[1] + 4,
                        right: content_width - 12,
                        bottom: cred_rows[1] + 24,
                    },
                ));
                slots.push((
                    Slot::CredentialHint,
                    RECT {
                        left: 12,
                        top: cred_rows[2],
                        right: content_width - 12,
                        bottom: cred_rows[2] + 16,
                    },
                ));
                let rows = distributed_tops(
                    3,
                    content_top + tops[2] + 28,
                    content_top + tops[2] + heights[2] - 12,
                    field_block,
                );
                for (label_slot, input_slot, row) in [
                    (Slot::SelectionLabel, Slot::SelectionDefault, rows[0]),
                    (Slot::HoverLabel, Slot::HoverDefault, rows[1]),
                    (Slot::LangLabel, Slot::Language, rows[2]),
                ] {
                    slots.push((
                        label_slot,
                        RECT {
                            left: 12,
                            top: row,
                            right: 12 + inner_width,
                            bottom: row + 14,
                        },
                    ));
                    slots.push((
                        input_slot,
                        RECT {
                            left: 12,
                            top: row + 16,
                            right: 12 + inner_width,
                            bottom: row + 44,
                        },
                    ));
                }
                // Hot key: four optional fields in a 2×2 grid, matching the
                // mockup's two-column "fields2" arrangement. Empty input
                // means the hotkey is off (the edit's cue banner says so).
                let hot_rows = distributed_tops(
                    2,
                    content_top + tops[3] + 28,
                    content_top + tops[3] + heights[3] - 12,
                    field_block,
                );
                let half_width = (inner_width - 16) / 2;
                for (label_slot, input_slot, row, column) in [
                    (
                        Slot::HotkeySelectionLabel,
                        Slot::HotkeySelection,
                        hot_rows[0],
                        0,
                    ),
                    (Slot::HotkeyHoverLabel, Slot::HotkeyHover, hot_rows[0], 1),
                    (Slot::HotkeyPauseLabel, Slot::HotkeyPause, hot_rows[1], 0),
                    (
                        Slot::HotkeyOpenPopupLabel,
                        Slot::HotkeyOpenPopup,
                        hot_rows[1],
                        1,
                    ),
                ] {
                    let left = 12 + column * (half_width + 16);
                    slots.push((
                        label_slot,
                        RECT {
                            left,
                            top: row,
                            right: left + half_width,
                            bottom: row + 14,
                        },
                    ));
                    slots.push((
                        input_slot,
                        RECT {
                            left,
                            top: row + 16,
                            right: left + half_width,
                            bottom: row + 44,
                        },
                    ));
                }
                slots.push((
                    Slot::SaveSettings,
                    RECT {
                        left: 0,
                        top: content_top + save_top,
                        right: 160,
                        bottom: content_top + save_top + 36,
                    },
                ));
                geometry_scroll_max = scroll_max;
            }
            View::Prompts => {
                // Meta row: ID fixed, Name flexes, the numeric group stays
                // right-aligned; editors and the status row absorb the rest.
                let meta_label = content_top + 4;
                let meta_input = content_top + 20;
                let name_right = (content_width - 560).max(170) + 160;
                slots.push((
                    Slot::PromptIdLabel,
                    RECT {
                        left: 0,
                        top: meta_label,
                        right: 150,
                        bottom: meta_label + 14,
                    },
                ));
                slots.push((
                    Slot::PromptId,
                    RECT {
                        left: 0,
                        top: meta_input,
                        right: 150,
                        bottom: meta_input + 28,
                    },
                ));
                slots.push((
                    Slot::PromptNameLabel,
                    RECT {
                        left: 160,
                        top: meta_label,
                        right: name_right,
                        bottom: meta_label + 14,
                    },
                ));
                slots.push((
                    Slot::PromptName,
                    RECT {
                        left: 160,
                        top: meta_input,
                        right: name_right,
                        bottom: meta_input + 28,
                    },
                ));
                for (label_slot, input_slot, left, width) in [
                    (
                        Slot::PromptModelLabel,
                        Slot::PromptModel,
                        content_width - 350,
                        150,
                    ),
                    (
                        Slot::PromptTempLabel,
                        Slot::PromptTemp,
                        content_width - 190,
                        90,
                    ),
                    (
                        Slot::PromptTokensLabel,
                        Slot::PromptTokens,
                        content_width - 90,
                        90,
                    ),
                ] {
                    slots.push((
                        label_slot,
                        RECT {
                            left,
                            top: meta_label,
                            right: left + width,
                            bottom: meta_label + 14,
                        },
                    ));
                    slots.push((
                        input_slot,
                        RECT {
                            left,
                            top: meta_input,
                            right: left + width,
                            bottom: meta_input + 28,
                        },
                    ));
                }
                slots.push((
                    Slot::NewPrompt,
                    RECT {
                        left: content_width - 250,
                        top: content_top + 56,
                        right: content_width - 170,
                        bottom: content_top + 92,
                    },
                ));
                slots.push((
                    Slot::DeletePrompt,
                    RECT {
                        left: content_width - 420,
                        top: content_top + 56,
                        right: content_width - 260,
                        bottom: content_top + 92,
                    },
                ));
                slots.push((
                    Slot::SavePrompt,
                    RECT {
                        left: content_width - 160,
                        top: content_top + 56,
                        right: content_width,
                        bottom: content_top + 92,
                    },
                ));
                slots.push((
                    Slot::PromptHint,
                    RECT {
                        left: 0,
                        top: content_top + 148,
                        right: content_width,
                        bottom: content_top + 164,
                    },
                ));
                for (index, slot) in [
                    Slot::PromptModelHint,
                    Slot::PromptTempHint,
                    Slot::PromptTokensHint,
                ]
                .into_iter()
                .enumerate()
                {
                    let top = content_top + 100 + 16 * index as i32;
                    slots.push((
                        slot,
                        RECT {
                            left: 0,
                            top,
                            right: content_width,
                            bottom: top + 16,
                        },
                    ));
                }
                let editor_bottom = content_height - 24;
                let editor_mid = (content_width - 10) / 2;
                cards.push((
                    RECT {
                        left: 0,
                        top: content_top + 176,
                        right: editor_mid,
                        bottom: content_top + editor_bottom,
                    },
                    TextKey::SystemPrompt,
                ));
                cards.push((
                    RECT {
                        left: editor_mid + 10,
                        top: content_top + 176,
                        right: content_width,
                        bottom: content_top + editor_bottom,
                    },
                    TextKey::UserTemplate,
                ));
                slots.push((
                    Slot::SystemPrompt,
                    RECT {
                        left: 10,
                        top: content_top + 206,
                        right: editor_mid - 10,
                        bottom: content_top + editor_bottom - 10,
                    },
                ));
                slots.push((
                    Slot::UserTemplate,
                    RECT {
                        left: editor_mid + 20,
                        top: content_top + 206,
                        right: content_width - 10,
                        bottom: content_top + editor_bottom - 10,
                    },
                ));
                slots.push((
                    Slot::PromptStatus,
                    RECT {
                        left: 0,
                        top: content_top + content_height - 18,
                        right: content_width,
                        bottom: content_top + content_height - 2,
                    },
                ));
            }
            View::History => {
                // Search card.
                cards.push((
                    RECT {
                        left: 0,
                        top: content_top,
                        right: content_width,
                        bottom: content_top + 103,
                    },
                    TextKey::SearchCard,
                ));
                slots.push((
                    Slot::HistorySearch,
                    RECT {
                        left: 12,
                        top: content_top + 29,
                        right: content_width - 232,
                        bottom: content_top + 57,
                    },
                ));
                slots.push((
                    Slot::HistoryRefresh,
                    RECT {
                        left: content_width - 222,
                        top: content_top + 29,
                        right: content_width - 122,
                        bottom: content_top + 57,
                    },
                ));
                slots.push((
                    Slot::HistoryCopy,
                    RECT {
                        left: content_width - 112,
                        top: content_top + 29,
                        right: content_width - 4,
                        bottom: content_top + 57,
                    },
                ));
                let filter_width = (content_width - 24 - 20) / 3;
                for (index, slot) in [
                    Slot::HistPromptFilter,
                    Slot::HistSourceFilter,
                    Slot::HistOrderFilter,
                ]
                .into_iter()
                .enumerate()
                {
                    let left = 12 + index as i32 * (filter_width + 10);
                    slots.push((
                        slot,
                        RECT {
                            left,
                            top: content_top + 63,
                            right: left + filter_width,
                            bottom: content_top + 91,
                        },
                    ));
                }
                // Entries card: the list/detail split absorbs the rest.
                let card_top = content_top + 114;
                let card_bottom = content_top + content_height - 64;
                cards.push((
                    RECT {
                        left: 0,
                        top: card_top,
                        right: content_width,
                        bottom: card_bottom,
                    },
                    TextKey::EntriesCard,
                ));
                let list_width = 300;
                slots.push((
                    Slot::HistoryList,
                    RECT {
                        left: 12,
                        top: card_top + 25,
                        right: 12 + list_width,
                        bottom: card_bottom - 12,
                    },
                ));
                let detail_left = 12 + list_width + 10;
                let detail_right = content_width - 12;
                // Selection detail: caption, then one well holding the target
                // pane above the muted context pane. The well and its editors
                // span exactly the same x-range as the Output well below, so
                // both detail areas read as one aligned column.
                extra_caps.push((
                    RECT {
                        left: detail_left,
                        top: card_top + 25,
                        right: detail_right,
                        bottom: card_top + 38,
                    },
                    TextKey::Selection,
                ));
                wells.push(RECT {
                    left: detail_left,
                    top: card_top + 41,
                    right: detail_right,
                    bottom: card_top + 111,
                });
                slots.push((
                    Slot::HistoryTarget,
                    RECT {
                        left: detail_left,
                        top: card_top + 44,
                        right: detail_right,
                        // Upper band of the Selection well; the context pane
                        // takes the lower band. With no context the target
                        // editor expands over both bands at selection time.
                        bottom: card_top + 71,
                    },
                ));
                slots.push((
                    Slot::HistoryContext,
                    RECT {
                        left: detail_left,
                        top: card_top + 75,
                        right: detail_right,
                        bottom: card_top + 108,
                    },
                ));
                extra_caps.push((
                    RECT {
                        left: detail_left,
                        top: card_top + 117,
                        right: detail_right,
                        bottom: card_top + 130,
                    },
                    TextKey::Output,
                ));
                wells.push(RECT {
                    left: detail_left,
                    top: card_top + 132,
                    right: detail_right,
                    bottom: card_bottom - 24,
                });
                slots.push((
                    Slot::HistoryOutput,
                    RECT {
                        left: detail_left,
                        top: card_top + 135,
                        right: detail_right,
                        bottom: card_bottom - 27,
                    },
                ));
                slots.push((
                    Slot::HistoryHint,
                    RECT {
                        left: detail_left,
                        top: card_bottom - 22,
                        right: detail_right,
                        bottom: card_bottom - 8,
                    },
                ));
                slots.push((
                    Slot::HistoryMeta,
                    RECT {
                        left: 0,
                        top: card_bottom + 4,
                        right: 300,
                        bottom: card_bottom + 24,
                    },
                ));
                slots.push((
                    Slot::HistoryDelete,
                    RECT {
                        left: content_width - 144,
                        top: card_bottom + 2,
                        right: content_width,
                        bottom: card_bottom + 34,
                    },
                ));
            }
        }
        PageGeometry {
            cards,
            wells,
            extra_caps,
            slots,
            scroll_max: geometry_scroll_max,
        }
    }

    /// Shift every rect in a geometry up by `dy` logical pixels. Used to
    /// scroll the Settings page: geometry is shared by the painter and the
    /// layout, so one shift moves the cards, slots, and dynamic rows alike.
    fn shift_geometry(geometry: &mut PageGeometry, dy: i32) {
        let shift = |rect: &mut RECT| {
            rect.top -= dy;
            rect.bottom -= dy;
        };
        for (rect, _) in &mut geometry.cards {
            shift(rect);
        }
        for rect in &mut geometry.wells {
            shift(rect);
        }
        for (rect, _) in &mut geometry.extra_caps {
            shift(rect);
        }
        for (_, rect) in &mut geometry.slots {
            shift(rect);
        }
    }

    /// Reflect the Settings scroll state in the page container's vertical
    /// scrollbar. The scrollbar is shown only when content overflows.
    fn update_settings_scrollbar(_parent: HWND, state: &ManagerState) {
        let page = state.handles.settings_page;
        if state.view != View::Settings || state.settings_scroll_max == 0 {
            unsafe {
                let _ = ShowScrollBar(page, SB_VERT, false);
            }
            return;
        }
        let mut client = RECT::default();
        if unsafe { GetClientRect(page, &mut client) }.is_err() {
            return;
        }
        let page_height = unscale_from_dpi(client.bottom - client.top, state.dpi).max(1);
        let info = SCROLLINFO {
            cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
            fMask: SIF_RANGE | SIF_POS | SIF_ALL,
            nMin: 0,
            nMax: state.settings_scroll_max + page_height - 1,
            nPage: page_height as u32,
            nPos: state.settings_scroll,
            nTrackPos: 0,
        };
        unsafe {
            SetScrollInfo(page, SB_VERT, &info, true);
            let _ = ShowScrollBar(page, SB_VERT, true);
        }
    }

    /// Handle a vertical scroll gesture aimed at the Settings page.
    fn handle_settings_scroll(parent: HWND, state: &mut ManagerState, wparam: WPARAM) {
        if state.view != View::Settings || state.settings_scroll_max == 0 {
            return;
        }
        let request =
            windows::Win32::UI::WindowsAndMessaging::SCROLLBAR_COMMAND((wparam.0 & 0xffff) as i32);
        let step = |lines: i32| lines * 32;
        let mut next = state.settings_scroll;
        match request {
            SB_LINEUP => next -= step(1),
            SB_LINEDOWN => next += step(1),
            SB_PAGEUP => next -= step(6),
            SB_PAGEDOWN => next += step(6),
            SB_THUMBPOSITION => {
                // The high word of wParam carries the absolute position.
                next = ((wparam.0 >> 16) as u16) as i32;
            }
            _ => return,
        }
        state.settings_scroll = next.clamp(0, state.settings_scroll_max);
        unsafe {
            let _ = SetScrollInfo(
                state.handles.settings_page,
                SB_VERT,
                &SCROLLINFO {
                    cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
                    fMask: SIF_POS,
                    nMin: 0,
                    nMax: 0,
                    nPage: 0,
                    nPos: state.settings_scroll,
                    nTrackPos: 0,
                },
                true,
            );
        }
        apply_manager_layout(parent, state);
    }

    /// Reconcile the dynamic profile reorder rows with the profile count,
    /// then position them inside the (scrolled) Profiles card. Row labels
    /// and button enable states always refresh: a reorder only swaps two
    /// labels, it never recreates the rows.
    fn sync_profile_order_rows(parent: HWND, state: &mut ManagerState, card: RECT) {
        let page = state.handles.settings_page;
        let count = state.config.profiles.len();
        if state.handles.profile_rows.len() != count {
            let mut dead: Vec<HWND> = Vec::new();
            for row in state.handles.profile_rows.drain(..) {
                for hwnd in [row.label, row.up, row.down] {
                    unsafe {
                        let _ = DestroyWindow(hwnd);
                    }
                    dead.push(hwnd);
                }
            }
            state
                .handles
                .placements
                .retain(|placement| !dead.contains(&placement.hwnd));
            for index in 0..count.min(ID_PROFILE_MOVE_SLOTS / 2) {
                let label = create_profile_row_control(page, "", ID_PROFILE_MOVE_BASE, true);
                let up = create_profile_row_control(
                    page,
                    "\u{2191}",
                    ID_PROFILE_MOVE_BASE + index * 2,
                    false,
                );
                let down = create_profile_row_control(
                    page,
                    "\u{2193}",
                    ID_PROFILE_MOVE_BASE + index * 2 + 1,
                    false,
                );
                let label = label.unwrap_or_default();
                let up = up.unwrap_or_default();
                let down = down.unwrap_or_default();
                for hwnd in [label, up, down] {
                    state.handles.placements.push(ControlPlacement {
                        hwnd,
                        view: View::Settings,
                        slot: Slot::ProfilesDynamic,
                    });
                }
                state
                    .handles
                    .profile_rows
                    .push(ProfileOrderRow { label, up, down });
            }
            // The divider and hint ride along with the rows; they are created
            // once and only shown when the stack actually folds profiles.
            if state.handles.profiles_divider.0.is_null() {
                state.handles.profiles_divider = create_profile_row_control(
                    page,
                    ui_text(state.language(), TextKey::ProfilesBeneathDivider),
                    ID_PROFILE_MOVE_BASE,
                    true,
                )
                .unwrap_or_default();
                if !state.handles.profiles_divider.0.is_null() {
                    state.handles.placements.push(ControlPlacement {
                        hwnd: state.handles.profiles_divider,
                        view: View::Settings,
                        slot: Slot::ProfilesDynamic,
                    });
                    state.handles.localized.push(LocalizedControl {
                        hwnd: state.handles.profiles_divider,
                        key: TextKey::ProfilesBeneathDivider,
                    });
                }
            }
            if state.handles.profiles_hint.0.is_null() {
                state.handles.profiles_hint = create_profile_row_control(
                    page,
                    ui_text(state.language(), TextKey::ProfilesRailHint),
                    ID_PROFILE_MOVE_BASE,
                    true,
                )
                .unwrap_or_default();
                if !state.handles.profiles_hint.0.is_null() {
                    state.handles.placements.push(ControlPlacement {
                        hwnd: state.handles.profiles_hint,
                        view: View::Settings,
                        slot: Slot::ProfilesDynamic,
                    });
                    state.handles.localized.push(LocalizedControl {
                        hwnd: state.handles.profiles_hint,
                        key: TextKey::ProfilesRailHint,
                    });
                }
            }
            // Controls created after startup missed the initial font pass:
            // without it they render in the tiny system font.
            apply_control_fonts(state);
        }
        // Position and label every row. The card rect is in page-content
        // coordinates like every slot, so it carries the same left padding;
        // rows below the rail limit leave room for the "beneath More…"
        // divider instead of sitting under it.
        let row_h = 32;
        let divider_gap = 26;
        let card = RECT {
            left: card.left + CONTENT_PADDING,
            ..card
        };
        for (index, row) in state.handles.profile_rows.iter().enumerate() {
            let folded = if index >= RAIL_INLINE_LIMIT {
                divider_gap
            } else {
                0
            };
            let top = card.top + 30 + (index as i32) * row_h + folded;
            let label_rect = scaled_rect(
                RECT {
                    left: card.left + 14,
                    top,
                    right: card.right - 16 - 2 * 30,
                    bottom: top + 30,
                },
                state.dpi,
            );
            let up_rect = scaled_rect(
                RECT {
                    left: card.right - 16 - 2 * 30,
                    top: top + 2,
                    right: card.right - 16 - 30,
                    bottom: top + 28,
                },
                state.dpi,
            );
            let down_rect = scaled_rect(
                RECT {
                    left: card.right - 16 - 26,
                    top: top + 2,
                    right: card.right - 16,
                    bottom: top + 28,
                },
                state.dpi,
            );
            unsafe {
                let _ = SetWindowPos(
                    row.label,
                    None,
                    label_rect.left,
                    label_rect.top,
                    label_rect.right - label_rect.left,
                    label_rect.bottom - label_rect.top,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
                let _ = SetWindowPos(
                    row.up,
                    None,
                    up_rect.left,
                    up_rect.top,
                    up_rect.right - up_rect.left,
                    up_rect.bottom - up_rect.top,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
                let _ = SetWindowPos(
                    row.down,
                    None,
                    down_rect.left,
                    down_rect.top,
                    down_rect.right - down_rect.left,
                    down_rect.bottom - down_rect.top,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
            }
            relabel_profile_row(state, index);
        }
        // Divider after the inline limit; hint below the card.
        if !state.handles.profiles_divider.0.is_null() {
            let show = count > RAIL_INLINE_LIMIT;
            let divider_top = card.top + 30 + (RAIL_INLINE_LIMIT as i32) * row_h + 2;
            let rect = scaled_rect(
                RECT {
                    left: card.left + 14,
                    top: divider_top,
                    right: card.right - 14,
                    bottom: divider_top + 20,
                },
                state.dpi,
            );
            unsafe {
                let _ = SetWindowPos(
                    state.handles.profiles_divider,
                    None,
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
                let _ = ShowWindow(
                    state.handles.profiles_divider,
                    if show { SW_SHOW } else { SW_HIDE },
                );
            }
        }
        if !state.handles.profiles_hint.0.is_null() {
            let hint_top = card.bottom + 6;
            let rect = scaled_rect(
                RECT {
                    left: card.left + 2,
                    top: hint_top,
                    right: card.right,
                    bottom: hint_top + 18,
                },
                state.dpi,
            );
            unsafe {
                let _ = SetWindowPos(
                    state.handles.profiles_hint,
                    None,
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
            }
        }
        let _ = parent;
    }

    fn relabel_profile_row(state: &ManagerState, index: usize) {
        let Some(row) = state.handles.profile_rows.get(index) else {
            return;
        };
        let Some(profile) = state.config.profiles.get(index) else {
            return;
        };
        set_text(row.label, profile_display_name(profile));
        unsafe {
            let _ = EnableWindow(row.up, index > 0);
            let _ = EnableWindow(row.down, index + 1 < state.config.profiles.len());
        }
    }

    /// Create one dynamic Settings-page control. Statics carry the row
    /// labels, divider, and hint; buttons are owner-drawn so they share the
    /// manager's raised button treatment.
    fn create_profile_row_control(
        parent: HWND,
        text: &str,
        id: usize,
        is_static: bool,
    ) -> windows::core::Result<HWND> {
        if is_static {
            // Single-line labels: vertically centred in their row (the row
            // is taller than the text, beside the arrow buttons) and
            // ellipsised instead of clipped.
            create_control(
                parent,
                w!("STATIC"),
                text,
                WS_CHILD | WS_VISIBLE | WINDOW_STYLE(0x0200 | 0x4000),
                0,
                0,
                100,
                24,
                id,
            )
        } else {
            create_control(
                parent,
                w!("BUTTON"),
                text,
                WS_CHILD
                    | WS_VISIBLE
                    | WS_TABSTOP
                    | WINDOW_STYLE(BS_PUSHBUTTON as u32 | BS_OWNERDRAW as u32),
                0,
                0,
                26,
                24,
                id,
            )
        }
    }

    /// Swap a profile with its neighbour and persist immediately: the order
    /// is the chooser rail contract, so it must survive without an extra
    /// Save click.
    fn move_profile_order(parent: HWND, state: &mut ManagerState, index: usize, up: bool) {
        let target = if up {
            match index.checked_sub(1) {
                Some(value) => value,
                None => return,
            }
        } else if index + 1 < state.config.profiles.len() {
            index + 1
        } else {
            return;
        };
        state.config.profiles.swap(index, target);
        if let Err(error) = save_config(state, &state.config) {
            set_status(
                state,
                &status_text(
                    state.language(),
                    StatusEvent::CannotSaveSettings { detail: &error },
                ),
            );
            return;
        }
        let refresh = notify_config_changed();
        populate_profile_selectors(state);
        set_status(state, &config_refresh_status(state.language(), refresh));
        apply_manager_layout(parent, state);
    }

    fn apply_control_fonts(state: &ManagerState) {
        let body = state.theme.body_font;
        let small = state.theme.small_font;
        let note = state.theme.note_font;
        let mono = state.theme.mono_font;
        let set_font = |hwnd: HWND, font: HFONT| unsafe {
            let _ = SendMessageW(
                hwnd,
                WM_SETFONT,
                Some(WPARAM(font.0 as usize)),
                Some(LPARAM(1)),
            );
        };
        for hwnd in [
            state.handles.settings_nav,
            state.handles.prompts_nav,
            state.handles.history_nav,
        ] {
            set_font(hwnd, body);
        }
        set_font(state.handles.status, note);
        for placement in &state.handles.placements {
            if placement.hwnd == state.handles.profiles_divider
                || placement.hwnd == state.handles.profiles_hint
            {
                set_font(placement.hwnd, note);
                continue;
            }
            let font = match placement.slot {
                // History detail shares the popup's treatment: mono target
                // pane, body context pane, mono output.
                Slot::SystemPrompt | Slot::UserTemplate | Slot::HistoryOutput => mono,
                Slot::HistoryTarget => mono,
                Slot::HistoryContext => note,
                Slot::CredentialHint
                | Slot::PromptHint
                | Slot::PromptModelHint
                | Slot::PromptTempHint
                | Slot::PromptTokensHint
                | Slot::HistoryHint
                | Slot::HistoryMeta
                | Slot::CredentialStatus
                | Slot::PromptStatus => note,
                Slot::EndpointLabel
                | Slot::ModelLabel
                | Slot::CredentialTargetLabel
                | Slot::ApiKeyLabel
                | Slot::SelectionLabel
                | Slot::HoverLabel
                | Slot::LangLabel
                | Slot::HotkeySelectionLabel
                | Slot::HotkeyHoverLabel
                | Slot::HotkeyPauseLabel
                | Slot::HotkeyOpenPopupLabel
                | Slot::PromptIdLabel
                | Slot::PromptNameLabel
                | Slot::PromptModelLabel
                | Slot::PromptTempLabel
                | Slot::PromptTokensLabel => small,
                _ => body,
            };
            set_font(placement.hwnd, font);
        }
        // WM_SETFONT resets RichEdit character formatting: re-ink the
        // markdown-aware fields after every font application.
        for rich in [
            state.handles.system_prompt,
            state.handles.user_template,
            state.handles.history_output,
        ] {
            apply_rich_ink(rich);
        }
    }

    fn apply_control_themes(state: &ManagerState) {
        for hwnd in [
            state.handles.settings_nav,
            state.handles.prompts_nav,
            state.handles.history_nav,
            state.handles.status,
        ]
        .into_iter()
        .chain(state.handles.placements.iter().map(|item| item.hwnd))
        {
            unsafe {
                let _ = SetWindowTheme(hwnd, w!("DarkMode_Explorer"), PCWSTR::null());
            }
        }
        // COMBOBOX does not inherit the dark palette from its parent.  Apply the dark Explorer
        // part explicitly to both the closed control and its lazily-created drop-down list; the
        // latter is why merely handling WM_CTLCOLOR* still leaves a light arrow/list on some
        // Windows builds.
        for combo in [
            state.handles.language,
            state.handles.profile_id,
            state.handles.history_prompt,
            state.handles.history_source,
            state.handles.history_order,
        ] {
            unsafe {
                let _ = SetWindowTheme(combo, w!("DarkMode_Explorer"), PCWSTR::null());
                let _ = InvalidateRect(Some(combo), None, true);
            }
        }
    }

    fn paint_manager(hwnd: HWND, state: &ManagerState) {
        let mut paint = windows::Win32::Graphics::Gdi::PAINTSTRUCT::default();
        let hdc = unsafe { BeginPaint(hwnd, &mut paint) };
        let mut client = RECT::default();
        if unsafe { GetClientRect(hwnd, &mut client) }.is_ok() {
            unsafe {
                let _ = FillRect(hdc, &client, state.theme.background);
            }
            // Tab bar: surface strip with a hairline bottom border; the live
            // status dot precedes the status label on the right.
            let layout = ManagerLayout::for_client(client.right, client.bottom, state.dpi);
            let tab_bar = scaled_rect(layout.tab_bar, state.dpi);
            unsafe {
                let _ = FillRect(hdc, &tab_bar, state.theme.surface);
                let hairline = CreateSolidBrush(BORDER);
                let _ = FillRect(
                    hdc,
                    &RECT {
                        left: tab_bar.left,
                        top: tab_bar.bottom - 1,
                        right: tab_bar.right,
                        bottom: tab_bar.bottom,
                    },
                    hairline,
                );
                let _ = DeleteObject(hairline.into());
                // Live dot: 8px ok-green circle left of the status text.
                let status = scaled_rect(layout.status, state.dpi);
                let dot_radius = scale_for_dpi(4, state.dpi).max(2);
                let dot_center_y = (status.top + status.bottom) / 2;
                let brush = CreateSolidBrush(OK);
                let pen = CreatePen(PS_NULL, 0, COLORREF(0));
                let old_brush = SelectObject(hdc, HGDIOBJ(brush.0));
                let old_pen = SelectObject(hdc, HGDIOBJ(pen.0));
                let _ = Ellipse(
                    hdc,
                    status.left - dot_radius * 3,
                    dot_center_y - dot_radius,
                    status.left - dot_radius,
                    dot_center_y + dot_radius,
                );
                let _ = SelectObject(hdc, old_brush);
                let _ = SelectObject(hdc, old_pen);
                let _ = DeleteObject(brush.into());
                let _ = DeleteObject(pen.into());
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
        let mut paint = windows::Win32::Graphics::Gdi::PAINTSTRUCT::default();
        let hdc = unsafe { BeginPaint(hwnd, &mut paint) };
        let mut client = RECT::default();
        if unsafe { GetClientRect(hwnd, &mut client) }.is_ok() {
            unsafe {
                let _ = FillRect(hdc, &client, state.theme.background);
            }
            let Some(view) = page_identity(state, hwnd) else {
                unsafe {
                    let _ = EndPaint(hwnd, &paint);
                }
                return;
            };
            // Responsive geometry shared with apply_manager_layout.
            let page_width = unscale_from_dpi(client.right, state.dpi);
            let container_height = unscale_from_dpi(client.bottom, state.dpi);
            let mut geometry = page_geometry(
                view,
                page_width,
                container_height,
                state.config.profiles.len(),
            );
            if view == View::Settings && state.settings_scroll > 0 {
                shift_geometry(&mut geometry, state.settings_scroll);
            }
            // Slot rects are logical container coordinates with the content
            // padding already accounted for; scale exactly once here.
            let place = |rect: &RECT| {
                scaled_rect(
                    RECT {
                        left: CONTENT_PADDING + rect.left,
                        top: rect.top,
                        right: CONTENT_PADDING + rect.right,
                        bottom: rect.bottom,
                    },
                    state.dpi,
                )
            };
            let (title_key, subtitle_key) = match view {
                View::Settings => (TextKey::Settings, TextKey::SettingsSubtitle),
                View::Prompts => (TextKey::Prompts, TextKey::PromptsSubtitle),
                View::History => (TextKey::History, TextKey::HistorySubtitle),
            };
            let title = ui_text(state.language(), title_key);
            let subtitle = ui_text(state.language(), subtitle_key);
            // Title and subtitle live in the header band at the top of the
            // page container; content coordinates start below it.
            let mut title_rect = place(&RECT {
                left: 0,
                top: 4,
                right: page_width - CONTENT_PADDING,
                bottom: 38,
            });
            let mut subtitle_rect = place(&RECT {
                left: 2,
                top: 38,
                right: page_width - CONTENT_PADDING,
                bottom: 58,
            });
            unsafe {
                let old = SelectObject(hdc, HGDIOBJ(state.theme.title_font.0));
                SetBkColor(hdc, MANAGER_BG);
                SetTextColor(hdc, TEXT);
                let mut text: Vec<u16> = title.encode_utf16().collect();
                let _ = DrawTextW(hdc, &mut text, &mut title_rect, DRAW_TEXT_FORMAT(0x0100));
                let _ = SelectObject(hdc, HGDIOBJ(state.theme.body_font.0));
                SetTextColor(hdc, MUTED);
                let mut text: Vec<u16> = subtitle.encode_utf16().collect();
                let _ = DrawTextW(hdc, &mut text, &mut subtitle_rect, DRAW_TEXT_FORMAT(0x0100));
                let _ = SelectObject(hdc, old);
            }

            // Grouped cards: raised rounded rectangles behind each field group.
            for (rect, cap_key) in &geometry.cards {
                let card = place(rect);
                paint_rounded_card(hdc, card, state.dpi);
                let cap = ui_text(state.language(), *cap_key);
                let mut cap_rect = place(&RECT {
                    left: rect.left + 12,
                    top: rect.top + 9,
                    right: rect.right - 12,
                    bottom: rect.top + 23,
                });
                unsafe {
                    let old = SelectObject(hdc, HGDIOBJ(state.theme.caps_font.0));
                    SetBkMode(hdc, TRANSPARENT);
                    SetTextColor(hdc, MUTED);
                    let mut text: Vec<u16> = cap.encode_utf16().collect();
                    let _ = DrawTextW(hdc, &mut text, &mut cap_rect, DRAW_TEXT_FORMAT(0x0100));
                    let _ = SelectObject(hdc, old);
                }
            }

            // History detail wells: the Selection card and Output box sit on
            // --void inside the raised Entries card, matching the mockup.
            for well in &geometry.wells {
                paint_well(hdc, place(well));
            }
            for (rect, cap_key) in &geometry.extra_caps {
                let cap = ui_text(state.language(), *cap_key);
                let mut cap_rect = place(rect);
                unsafe {
                    let old = SelectObject(hdc, HGDIOBJ(state.theme.caps_font.0));
                    SetBkMode(hdc, TRANSPARENT);
                    SetTextColor(hdc, MUTED);
                    let mut text: Vec<u16> = cap.encode_utf16().collect();
                    let _ = DrawTextW(hdc, &mut text, &mut cap_rect, DRAW_TEXT_FORMAT(0x0100));
                    let _ = SelectObject(hdc, old);
                }
            }
        }
        unsafe {
            let _ = EndPaint(hwnd, &paint);
        }
    }

    /// Raised card with hairline border, radius 10 (mockup `.mfield`).
    fn paint_rounded_card(hdc: HDC, rect: RECT, dpi: u32) {
        let radius = scale_for_dpi(10, dpi).max(4);
        let fill = unsafe { CreateSolidBrush(SURFACE_BG) };
        let border = unsafe { CreateSolidBrush(BORDER) };
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
                let _ = FillRgn(hdc, region, fill);
                let _ = FrameRgn(hdc, region, border, 1, 1);
                let _ = DeleteObject(region.into());
            } else {
                let _ = FillRect(hdc, &rect, fill);
            }
            let _ = DeleteObject(fill.into());
            let _ = DeleteObject(border.into());
        }
    }

    /// Void well with hairline border (history Selection/Output areas).
    fn paint_well(hdc: HDC, rect: RECT) {
        let fill = unsafe { CreateSolidBrush(MANAGER_BG) };
        let border = unsafe { CreateSolidBrush(BORDER) };
        unsafe {
            let _ = FillRect(hdc, &rect, fill);
            let _ = FrameRect(hdc, &rect, border);
            let _ = DeleteObject(fill.into());
            let _ = DeleteObject(border.into());
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
        let is_static = message == 0x0138;
        let on_void = child == state.handles.history_list
            || child == state.handles.history_target
            || child == state.handles.history_context
            || child == state.handles.history_output
            || child == state.handles.system_prompt
            || child == state.handles.user_template;
        // Settings-page static labels and notes sit on the raised card body;
        // prompts/history statics sit on the page background.
        let on_card = is_static
            && !on_void
            && state
                .handles
                .placements
                .iter()
                .any(|placement| placement.hwnd == child && placement.view == View::Settings);
        let (background, foreground) = if child == state.handles.status {
            (TAB_BG, MUTED)
        } else if child == state.handles.profiles_hint {
            // The rail hint sits below the Profiles card, on the page.
            (MANAGER_BG, MUTED)
        } else if on_void {
            (
                MANAGER_BG,
                if child == state.handles.history_context {
                    MUTED
                } else {
                    TEXT
                },
            )
        } else if on_card {
            (SURFACE_BG, MUTED)
        } else if is_static {
            (MANAGER_BG, MUTED)
        } else {
            (SURFACE_BG, TEXT)
        };
        unsafe {
            SetTextColor(hdc, foreground);
            SetBkColor(hdc, background);
        }
        LRESULT(if background == MANAGER_BG {
            state.theme.background.0 as isize
        } else if background == SURFACE_BG {
            state.theme.card.0 as isize
        } else {
            state.theme.surface.0 as isize
        })
    }

    fn draw_manager_button(item: &DRAWITEMSTRUCT, state: &ManagerState) {
        if item.CtlType != ODT_BUTTON {
            return;
        }
        let pressed = item.itemState.0 & 0x0001 != 0;
        let disabled = item.itemState.0 & 0x0004 != 0;
        let focused = item.itemState.0 & 0x0010 != 0;
        let selected_nav = matches!(
            (item.CtlID as usize, state.view),
            (ID_SETTINGS_TAB, View::Settings)
                | (ID_PROMPTS_TAB, View::Prompts)
                | (ID_HISTORY_TAB, View::History)
        );
        let destructive = matches!(
            item.CtlID as usize,
            ID_DELETE_KEY | ID_HISTORY_DELETE | ID_DELETE_PROMPT
        );
        let primary = matches!(
            item.CtlID as usize,
            ID_SAVE_SETTINGS | ID_SAVE_KEY | ID_SAVE_PROMPT | ID_HISTORY_REFRESH
        );
        // Mockup treatments: tabs are accent-tinted when active; primary
        // actions use the accent fill; destructive actions use the error
        // wash; everything else is raised with a hairline border.
        let (fill, border_color, text_color, _weight) = if selected_nav {
            (TAB_HOT, TAB_HOT, ACCENT, 600)
        } else if primary {
            let fill = if disabled { SURFACE_HOVER } else { ACCENT };
            (fill, fill, PRIMARY_INK, 600)
        } else if destructive {
            let border = if disabled { BORDER } else { DANGER_BORDER };
            (DANGER_BG, border, DANGER, 500)
        } else if disabled {
            (MANAGER_BG, BORDER, MUTED, 500)
        } else if pressed || focused {
            (SURFACE_HOVER, BORDER, TEXT, 500)
        } else {
            (SURFACE_BG, BORDER, TEXT, 500)
        };
        let fill_brush = unsafe { CreateSolidBrush(fill) };
        let border_brush = unsafe { CreateSolidBrush(border_color) };
        let mut rect = item.rcItem;
        let radius = scale_for_dpi(8, state.dpi).max(2);
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
                if border_color != fill {
                    let _ = FrameRgn(item.hDC, region, border_brush, 1, 1);
                }
                let _ = DeleteObject(region.into());
            } else {
                let _ = FillRect(item.hDC, &rect, fill_brush);
            }
            let _ = DeleteObject(fill_brush.into());
            let _ = DeleteObject(border_brush.into());
            let length = GetWindowTextLengthW(item.hwndItem).max(0) as usize;
            let mut text = vec![0u16; length + 1];
            let written = GetWindowTextW(item.hwndItem, &mut text).max(0) as usize;
            let font = SendMessageW(item.hwndItem, 0x0031, Some(WPARAM(0)), Some(LPARAM(0)));
            let old = if font.0 != 0 {
                Some(SelectObject(item.hDC, HGDIOBJ(font.0 as *mut _)))
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
                // DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX
                DRAW_TEXT_FORMAT(0x0001 | 0x0004 | 0x0020 | 0x0100 | 0x0800),
            );
            if focused {
                rect.left += scale_for_dpi(4, state.dpi);
                rect.top += scale_for_dpi(4, state.dpi);
                rect.right -= scale_for_dpi(4, state.dpi);
                rect.bottom -= scale_for_dpi(4, state.dpi);
                let _ = DrawFocusRect(item.hDC, &rect);
            }
            if let Some(old) = old {
                let _ = SelectObject(item.hDC, old);
            }
        }
    }

    /// Owner-draw row for the history list: target (mono), output preview,
    /// and a `time · profile` meta line — the mockup's three-line item.
    fn draw_history_list_item(item: &DRAWITEMSTRUCT, state: &ManagerState) {
        if item.CtlType != ODT_LISTBOX {
            return;
        }
        let selected = item.itemState.0 & 0x0001 != 0;
        let background = if selected { TAB_HOT } else { MANAGER_BG };
        let fill = unsafe { CreateSolidBrush(background) };
        unsafe {
            let _ = FillRect(item.hDC, &item.rcItem, fill);
            let _ = DeleteObject(fill.into());
            if (item.itemID as usize) >= state.history_entries.len() {
                return;
            }
            let entry = &state.history_entries[item.itemID as usize];
            let inset = scale_for_dpi(12, state.dpi);
            let mut bounds = item.rcItem;
            bounds.left += inset;
            bounds.right -= inset;
            let line_gap = scale_for_dpi(2, state.dpi);
            let mut target_rect = bounds;
            target_rect.top = bounds.top + scale_for_dpi(8, state.dpi);
            target_rect.bottom = target_rect.top + scale_for_dpi(16, state.dpi);
            let mut preview_rect = bounds;
            preview_rect.top = target_rect.bottom + line_gap;
            preview_rect.bottom = preview_rect.top + scale_for_dpi(15, state.dpi);
            let mut meta_rect = bounds;
            meta_rect.top = preview_rect.bottom + line_gap;
            meta_rect.bottom = meta_rect.top + scale_for_dpi(13, state.dpi);
            let old_bk = SetBkMode(item.hDC, TRANSPARENT);
            // 1 · Target snippet, mono ink.
            let mono_old = SelectObject(item.hDC, HGDIOBJ(state.theme.mono_font.0));
            SetTextColor(item.hDC, TEXT);
            let mut snippet: Vec<u16> = single_line(&entry.target).encode_utf16().collect();
            let _ = DrawTextW(
                item.hDC,
                &mut snippet,
                &mut target_rect,
                DRAW_TEXT_FORMAT(0x0100 | 0x0800 | 0x2000),
            );
            // 2 · Output preview, muted body face.
            SelectObject(item.hDC, HGDIOBJ(state.theme.note_font.0));
            SetTextColor(item.hDC, MUTED);
            let mut preview: Vec<u16> = single_line(&entry.output).encode_utf16().collect();
            let _ = DrawTextW(
                item.hDC,
                &mut preview,
                &mut preview_rect,
                DRAW_TEXT_FORMAT(0x0100 | 0x0800 | 0x2000),
            );
            // 3 · `HH:MM · profile` meta line; the profile id takes accent.
            let mono_old2 = SelectObject(item.hDC, HGDIOBJ(state.theme.mono_font.0));
            SetTextColor(item.hDC, MUTED);
            let time = entry
                .created_at_utc
                .get(11..16)
                .unwrap_or_default()
                .to_owned();
            let mut meta_text: Vec<u16> = format!("{time} · ").encode_utf16().collect();
            let _ = DrawTextW(
                item.hDC,
                &mut meta_text,
                &mut meta_rect,
                DRAW_TEXT_FORMAT(0x0100 | 0x0800),
            );
            let prefix_width = {
                let mut size = windows::Win32::Foundation::SIZE::default();
                let _ = GetTextExtentPoint32W(item.hDC, &meta_text, &mut size);
                size.cx
            };
            meta_rect.left += prefix_width;
            SetTextColor(item.hDC, ACCENT);
            let mut profile_text: Vec<u16> = single_line(&history_profile_label(
                &state.config.profiles,
                &entry.prompt_id,
                ui_text(state.language(), TextKey::DeletedProfile),
            ))
            .encode_utf16()
            .collect();
            let _ = DrawTextW(
                item.hDC,
                &mut profile_text,
                &mut meta_rect,
                DRAW_TEXT_FORMAT(0x0100 | 0x0800 | 0x2000),
            );
            let _ = SelectObject(item.hDC, mono_old);
            let _ = SelectObject(item.hDC, mono_old2);
            SetBkMode(item.hDC, BACKGROUND_MODE(old_bk as u32));
            if selected && (item.itemState.0 & 0x0010 == 0) {
                let _ = DrawFocusRect(item.hDC, &item.rcItem);
            }
        }
    }

    /// Page children are visible within their initially hidden container.
    /// Exactly one container is shown by `show_view` after initialization.
    fn visibility_style(view: Option<View>) -> WINDOW_STYLE {
        let _ = view;
        WS_VISIBLE
    }

    /// Give the dropped list room. The responsive layout sizes a combobox to
    /// its closed selection-field height, and Windows derives the dropped-list
    /// height from the window height, so without this the list opens as a
    /// two-pixel sliver. CBN_DROPDOWN arrives before the list is measured, so
    /// growing the window here is reflected in the open list.
    fn expand_combo_for_dropdown(state: &mut ManagerState, control: HWND) {
        if control.0.is_null() {
            return;
        }
        unsafe {
            let mut rect = RECT::default();
            if GetWindowRect(control, &mut rect).is_err() {
                return;
            }
            let compact = rect.bottom - rect.top;
            let key = GetDlgCtrlID(control);
            if !state
                .combo_compact_heights
                .iter()
                .any(|(existing, _)| *existing == key)
            {
                state.combo_compact_heights.push((key, compact));
            }
            let items = SendMessageW(control, CB_GETCOUNT, Some(WPARAM(0)), Some(LPARAM(0))).0;
            let item_height =
                SendMessageW(control, CB_GETITEMHEIGHT, Some(WPARAM(0)), Some(LPARAM(0))).0 as i32;
            if item_height <= 0 {
                return;
            }
            let visible = items.clamp(1, 8) as i32;
            let target = compact + item_height * visible + scale_for_dpi(4, state.dpi);
            let _ = SetWindowPos(
                control,
                None,
                0,
                0,
                (rect.right - rect.left).max(1),
                target,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }

    /// Restore the compact closed height once the dropdown list is gone.
    fn restore_combo_after_dropdown(state: &mut ManagerState, control: HWND) {
        if control.0.is_null() {
            return;
        }
        let key = unsafe { GetDlgCtrlID(control) };
        if let Some(position) = state
            .combo_compact_heights
            .iter()
            .position(|(existing, _)| *existing == key)
        {
            let (_, compact) = state.combo_compact_heights.remove(position);
            unsafe {
                let mut rect = RECT::default();
                if GetWindowRect(control, &mut rect).is_ok() {
                    let _ = SetWindowPos(
                        control,
                        None,
                        0,
                        0,
                        (rect.right - rect.left).max(1),
                        compact,
                        SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
            }
        }
    }

    fn handle_command(
        hwnd: HWND,
        state: &mut ManagerState,
        command: usize,
        control: HWND,
    ) -> LRESULT {
        let id = command & 0xffff;
        let notification = (command >> 16) & 0xffff;
        if notification == CBN_DROPDOWN {
            expand_combo_for_dropdown(state, control);
        }
        if notification == CBN_CLOSEUP {
            restore_combo_after_dropdown(state, control);
        }
        if id == ID_HISTORY_LIST && notification == LBN_SELCHANGE {
            history_selection_changed(state);
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
        if id == ID_PROMPT_ID && notification == CBN_SELCHANGE {
            // The ID dropdown lists every profile; picking one loads it.
            load_profile_from_dropdown(state);
            return LRESULT(0);
        }
        if (ID_PROFILE_MOVE_BASE..ID_PROFILE_MOVE_BASE + ID_PROFILE_MOVE_SLOTS).contains(&id) {
            // Reorder buttons send BN_CLICKED only; other notifications (for
            // example focus aesthetics) fall through.
            if notification == 0 {
                let slot = id - ID_PROFILE_MOVE_BASE;
                move_profile_order(hwnd, state, slot / 2, slot.is_multiple_of(2));
            }
            return LRESULT(0);
        }
        match id {
            ID_SETTINGS_TAB => switch_view(hwnd, state, View::Settings),
            ID_PROMPTS_TAB => switch_view(hwnd, state, View::Prompts),
            ID_HISTORY_TAB => switch_view(hwnd, state, View::History),
            ID_CLOSE => unsafe {
                DestroyWindow(hwnd).ok();
            },
            ID_SAVE_SETTINGS => save_settings(state),
            ID_SAVE_KEY => save_key(state),
            ID_DELETE_KEY => delete_key(state),
            ID_SAVE_PROMPT => save_prompt(state),
            ID_NEW_PROMPT => new_prompt(state),
            ID_DELETE_PROMPT => delete_profile(hwnd, state),
            ID_HISTORY_REFRESH => refresh_history(state),
            ID_HISTORY_COPY => copy_history_output(hwnd, state),
            ID_HISTORY_DELETE => delete_history(hwnd, state),
            _ => {}
        }
        LRESULT(0)
    }
    fn switch_view(hwnd: HWND, state: &mut ManagerState, view: View) {
        if state.view == View::Prompts && view != View::Prompts {
            discard_draft(state);
        }
        state.view = view;
        show_view(hwnd, state);
        // Re-run the responsive layout: page slots are positioned for the
        // active view only, so a newly shown page needs its geometry applied.
        apply_manager_layout(hwnd, state);
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
        apply_search_placeholder(state);
        populate_language_selector(state);
        populate_history_filters(state);
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

    /// Muted in-field hint for the empty history search box (mockup shows
    /// the placeholder inside the input).
    fn apply_search_placeholder(state: &ManagerState) {
        const EM_SETCUEBANNER: u32 = 0x1501;
        let text = wide(ui_text(state.language(), TextKey::SearchTarget));
        unsafe {
            let _ = SendMessageW(
                state.handles.history_search,
                EM_SETCUEBANNER,
                Some(WPARAM(1)),
                Some(LPARAM(text.as_ptr() as isize)),
            );
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

    fn config_with_theme(config: &AppConfig, theme: UiTheme) -> AppConfig {
        let mut next = config.clone();
        next.ui.theme = theme;
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
            add_combo_string(state.handles.history_prompt, profile_display_name(profile));
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
                add_combo_string(hwnd, profile_display_name(profile));
            }
            if let Some(index) = selected {
                set_combo_selection(hwnd, index);
            }
        }
    }

    fn selected_profile_index(profiles: &[PromptConfig], id: &str) -> Option<usize> {
        profiles.iter().position(|profile| profile.id == id)
    }

    fn selected_profile_id(hwnd: HWND, profiles: &[PromptConfig]) -> Option<String> {
        profile_id_for_index(combo_selection(hwnd), profiles)
    }

    fn profile_id_for_index(index: Option<usize>, profiles: &[PromptConfig]) -> Option<String> {
        index
            .and_then(|index| profiles.get(index))
            .map(|profile| profile.id.clone())
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
                    state.handles.history_meta,
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
                set_text(state.handles.history_meta, &message);
                set_status(state, &message);
            }
        }
        fair_copy_push_history(state);
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

    fn populate_history_list(state: &mut ManagerState) {
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
                // Two panes, same as the popup's selection card: the mono
                // target pane above the muted context pane.
                let context = match entry.context.as_deref() {
                    Some(context) if !context.trim().is_empty() => context.to_owned(),
                    _ => String::new(),
                };
                state.history_has_context = !context.is_empty();
                set_text(state.handles.history_target, &entry.target);
                set_text(state.handles.history_context, &context);
                apply_history_detail_split(state);
                set_text(state.handles.history_output, &entry.output);
                apply_rich_ink(state.handles.history_output);
                set_text(
                    state.handles.history_meta,
                    &format!(
                        "{} · {} · {} · {}{}",
                        entry.created_at_utc,
                        source_label(state.language(), entry.source),
                        history_profile_label(
                            &state.config.profiles,
                            &entry.prompt_id,
                            ui_text(state.language(), TextKey::DeletedProfile),
                        ),
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

    fn clear_history_detail(state: &mut ManagerState) {
        state.history_has_context = false;
        set_text(state.handles.history_target, "");
        set_text(state.handles.history_context, "");
        apply_history_detail_split(state);
        set_text(state.handles.history_output, "");
        if state.history_loaded {
            set_text(
                state.handles.history_meta,
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

    /// Delete the profile selected on the Profiles page, after confirmation.
    /// An unsaved draft is discarded instead (see `discard_draft`).
    fn delete_profile(owner: HWND, state: &mut ManagerState) {
        if state.draft_prompt.is_some() {
            return;
        }
        let Some(profile) = state.config.profiles.get(state.profile_index) else {
            set_text(
                state.handles.prompt_status,
                &status_text(state.language(), StatusEvent::NoPromptProfile),
            );
            return;
        };
        let id = profile.id.clone();
        let name = profile_display_name(profile).to_owned();
        let next = match config_without_profile(&state.config, &id) {
            Ok(next) => next,
            Err(blocked) => {
                let event = match blocked {
                    DeleteBlocked::LastProfile => StatusEvent::DeleteBlockedLast,
                    DeleteBlocked::SelectionDefault => StatusEvent::DeleteBlockedSelectionDefault,
                    DeleteBlocked::HoverDefault => StatusEvent::DeleteBlockedHoverDefault,
                };
                set_text(
                    state.handles.prompt_status,
                    &status_text(state.language(), event),
                );
                return;
            }
        };
        let confirmation = status_text(
            state.language(),
            StatusEvent::DeleteProfileConfirm { name: &name },
        );
        let message = wide(&confirmation);
        let title = wide(ui_text(state.language(), TextKey::WindowTitle));
        let result = unsafe {
            MessageBoxW(
                Some(owner),
                PCWSTR(message.as_ptr()),
                PCWSTR(title.as_ptr()),
                MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2,
            )
        };
        if result != IDYES {
            set_text(
                state.handles.prompt_status,
                &status_text(state.language(), StatusEvent::DeletionCancelled),
            );
            return;
        }
        if let Err(error) = next
            .validate()
            .map_err(|error| error.to_string())
            .and_then(|()| save_config(state, &next))
        {
            set_text(
                state.handles.prompt_status,
                &status_text(
                    state.language(),
                    StatusEvent::DeleteProfileFailed { detail: &error },
                ),
            );
            return;
        }
        state.config = next;
        let refresh = notify_config_changed();
        state.profile_index = state
            .profile_index
            .min(state.config.profiles.len().saturating_sub(1));
        refresh_prompt_form(state);
        populate_profile_selectors(state);
        populate_history_filters(state);
        let _ = refresh;
        set_text(
            state.handles.prompt_status,
            &status_text(
                state.language(),
                StatusEvent::ProfileDeleted { name: &name },
            ),
        );
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
        set_text(
            state.handles.hotkey_selection,
            &display_hotkey(state.config.hotkeys.selection_translate.as_deref()),
        );
        set_text(
            state.handles.hotkey_hover,
            &display_hotkey(state.config.hotkeys.hover_translate.as_deref()),
        );
        set_text(
            state.handles.hotkey_pause,
            &display_hotkey(state.config.hotkeys.pause_resume.as_deref()),
        );
        set_text(
            state.handles.hotkey_open_popup,
            &display_hotkey(state.config.hotkeys.open_popup.as_deref()),
        );
        populate_profile_selectors(state);
    }

    /// Hotkeys are shown with spaced separators ("Ctrl + Shift + T") and an
    /// empty field means the hotkey is off.
    fn display_hotkey(spec: Option<&str>) -> String {
        spec.unwrap_or_default()
            .split('+')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" + ")
    }

    /// Normalize typed hotkey text back to the canonical "Ctrl+Shift+T"
    /// form. Empty input normalizes to an empty string, meaning "off".
    fn normalized_hotkey(input: &str) -> String {
        input
            .split('+')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("+")
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
        discard_draft(state);
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
        next.hotkeys.selection_translate = Some(normalized_hotkey(&read_text(
            state.handles.hotkey_selection,
        )));
        next.hotkeys.hover_translate =
            Some(normalized_hotkey(&read_text(state.handles.hotkey_hover)));
        next.hotkeys.pause_resume = Some(normalized_hotkey(&read_text(state.handles.hotkey_pause)));
        next.hotkeys.open_popup = Some(normalized_hotkey(&read_text(
            state.handles.hotkey_open_popup,
        )));
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
        fair_copy_push_all(state);
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
        if state.draft_prompt.is_none()
            && (state.config.profiles.is_empty()
                || state.profile_index >= state.config.profiles.len())
        {
            set_status(
                state,
                &status_text(state.language(), StatusEvent::NoPromptProfile),
            );
            return;
        }
        let prompt = match prompt_from_form(state) {
            Ok(prompt) => prompt,
            Err(error) => {
                set_text(state.handles.prompt_status, &error);
                return;
            }
        };
        if profile_name_taken(&state.config.profiles, &prompt.name, Some(&prompt.id)) {
            set_text(
                state.handles.prompt_status,
                &status_text(
                    state.language(),
                    StatusEvent::DuplicateProfileName { name: &prompt.name },
                ),
            );
            return;
        }
        let was_draft = state.draft_prompt.is_some();
        let old_id = (!was_draft).then(|| state.config.profiles[state.profile_index].id.clone());
        // Global defaults belong to the Settings page; prompt saves leave
        // them untouched.
        let next = apply_prompt(
            &state.config,
            (!was_draft).then_some(state.profile_index),
            old_id.as_deref(),
            prompt,
        );
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
        let refresh = notify_config_changed();
        populate_history_filters(state);
        state.draft_prompt = None;
        if was_draft {
            state.profile_index = state.config.profiles.len() - 1;
        }
        set_text(
            state.handles.prompt_status,
            &config_refresh_status(state.language(), refresh),
        );
        refresh_prompt_form(state);
    }
    fn new_prompt(state: &mut ManagerState) {
        let mut prompt = PromptConfig::new(next_profile_id(&state.config));
        prompt.name = "New profile".to_owned();
        prompt.system_prompt = "Interpret the target using the supplied context.".to_owned();
        prompt.user_template =
            "Target:\n{target}\nContext:\n{context}\nSource:\n{source}".to_owned();
        state.draft_prompt = Some(prompt);
        refresh_prompt_form(state);
        set_text(
            state.handles.prompt_status,
            &status_text(state.language(), StatusEvent::NewPromptUnsaved),
        );
    }
    /// Load the profile chosen in the ID dropdown (mockup: "ID dropdown
    /// lists all profiles — switch to load another prompt").
    fn load_profile_from_dropdown(state: &mut ManagerState) {
        let Some(index) = combo_selection(state.handles.profile_id) else {
            return;
        };
        if state.draft_prompt.is_some() {
            refresh_profile_dropdown(state);
            return;
        }
        if index >= state.config.profiles.len() {
            return;
        }
        if index != state.profile_index {
            state.profile_index = index;
            refresh_prompt_form(state);
        }
    }
    fn refresh_prompt_form(state: &mut ManagerState) {
        refresh_profile_dropdown(state);
        if let Some(prompt) = state
            .draft_prompt
            .as_ref()
            .or_else(|| state.config.profiles.get(state.profile_index))
        {
            set_text(state.handles.profile_name, &prompt.name);
            set_text(state.handles.system_prompt, &prompt.system_prompt);
            set_text(state.handles.user_template, &prompt.user_template);
            apply_rich_ink(state.handles.system_prompt);
            apply_rich_ink(state.handles.user_template);
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
        }
    }

    /// Sync the ID dropdown with the configured profiles. Selecting an
    /// option switches profiles; the edit half stays free for renaming or
    /// draft ids.
    fn refresh_profile_dropdown(state: &mut ManagerState) {
        reset_combo(state.handles.profile_id);
        for profile in &state.config.profiles {
            add_combo_string(state.handles.profile_id, profile_display_name(profile));
        }
        match state.draft_prompt.as_ref() {
            Some(draft) => {
                add_combo_string(state.handles.profile_id, profile_display_name(draft));
                set_combo_selection(state.handles.profile_id, state.config.profiles.len());
            }
            None => set_combo_selection(state.handles.profile_id, state.profile_index),
        }
    }

    fn discard_draft(state: &mut ManagerState) {
        if state.draft_prompt.take().is_some() {
            refresh_prompt_form(state);
            set_text(
                state.handles.prompt_status,
                &status_text(state.language(), StatusEvent::UnsavedPromptDiscarded),
            );
        }
    }

    /// Why a profile cannot be deleted.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum DeleteBlocked {
        LastProfile,
        SelectionDefault,
        HoverDefault,
    }

    /// The configuration without profile `id`, or the reason it must stay.
    /// The user changes a default in Settings before deleting its profile.
    fn config_without_profile(config: &AppConfig, id: &str) -> Result<AppConfig, DeleteBlocked> {
        if config.profiles.len() <= 1 {
            return Err(DeleteBlocked::LastProfile);
        }
        if config.defaults.selection == id {
            return Err(DeleteBlocked::SelectionDefault);
        }
        if config.defaults.hover == id {
            return Err(DeleteBlocked::HoverDefault);
        }
        let mut next = config.clone();
        next.profiles.retain(|profile| profile.id != id);
        Ok(next)
    }

    /// An ID no existing profile uses. `len + 1` alone can collide after a delete.
    fn next_profile_id(config: &AppConfig) -> String {
        let mut number = config.profiles.len() + 1;
        loop {
            let id = format!("custom-{number}");
            if !config.profiles.iter().any(|profile| profile.id == id) {
                return id;
            }
            number += 1;
        }
    }

    /// True when another profile (not `own_id`) already has this name,
    /// compared trimmed and case-insensitively.
    fn profile_name_taken(profiles: &[PromptConfig], name: &str, own_id: Option<&str>) -> bool {
        let wanted = name.trim().to_lowercase();
        profiles.iter().any(|profile| {
            Some(profile.id.as_str()) != own_id && profile.name.trim().to_lowercase() == wanted
        })
    }

    /// What users see for a profile: its name, never its ID (an old file
    /// with a blank name falls back to the ID rather than an empty entry).
    fn profile_display_name(profile: &PromptConfig) -> &str {
        let name = profile.name.trim();
        if name.is_empty() {
            &profile.id
        } else {
            name
        }
    }

    /// The name of the profile that produced a history entry, or `deleted`
    /// when that profile no longer exists.
    fn history_profile_label(profiles: &[PromptConfig], prompt_id: &str, deleted: &str) -> String {
        profiles
            .iter()
            .find(|profile| profile.id == prompt_id)
            .map_or_else(
                || deleted.to_owned(),
                |p| profile_display_name(p).to_owned(),
            )
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
            // The ID is assigned by the app and is not editable.
            id: state
                .draft_prompt
                .as_ref()
                .or_else(|| state.config.profiles.get(state.profile_index))
                .map(|profile| profile.id.clone())
                .unwrap_or_default(),
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
    /// Pure guard: a config built from defaults may only be written when no
    /// file exists at the target path; a file that appeared after startup
    /// (or failed to load) belongs to the user and is never clobbered.
    fn should_write_config(loaded_from_disk: bool, path_exists: bool) -> bool {
        loaded_from_disk || !path_exists
    }

    // —— Fair-copy manager view (WebView2 companion) ——

    /// Timer that watches the companion process so closing the page restores
    /// the native manager window.
    const FAIR_COPY_POLL_TIMER_ID: usize = 1;
    /// Polls config.toml so a tray skin switch reaches an open manager.
    const CONFIG_POLL_TIMER_ID: usize = 2;

    /// Set while the fair-copy view owns the presentation: the startup
    /// ShowWindow must then leave the native window hidden (WM_CREATE hides
    /// it inside start_fair_copy_view, and the startup show would otherwise
    /// undo that, leaving both UIs on screen at once).
    static FAIR_COPY_ACTIVE: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);

    /// Spawn the fair-copy manager view. On success the native window hides
    /// and the page becomes the presentation; on any failure the native
    /// manager simply stays as it is.
    fn start_fair_copy_view(hwnd: HWND, state: &mut ManagerState) {
        if state.fair_copy.is_some() {
            return;
        }
        // Respect the skin setting: the fair-copy manager view only exists
        // while the fair-copy skin is active; with the classic skin the
        // native manager stays (the user switches via the tray menu). The
        // skin is re-read from config.toml — the manager's in-memory
        // snapshot predates any tray switch made after this process started.
        if let Some(path) = default_config_path() {
            if let Ok(current) = lingosnap_core::AppConfig::load(&path) {
                state.config.ui.popup_skin = current.ui.popup_skin;
            }
        }
        if state.config.ui.popup_skin != lingosnap_core::config::PopupSkin::FairCopy {
            return;
        }
        let Ok(mut ui) = webview_ui::FairCopyUi::start("manager", hwnd) else {
            return;
        };
        // Initial snapshots; a companion that is still navigating may miss
        // them, so the page can re-request via the refresh commands.
        fair_copy_push_settings(state, &mut ui);
        fair_copy_push_prompts(state, &mut ui);
        state.fair_copy = Some(ui);
        state.fair_copy_ready = false;
        FAIR_COPY_ACTIVE.store(true, std::sync::atomic::Ordering::Relaxed);
        // The page positions itself (centered, at the shell's designed
        // 860×700 size) and is the manager's taskbar window; the native
        // window stays hidden behind it as the source of truth.
        unsafe {
            let _ = SetTimer(Some(hwnd), FAIR_COPY_POLL_TIMER_ID, 500, None);
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }

    fn fair_copy_push_settings(state: &ManagerState, ui: &mut webview_ui::FairCopyUi) {
        let profiles: Vec<_> = state
            .config
            .profiles
            .iter()
            .map(|profile| serde_json::json!({ "id": profile.id, "name": profile.name }))
            .collect();
        let order: Vec<_> = state
            .config
            .profiles
            .iter()
            .map(|profile| profile.id.clone())
            .collect();
        let hotkeys = &state.config.hotkeys;
        let payload = serde_json::json!({
            "apply": {
                "kind": "settings",
                "config": {
                    "endpoint": state.config.provider.endpoint,
                    "model": state.config.provider.model,
                    "credtarget": state.config.provider.credential_target,
                    "hkSelection": hotkeys.selection_translate,
                    "hkHover": hotkeys.hover_translate,
                    "hkPause": hotkeys.pause_resume,
                    "hkPopup": hotkeys.open_popup,
                    "language": match state.config.ui.manager_language {
                        UiLanguage::SimplifiedChinese => "zh-CN",
                        UiLanguage::English => "en",
                    },
                    "theme": match state.config.ui.theme {
                        UiTheme::Dark => "dark",
                        UiTheme::Light => "light",
                    },
                    "selectionId": state.config.defaults.selection,
                    "hoverId": state.config.defaults.hover,
                    "credentialPresent": matches!(
                        state.credential_status,
                        CredentialStatusState::Present
                    ),
                    "credentialStatus": status_text(
                        state.language(),
                        credential_status_event(&state.credential_status),
                    ),
                    "profiles": profiles,
                    "order": order,
                }
            }
        });
        let _ = ui.send_raw(&payload.to_string());
    }

    fn fair_copy_push_prompts(state: &ManagerState, ui: &mut webview_ui::FairCopyUi) {
        let profiles: Vec<_> = state
            .config
            .profiles
            .iter()
            .map(|profile| serde_json::json!({ "id": profile.id, "name": profile.name }))
            .collect();
        // An unsaved New prompt is the form's content until it is saved,
        // exactly as in the native Prompts page.
        let draft = state.draft_prompt.as_ref();
        let shown = draft.or_else(|| state.config.profiles.get(state.profile_index));
        let payload = shown.map_or_else(
            || {
                serde_json::json!({
                    "apply": { "kind": "prompts", "profiles": profiles, "selectedId": null }
                })
            },
            |profile| {
                serde_json::json!({
                    "apply": {
                        "kind": "prompts",
                        "profiles": profiles,
                        "selectedId": profile.id,
                        "draft": draft.is_some(),
                        "selected": {
                            "name": profile.name,
                            "model": profile.model,
                            "temp": profile
                                .temperature
                                .map(|t| (f64::from(t) * 100.0).round() / 100.0),
                            "tokens": profile.max_output_tokens,
                            "sys": profile.system_prompt,
                            "user": profile.user_template,
                        }
                    }
                })
            },
        );
        let _ = ui.send_raw(&payload.to_string());
    }

    fn fair_copy_history_entries(state: &ManagerState) -> Vec<serde_json::Value> {
        state
            .history_entries
            .iter()
            .take(200)
            .map(|entry| {
                let preview = entry.output.lines().find(|line| !line.trim().is_empty());
                let preview = preview
                    .unwrap_or_default()
                    .trim_start_matches(['#', '-', '*', ' '])
                    .to_owned();
                serde_json::json!({
                    "utc": entry.created_at_utc,
                    "target": entry.target,
                    "preview": preview,
                    "prompt": history_profile_label(
                        &state.config.profiles,
                        &entry.prompt_id,
                        ui_text(state.language(), TextKey::DeletedProfile),
                    ),
                    "source": source_label(state.language(), entry.source),
                    "context": entry.context,
                    "output": entry.output,
                })
            })
            .collect()
    }

    fn fair_copy_push_history(state: &mut ManagerState) {
        let entries = fair_copy_history_entries(state);
        let selected =
            selected_history_index(state.handles.history_list, state.history_entries.len())
                .unwrap_or(0);
        let payload = serde_json::json!({
            "apply": {
                "kind": "history",
                "total": state.history_entries.len(),
                "selected": selected,
                "entries": entries,
            }
        });
        if let Some(ui) = state.fair_copy.as_mut() {
            let _ = ui.send_raw(&payload.to_string());
        }
    }

    fn fair_copy_push_history_detail(state: &mut ManagerState, index: usize) {
        let Some(entry) = fair_copy_history_entries(state).get(index).cloned() else {
            return;
        };
        let payload = serde_json::json!({
            "apply": { "kind": "history_detail", "index": index, "entry": entry }
        });
        if let Some(ui) = state.fair_copy.as_mut() {
            let _ = ui.send_raw(&payload.to_string());
        }
    }

    /// Mirror the native status line (and the prompt status, which the
    /// native Prompts page shows separately) onto the page.
    fn fair_copy_push_status(state: &mut ManagerState, prompt_page: bool) {
        let handle = if prompt_page {
            state.handles.prompt_status
        } else {
            state.handles.status
        };
        let message = read_text(handle);
        if message.trim().is_empty() {
            return;
        }
        let payload = serde_json::json!({
            "apply": {
                "kind": "status",
                "message": message,
                "ok": !status_looks_failed(&message),
            }
        });
        if let Some(ui) = state.fair_copy.as_mut() {
            let _ = ui.send_raw(&payload.to_string());
        }
    }

    /// The native status strings carry no severity flag; failures are the
    /// ones that explain what could not be done.
    fn status_looks_failed(message: &str) -> bool {
        let lower = message.to_lowercase();
        [
            "fail",
            "cannot",
            "could not",
            "unavailable",
            "error",
            "invalid",
            "失败",
            "无法",
            "错误",
        ]
        .iter()
        .any(|word| lower.contains(word))
    }

    /// Select one native history row, so the native copy/delete operations
    /// act on the entry the page shows.
    fn select_native_history_row(state: &ManagerState, index: usize) {
        unsafe {
            let _ = SendMessageW(
                state.handles.history_list,
                windows::Win32::UI::WindowsAndMessaging::LB_SETCURSEL,
                Some(WPARAM(index)),
                Some(LPARAM(0)),
            );
        }
    }

    /// Commands posted by the fair-copy page. The native state stays the
    /// source of truth: every command flows through the same validation and
    /// save path as the native controls, then the fresh state is pushed back.
    fn handle_fair_copy_command(hwnd: HWND, state: &mut ManagerState, line: &str) {
        let Ok(command) = serde_json::from_str::<serde_json::Value>(line) else {
            return;
        };
        let text = |key: &str| {
            command
                .get(key)
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let index = |key: &str| {
            command
                .get(key)
                .and_then(serde_json::Value::as_u64)
                .map(|value| value as usize)
        };
        // The page announces itself after loading: the snapshots pushed at
        // spawn raced the navigation and were lost, so the page would show
        // mockup sample data instead of the user's real configuration.
        if command.get("type").and_then(serde_json::Value::as_str) == Some("page_ready") {
            state.fair_copy_ready = true;
            update_credential_status(state);
            fair_copy_push_all(state);
            refresh_history(state);
            fair_copy_push_history_detail(state, 0);
            return;
        }
        match command.get("mcmd").and_then(serde_json::Value::as_str) {
            Some("save_settings") => {
                let mut next = state.config.clone();
                next.provider.endpoint = text("endpoint").trim().to_owned();
                next.provider.model = text("model").trim().to_owned();
                next.provider.credential_target = text("credtarget").trim().to_owned();
                // Same contract as the native Settings page: an empty field is
                // stored as an empty hot key, meaning "off" (a missing key
                // would fall back to the default on the next load).
                let normalize = |raw: String| -> Option<String> { Some(normalized_hotkey(&raw)) };
                next.hotkeys.selection_translate = normalize(text("hkSelection"));
                next.hotkeys.hover_translate = normalize(text("hkHover"));
                next.hotkeys.pause_resume = normalize(text("hkPause"));
                next.hotkeys.open_popup = normalize(text("hkPopup"));
                for (key, slot) in [
                    ("selectionDefault", &mut next.defaults.selection),
                    ("hoverDefault", &mut next.defaults.hover),
                ] {
                    let id = text(key);
                    if !id.is_empty() && state.config.profile(&id).is_some() {
                        *slot = id;
                    }
                }
                if let Some(order) = command.get("order").and_then(serde_json::Value::as_array) {
                    let ids: Vec<_> = order
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .map(str::to_owned)
                        .collect();
                    if !ids.is_empty() {
                        let mut reordered: Vec<PromptConfig> = Vec::with_capacity(ids.len());
                        for id in &ids {
                            if let Some(position) =
                                next.profiles.iter().position(|profile| &profile.id == id)
                            {
                                reordered.push(next.profiles.remove(position));
                            }
                        }
                        reordered.append(&mut next.profiles);
                        next.profiles = reordered;
                    }
                }
                if next == state.config {
                    fair_copy_push_all(state);
                    return;
                }
                let cannot_save = |state: &ManagerState, detail: &str| {
                    set_status(
                        state,
                        &status_text(state.language(), StatusEvent::CannotSaveSettings { detail }),
                    )
                };
                match next.validate() {
                    Err(error) => cannot_save(state, &error.to_string()),
                    Ok(()) => match save_config(state, &next) {
                        Err(error) => cannot_save(state, &error),
                        Ok(()) => {
                            state.config = next;
                            refresh_settings_form(state);
                            populate_profile_selectors(state);
                            populate_history_filters(state);
                            let refresh = notify_config_changed();
                            set_status(state, &config_refresh_status(state.language(), refresh));
                            update_credential_status(state);
                        }
                    },
                }
                fair_copy_push_all(state);
                fair_copy_push_status(state, false);
            }
            Some("save_language") => {
                let language = if text("language") == "zh-CN" { 1 } else { 0 };
                set_combo_selection(state.handles.language, language);
                save_manager_language(hwnd, state);
                // Let the resident tell the popups about the new language.
                let _ = notify_config_changed();
                fair_copy_push_all(state);
                fair_copy_push_status(state, false);
            }
            Some("save_theme") => {
                let theme = if text("theme") == "dark" {
                    UiTheme::Dark
                } else {
                    UiTheme::Light
                };
                if theme != state.config.ui.theme {
                    let next = config_with_theme(&state.config, theme);
                    match save_config(state, &next) {
                        Ok(()) => {
                            state.config = next;
                            let refresh = notify_config_changed();
                            set_status(state, &config_refresh_status(state.language(), refresh));
                        }
                        Err(error) => set_status(
                            state,
                            &status_text(
                                state.language(),
                                StatusEvent::SaveThemeFailed { detail: &error },
                            ),
                        ),
                    }
                }
                fair_copy_push_all(state);
                fair_copy_push_status(state, false);
            }
            Some("save_key") => {
                // The key lives in the native password edit only for the
                // duration of the native save path, which clears it.
                set_text(state.handles.credential_target, &text("target"));
                set_text(state.handles.api_key, &text("key"));
                save_key(state);
                set_text(state.handles.api_key, "");
                fair_copy_push_all(state);
                fair_copy_push_status(state, false);
            }
            Some("delete_key") => {
                set_text(state.handles.credential_target, &text("target"));
                delete_key(state);
                fair_copy_push_all(state);
                fair_copy_push_status(state, false);
            }
            Some("select_prompt") => {
                let id = text("id");
                if let Some(index) = state.config.profiles.iter().position(|p| p.id == id) {
                    state.profile_index = index;
                    state.draft_prompt = None;
                    refresh_prompt_form(state);
                } else if state
                    .draft_prompt
                    .as_ref()
                    .is_some_and(|draft| draft.id == id)
                {
                    // Restore on an unsaved draft: start the draft over.
                    new_prompt(state);
                }
                fair_copy_push_all(state);
            }
            Some("new_prompt") => {
                new_prompt(state);
                fair_copy_push_all(state);
                fair_copy_push_status(state, true);
            }
            Some("delete_prompt") => {
                let id = text("id");
                if let Some(index) = state.config.profiles.iter().position(|p| p.id == id) {
                    state.profile_index = index;
                    state.draft_prompt = None;
                    let owner = unsafe { FindWindowW(w!("LingoSnapManagerUi"), PCWSTR::null()) }
                        .unwrap_or(hwnd);
                    delete_profile(owner, state);
                    populate_profile_selectors(state);
                }
                fair_copy_push_all(state);
                fair_copy_push_status(state, true);
            }
            Some("save_prompt") => {
                fair_copy_save_prompt(state, &command);
                populate_profile_selectors(state);
                fair_copy_push_all(state);
                fair_copy_push_status(state, true);
            }
            Some("history_query") => {
                set_text(state.handles.history_search, &text("search"));
                set_combo_selection(state.handles.history_prompt, index("prompt").unwrap_or(0));
                set_combo_selection(state.handles.history_source, index("source").unwrap_or(0));
                set_combo_selection(state.handles.history_order, index("order").unwrap_or(0));
                refresh_history(state);
                fair_copy_push_history_detail(state, 0);
            }
            Some("refresh_history") => {
                refresh_history(state);
                fair_copy_push_history_detail(state, 0);
            }
            Some("select_history") => {
                if let Some(index) = index("index") {
                    select_native_history_row(state, index);
                    fair_copy_push_history_detail(state, index);
                }
            }
            Some("copy_history") => {
                if let Some(index) = index("index") {
                    select_native_history_row(state, index);
                    copy_history_output(hwnd, state);
                    fair_copy_push_status(state, false);
                }
            }
            Some("delete_history") => {
                if let Some(index) = index("index") {
                    select_native_history_row(state, index);
                    // The confirmation belongs to the page's window, which
                    // the user is looking at (the native one is hidden).
                    let owner = unsafe {
                        FindWindowW(windows::core::w!("LingoSnapManagerUi"), PCWSTR::null())
                    }
                    .unwrap_or(hwnd);
                    delete_history(owner, state);
                    fair_copy_push_history(state);
                    fair_copy_push_history_detail(state, 0);
                    fair_copy_push_status(state, false);
                }
            }
            _ => {}
        }
    }

    /// The native Prompts save, fed from the page's form: same parsing,
    /// validation, draft and rename handling, and status text.
    fn fair_copy_save_prompt(state: &mut ManagerState, command: &serde_json::Value) {
        let text = |key: &str| {
            command
                .get(key)
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let language = state.language();
        let parsed = parse_optional_f32(&text("temp"), language).and_then(|temperature| {
            parse_optional_u32(&text("tokens"), language).map(|tokens| (temperature, tokens))
        });
        let (temperature, max_output_tokens) = match parsed {
            Ok(values) => values,
            Err(error) => {
                set_text(state.handles.prompt_status, &error);
                return;
            }
        };
        let prompt = PromptConfig {
            id: text("id").trim().to_owned(),
            name: text("name").trim().to_owned(),
            system_prompt: text("sys"),
            user_template: text("user"),
            model: nonempty(text("model")),
            temperature,
            max_output_tokens,
        };
        if let Err(error) = prompt.validate() {
            set_text(
                state.handles.prompt_status,
                &status_text(
                    language,
                    StatusEvent::PromptInvalid {
                        detail: &error.to_string(),
                    },
                ),
            );
            return;
        }
        if profile_name_taken(&state.config.profiles, &prompt.name, Some(&prompt.id)) {
            set_text(
                state.handles.prompt_status,
                &status_text(
                    language,
                    StatusEvent::DuplicateProfileName { name: &prompt.name },
                ),
            );
            return;
        }
        let was_draft = state.draft_prompt.is_some();
        if !was_draft && state.profile_index >= state.config.profiles.len() {
            set_text(
                state.handles.prompt_status,
                &status_text(language, StatusEvent::NoPromptProfile),
            );
            return;
        }
        let old_id = (!was_draft).then(|| state.config.profiles[state.profile_index].id.clone());
        let next = apply_prompt(
            &state.config,
            (!was_draft).then_some(state.profile_index),
            old_id.as_deref(),
            prompt,
        );
        let saved = next
            .validate()
            .map_err(|error| error.to_string())
            .and_then(|()| save_config(state, &next));
        if let Err(error) = saved {
            set_text(
                state.handles.prompt_status,
                &status_text(language, StatusEvent::CannotSavePrompt { detail: &error }),
            );
            return;
        }
        state.config = next;
        let refresh = notify_config_changed();
        populate_history_filters(state);
        state.draft_prompt = None;
        if was_draft {
            state.profile_index = state.config.profiles.len() - 1;
        }
        refresh_prompt_form(state);
        set_text(
            state.handles.prompt_status,
            &config_refresh_status(language, refresh),
        );
    }

    /// Push settings and prompts snapshots after a page-initiated change.
    fn fair_copy_push_all(state: &mut ManagerState) {
        // Take the companion out so the pushes can borrow state immutably.
        if let Some(mut ui) = state.fair_copy.take() {
            fair_copy_push_settings(state, &mut ui);
            fair_copy_push_prompts(state, &mut ui);
            state.fair_copy = Some(ui);
        }
    }

    fn save_config(state: &ManagerState, config: &AppConfig) -> Result<(), String> {
        let path = state
            .config_path
            .as_ref()
            .ok_or_else(|| status_text(state.language(), StatusEvent::ConfigPathUnavailable))?;
        if !should_write_config(state.loaded_from_disk, path.exists()) {
            return Err(status_text(
                state.language(),
                StatusEvent::ConfigExistsNotOverwritten,
            ));
        }
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
            config_without_profile, credential_refresh_status, format_history_row,
            history_count_text, history_profile_label, history_source_for_index, next_profile_id,
            parse_optional_f32, parse_optional_u32, profile_display_name, profile_id_for_index,
            profile_name_taken, resident_start_status, status_text, ui_text, valid_history_index,
            visibility_style, DeleteBlocked, ManagerLayout, StatusEvent, StatusOperation, View,
            ALL_TEXT_KEYS,
        };
        use lingosnap_core::{AppConfig, ExtractionSource, PromptConfig, UiLanguage};
        use lingosnap_platform_windows::app::{RefreshOutcome, ResidentStartOutcome};
        use lingosnap_storage::HistoryEntry;
        use windows::Win32::UI::WindowsAndMessaging::WS_VISIBLE;
        #[test]
        fn manager_page_window_class_matches_the_protocol_constant() {
            // The resident and the companion find the page window by this
            // name; w! needs a literal, so the agreement is checked here.
            let class = unsafe { windows::core::w!("LingoSnapManagerUi").to_string() }.unwrap();
            assert_eq!(class, lingosnap_ui_protocol::MANAGER_WINDOW_CLASS);
        }

        fn named(id: &str, name: &str) -> PromptConfig {
            let mut profile = PromptConfig::new(id);
            profile.name = name.to_owned();
            profile
        }

        #[test]
        fn delete_is_blocked_for_the_last_profile_and_for_defaults() {
            let mut config = AppConfig::default();
            let ids: Vec<String> = config.profiles.iter().map(|p| p.id.clone()).collect();
            assert!(ids.len() > 2);
            config.defaults.selection = ids[0].clone();
            config.defaults.hover = ids[1].clone();
            assert_eq!(
                config_without_profile(&config, &ids[0]).unwrap_err(),
                DeleteBlocked::SelectionDefault
            );
            assert_eq!(
                config_without_profile(&config, &ids[1]).unwrap_err(),
                DeleteBlocked::HoverDefault
            );
            let next = config_without_profile(&config, &ids[2]).expect("plain profile deletes");
            assert_eq!(next.profiles.len(), config.profiles.len() - 1);
            assert!(next.profiles.iter().all(|p| p.id != ids[2]));
            // The remaining order is untouched.
            let kept: Vec<_> = ids.iter().filter(|id| **id != ids[2]).cloned().collect();
            let after: Vec<_> = next.profiles.iter().map(|p| p.id.clone()).collect();
            assert_eq!(after, kept);
            assert!(next.validate().is_ok());

            let mut single = config.clone();
            single.profiles.truncate(1);
            single.defaults.selection = single.profiles[0].id.clone();
            single.defaults.hover = single.profiles[0].id.clone();
            assert_eq!(
                config_without_profile(&single, &single.profiles[0].id.clone()).unwrap_err(),
                DeleteBlocked::LastProfile
            );
        }

        #[test]
        fn new_profile_ids_never_collide_after_a_delete() {
            let mut config = AppConfig {
                profiles: vec![named("custom-2", "Two"), named("custom-3", "Three")],
                ..AppConfig::default()
            };
            // len + 1 would be custom-3, which exists.
            assert_eq!(next_profile_id(&config), "custom-4");
            config.profiles = vec![named("a", "A")];
            assert_eq!(next_profile_id(&config), "custom-2");
        }

        #[test]
        fn profile_names_must_be_unique_ignoring_case_and_padding() {
            let profiles = vec![named("a", "Translate"), named("b", "解释")];
            assert!(profile_name_taken(&profiles, "  translate ", None));
            assert!(profile_name_taken(&profiles, "TRANSLATE", Some("b")));
            // A profile may keep its own name.
            assert!(!profile_name_taken(&profiles, "Translate", Some("a")));
            assert!(!profile_name_taken(&profiles, "Explain", None));
        }

        #[test]
        fn history_shows_profile_names_and_marks_deleted_ones() {
            let profiles = vec![named("custom-8", "  Notes ")];
            assert_eq!(
                history_profile_label(&profiles, "custom-8", "Deleted profile"),
                "Notes"
            );
            assert_eq!(
                history_profile_label(&profiles, "gone", "Deleted profile"),
                "Deleted profile"
            );
        }

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
        fn manager_layout_keeps_tab_bar_page_disjoint_at_supported_dpi() {
            for dpi in [96, 144, 192] {
                let width = super::scale_for_dpi(980, dpi);
                let height = super::scale_for_dpi(760, dpi);
                let layout = ManagerLayout::for_client(width, height, dpi);
                assert_eq!(layout.tab_bar.left, 0);
                assert_eq!(layout.tab_bar.bottom, layout.page.top);
                assert_eq!(layout.page.right, 980);
                assert_eq!(layout.page.top, super::TAB_BAR_HEIGHT);
                assert_eq!(layout.page.bottom, 760);
                // Status sits inside the tab bar, right of the three tabs.
                assert!(layout.status.left > layout.tabs[2].right);
                assert!(layout.status.top >= layout.tab_bar.top);
                assert!(layout.status.bottom <= layout.tab_bar.bottom);
                for pair in layout.tabs.windows(2) {
                    assert!(pair[0].right < pair[1].left);
                }
            }
        }

        #[test]
        fn manager_layout_tabs_stay_fixed_and_content_fills_page() {
            let compact = ManagerLayout::for_client(980, 760, 96);
            let wide = ManagerLayout::for_client(1240, 820, 96);
            assert_eq!(compact.tabs, wide.tabs);
            assert_eq!(compact.page.left, wide.page.left);
            assert!(wide.page.right > compact.page.right);
            // Content starts at the fixed padding and spans the page.
            let geometry =
                super::page_geometry(super::View::Settings, 1240, 820 - super::TAB_BAR_HEIGHT, 1);
            let card = &geometry.cards[0];
            assert_eq!(card.0.left, 0);
            assert_eq!(card.0.right, 1240 - 2 * super::CONTENT_PADDING);
            // Cards stretch to absorb vertical extra space. The wide window
            // is tall enough that the five-card minimum stack leaves slack;
            // the compact one leaves none.
            let compact_geometry =
                super::page_geometry(super::View::Settings, 980, 760 - super::TAB_BAR_HEIGHT, 1);
            let wide_geometry =
                super::page_geometry(super::View::Settings, 1240, 1100 - super::TAB_BAR_HEIGHT, 1);
            let wide_card = &wide_geometry.cards[0];
            let compact_card = &compact_geometry.cards[0];
            assert!(
                wide_card.0.bottom - wide_card.0.top > compact_card.0.bottom - compact_card.0.top
            );
        }

        #[test]
        fn settings_geometry_matches_hand_computed_default_layout() {
            // 980-wide window: content spans the page minus padding; the
            // four field cards share the vertical slack by field count;
            // fields distribute evenly inside each card.
            let geometry = super::page_geometry(super::View::Settings, 980, 664, 8);
            let tops = [64, 258, 408, 602, 828];
            for (card, top) in geometry.cards.iter().zip(tops) {
                assert_eq!(card.0.left, 0);
                assert_eq!(card.0.right, 916);
                assert_eq!(card.0.top, top);
            }
            assert_eq!(geometry.cards[0].0.bottom, 248);
            assert_eq!(geometry.cards[1].0.bottom, 398);
            assert_eq!(geometry.cards[2].0.bottom, 592);
            assert_eq!(geometry.cards[3].0.bottom, 818);
            // The Profiles order card grows with the row count (8 rows) and
            // pushes the page past the container: the geometry reports the
            // overflow for the page scrollbar.
            assert_eq!(geometry.cards[4].0.bottom, 1156);
            assert_eq!(geometry.scroll_max, 552);
            let slot = |slot: super::Slot| {
                geometry
                    .slots
                    .iter()
                    .find(|(candidate, _)| *candidate == slot)
                    .map(|(_, rect)| *rect)
                    .expect("slot present")
            };
            let endpoint = slot(super::Slot::Endpoint);
            // Slot rects are card-relative; placement adds CONTENT_PADDING.
            assert_eq!(endpoint.left, 12);
            assert_eq!(endpoint.right, 916 - 12);
            assert_eq!(endpoint.top, 64 + 31 + 16);
            assert_eq!(endpoint.bottom, endpoint.top + 28);
            let save = slot(super::Slot::SaveSettings);
            assert_eq!(save.left, 0);
            assert_eq!(save.top, 1180);
            // History geometry: the entries split absorbs the remaining height.
            let history = super::page_geometry(super::View::History, 980, 664, 8);
            let list = history
                .slots
                .iter()
                .find(|(candidate, _)| *candidate == super::Slot::HistoryList)
                .map(|(_, rect)| *rect)
                .expect("list slot");
            assert_eq!(list.left, 12);
            assert_eq!(list.right - list.left, 300);
            assert_eq!(list.bottom, 64 + 576 - 64 - 12);
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
                StatusEvent::SaveThemeFailed {
                    detail: "opaque-error",
                },
                StatusEvent::DeleteProfileConfirm { name: "user text" },
                StatusEvent::ProfileDeleted { name: "user text" },
                StatusEvent::DeleteBlockedLast,
                StatusEvent::DeleteBlockedSelectionDefault,
                StatusEvent::DeleteBlockedHoverDefault,
                StatusEvent::DeleteProfileFailed {
                    detail: "opaque-error",
                },
                StatusEvent::DuplicateProfileName { name: "user text" },
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
                StatusEvent::NewPromptUnsaved,
                StatusEvent::UnsavedPromptDiscarded,
                StatusEvent::PromptInvalid {
                    detail: "opaque-error",
                },
                StatusEvent::InvalidTemperature,
                StatusEvent::InvalidMaxOutputTokens,
                StatusEvent::ConfigPathUnavailable,
                StatusEvent::ConfigExistsNotOverwritten,
                StatusEvent::CredentialStatusPresent,
                StatusEvent::CredentialStatusAbsent,
                StatusEvent::CredentialStatusUnavailable {
                    detail: "opaque-error",
                },
                StatusEvent::HistoryCount { count: 2 },
            ];
            assert!(events.len() >= 50);
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

            assert_eq!(profile_display_name(&first), "Translate");
            assert_eq!(profile_display_name(&second), "解释");
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
        fn defaults_never_write_over_a_config_file_that_appeared() {
            // Config loaded from disk: writes allowed regardless.
            assert!(super::should_write_config(true, false));
            assert!(super::should_write_config(true, true));
            // Config is only defaults: writes allowed only into the void.
            assert!(super::should_write_config(false, false));
            assert!(!super::should_write_config(false, true));
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
    }
}

#[cfg(windows)]
fn main() {
    if let Err(error) = windows_app::run() {
        eprintln!("LingoSnap manager failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {}
