use crate::clipboard::{self, ClipboardEvent};
use crate::model::{ClipboardEntry, ClipboardKind};
use crate::platform;
use crate::storage::Storage;
use crossbeam_channel::{Receiver, Sender, unbounded};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const ACCENT: egui::Color32 = egui::Color32::from_rgb(72, 123, 219);
const ACCENT_SOFT: egui::Color32 = egui::Color32::from_rgb(34, 76, 130);
const PANEL: egui::Color32 = egui::Color32::from_rgb(31, 32, 36);
const CARD_ACTIVE: egui::Color32 = egui::Color32::from_rgb(44, 61, 87);
const TEXT_MUTED: egui::Color32 = egui::Color32::from_rgb(168, 171, 180);
const TIEZ_BG: egui::Color32 = egui::Color32::from_rgb(20, 21, 24);
const TIEZ_HEADER: egui::Color32 = egui::Color32::from_rgb(31, 32, 36);
const TIEZ_ROW: egui::Color32 = egui::Color32::from_rgb(42, 44, 52);
const BORDER_DARK: egui::Color32 = egui::Color32::from_rgb(70, 74, 86);
const MACOS_CARD: egui::Color32 = egui::Color32::from_rgb(44, 46, 54);
const MACOS_CARD_HOVER: egui::Color32 = egui::Color32::from_rgb(56, 59, 68);
const PREFERENCES_KEY: &str = "ui.native_tiez";

struct PendingPaste {
    entry_id: i64,
    prefer_formatted: bool,
    due_at: Instant,
    restore_pinned_window: bool,
}

struct PendingEdgeHide {
    dock: DockMode,
    visible_pos: egui::Pos2,
    target_pos: egui::Pos2,
    restore_size: egui::Vec2,
    target_size: egui::Vec2,
    requested_at: Instant,
    last_attempt: Instant,
    attempts: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum AppPage {
    Clipboard,
    Emoji,
    Settings,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum EmojiTab {
    Emoji,
    Favorites,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum DockMode {
    Off,
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HotkeyTarget {
    Main,
    Sequential,
    RichPaste,
    Search,
}

impl Default for DockMode {
    fn default() -> Self {
        Self::Off
    }
}

impl Default for EmojiTab {
    fn default() -> Self {
        Self::Emoji
    }
}

const EMOJI_GROUPS: &[(&str, &[&str])] = &[
    (
        "常用",
        &[
            "😀", "😁", "😂", "🤣", "😊", "😍", "😘", "😎", "🤔", "😅", "😭", "😡", "👍", "👎",
            "🙏", "👏", "🎉", "🔥", "💯", "✨", "👌", "😴", "🥳", "🤩", "😬", "😇", "🤝", "🙌",
            "😌", "😮", "🥺", "😉",
        ],
    ),
    (
        "表情",
        &[
            "🙂", "😇", "🙃", "😉", "😌", "🤗", "🤩", "🥳", "😴", "😪", "😤", "😱", "🤯", "😵",
            "🤐", "🫠", "🫡", "🫣", "😐", "😑", "😶", "🙄", "😮", "😯", "😲", "🥺", "😢", "😥",
            "😓", "😕", "😟", "😔", "😞", "😖", "😫", "😩",
        ],
    ),
    (
        "人物",
        &[
            "👨‍💻",
            "👩‍💻",
            "👨‍🎨",
            "👩‍🎨",
            "👨‍🚀",
            "👩‍🚀",
            "👨‍🍳",
            "👩‍🍳",
            "👨‍⚕️",
            "👩‍⚕️",
            "👨‍🏫",
            "👩‍🏫",
            "🧑‍💼",
            "🧑‍🔧",
            "🧑‍🎧",
            "🧑‍🚒",
            "👶",
            "🧒",
            "👦",
            "👧",
            "🧑",
            "👱",
            "👴",
            "👵",
        ],
    ),
    (
        "手势",
        &[
            "👌", "✌️", "🤞", "🤟", "🤘", "🤙", "👊", "✊", "🤚", "🖐️", "✋", "👋", "🫶", "👉",
            "👈", "👇", "👆", "🫵", "🤝", "🙌", "🤲", "🤜", "🤛", "🫰",
        ],
    ),
    (
        "符号",
        &[
            "❤️", "🧡", "💛", "💚", "💙", "💜", "🖤", "🤍", "🤎", "💔", "❗", "❓", "✅", "❌",
            "⚠️", "⭕", "💯", "✨", "⭐", "🌟", "🔺", "🔻", "🔸", "🔹",
        ],
    ),
];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
struct AppPreferences {
    show_sensitive: bool,
    show_detail_panel: bool,
    window_pinned: bool,
    compact_rows: bool,
    kind_filter: Option<ClipboardKind>,
    tag_filter: Option<String>,
    emoji_panel_enabled: bool,
    emoji_tab: EmojiTab,
    persistent: bool,
    deduplicate: bool,
    capture_files: bool,
    capture_rich_text: bool,
    delete_after_paste: bool,
    move_to_top_after_paste: bool,
    show_search_box: bool,
    show_app_border: bool,
    arrow_key_selection: bool,
    tag_manager_enabled: bool,
    sound_enabled: bool,
    privacy_protection: bool,
    main_hotkeys: String,
    sequential_hotkey: String,
    rich_paste_hotkey: String,
    search_hotkey: String,
    hide_tray_icon: bool,
    close_to_tray: bool,
    edge_docking: DockMode,
    follow_mouse: bool,
    default_text_app: String,
    default_url_app: String,
    default_code_app: String,
    default_file_app: String,
    default_image_app: String,
    default_video_app: String,
    paste_method: String,
    #[serde(skip)]
    window_level_applied: bool,
}

impl Default for AppPreferences {
    fn default() -> Self {
        Self {
            show_sensitive: false,
            show_detail_panel: false,
            window_pinned: false,
            compact_rows: true,
            kind_filter: None,
            tag_filter: None,
            emoji_panel_enabled: true,
            emoji_tab: EmojiTab::Emoji,
            persistent: true,
            deduplicate: true,
            capture_files: true,
            capture_rich_text: false,
            delete_after_paste: false,
            move_to_top_after_paste: true,
            show_search_box: true,
            show_app_border: true,
            arrow_key_selection: true,
            tag_manager_enabled: true,
            sound_enabled: false,
            privacy_protection: true,
            main_hotkeys: "Alt+C\nSuper+V".to_string(),
            sequential_hotkey: "Alt+V".to_string(),
            rich_paste_hotkey: "Ctrl+Shift+Z".to_string(),
            search_hotkey: "Alt+F".to_string(),
            hide_tray_icon: false,
            close_to_tray: true,
            edge_docking: DockMode::Off,
            follow_mouse: true,
            default_text_app: String::new(),
            default_url_app: String::new(),
            default_code_app: String::new(),
            default_file_app: String::new(),
            default_image_app: String::new(),
            default_video_app: String::new(),
            paste_method: "shift_insert".to_string(),
            window_level_applied: false,
        }
    }
}

pub struct ClipboardApp {
    storage: Storage,
    event_sender: Sender<ClipboardEvent>,
    events: Receiver<ClipboardEvent>,
    entries: Vec<ClipboardEntry>,
    query: String,
    status: String,
    selected_id: Option<i64>,
    tag_editor: String,
    new_tag_input: String,
    focus_search: bool,
    current_page: AppPage,
    show_detail_panel: bool,
    show_sensitive: bool,
    window_pinned: bool,
    compact_rows: bool,
    kind_filter: Option<ClipboardKind>,
    tag_filter: Option<String>,
    emoji_panel_enabled: bool,
    emoji_tab: EmojiTab,
    emoji_favorites: Vec<String>,
    persistent: bool,
    deduplicate: bool,
    capture_files: bool,
    capture_rich_text: bool,
    delete_after_paste: bool,
    move_to_top_after_paste: bool,
    show_search_box: bool,
    show_app_border: bool,
    arrow_key_selection: bool,
    tag_manager_enabled: bool,
    sound_enabled: bool,
    privacy_protection: bool,
    main_hotkeys: String,
    sequential_hotkey: String,
    rich_paste_hotkey: String,
    search_hotkey: String,
    hide_tray_icon: bool,
    close_to_tray: bool,
    edge_docking: DockMode,
    follow_mouse: bool,
    default_text_app: String,
    default_url_app: String,
    default_code_app: String,
    default_file_app: String,
    default_image_app: String,
    default_video_app: String,
    paste_method: String,
    current_database_path: String,
    database_path_input: String,
    text_app_choices: Vec<platform::AppChoice>,
    url_app_choices: Vec<platform::AppChoice>,
    code_app_choices: Vec<platform::AppChoice>,
    file_app_choices: Vec<platform::AppChoice>,
    image_app_choices: Vec<platform::AppChoice>,
    video_app_choices: Vec<platform::AppChoice>,
    recording_hotkey: Option<HotkeyTarget>,
    image_textures: HashMap<i64, egui::TextureHandle>,
    hotkey_handle: platform::HotkeyUpdateHandle,
    tray_handle: Option<platform::TrayHandle>,
    window_level_applied: bool,
    window_visible: bool,
    edge_hidden: bool,
    edge_hide_armed: bool,
    current_edge_dock: DockMode,
    edge_restore_pos: Option<egui::Pos2>,
    edge_restore_size: Option<egui::Vec2>,
    pending_edge_hide: Option<PendingEdgeHide>,
    last_edge_transition: Instant,
    pending_paste: Option<PendingPaste>,
    saved_tags: Vec<String>,
    dev_mode: bool,
    show_dev_panel: bool,
    event_count: u64,
    saved_count: u64,
    error_count: u64,
    frame_count: u64,
    show_inspection: bool,
    show_memory: bool,
    force_quit: bool,
}

impl ClipboardApp {
    pub fn new(cc: &eframe::CreationContext<'_>, storage: Storage, dev_mode: bool) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        configure_fonts(&cc.egui_ctx);
        configure_style(&cc.egui_ctx);

        let (sender, events) = unbounded();
        clipboard::start_watcher(sender.clone());
        let preferences = load_preferences(&storage);
        let hotkey_handle = platform::start_hotkey_listener(
            sender.clone(),
            cc.egui_ctx.clone(),
            hotkey_config_from_preferences(&preferences),
        );
        let tray_handle = platform::start_tray(
            sender.clone(),
            cc.egui_ctx.clone(),
            !preferences.hide_tray_icon,
        );
        let saved_tags = storage.saved_tags().unwrap_or_default();
        let emoji_favorites = load_emoji_favorites(&storage);
        let current_database_path = storage.path().display().to_string();
        let text_app_choices = platform::discover_apps_for_mime("text/plain");
        let url_app_choices = platform::discover_apps_for_mime("x-scheme-handler/http");
        let code_app_choices = platform::discover_apps_for_mime("text/plain");
        let file_app_choices = platform::discover_apps_for_mime("application/octet-stream");
        let image_app_choices = platform::discover_apps_for_mime("image/png");
        let video_app_choices = platform::discover_apps_for_mime("video/mp4");

        let mut app = Self {
            storage,
            event_sender: sender,
            events,
            entries: Vec::new(),
            query: String::new(),
            status: platform::platform_note().to_string(),
            selected_id: None,
            tag_editor: String::new(),
            new_tag_input: String::new(),
            focus_search: false,
            current_page: AppPage::Clipboard,
            show_detail_panel: preferences.show_detail_panel,
            show_sensitive: preferences.show_sensitive,
            window_pinned: preferences.window_pinned,
            compact_rows: preferences.compact_rows,
            kind_filter: preferences.kind_filter,
            tag_filter: preferences.tag_filter,
            emoji_panel_enabled: preferences.emoji_panel_enabled,
            emoji_tab: preferences.emoji_tab,
            emoji_favorites,
            persistent: preferences.persistent,
            deduplicate: preferences.deduplicate,
            capture_files: preferences.capture_files,
            capture_rich_text: preferences.capture_rich_text,
            delete_after_paste: preferences.delete_after_paste,
            move_to_top_after_paste: preferences.move_to_top_after_paste,
            show_search_box: preferences.show_search_box,
            show_app_border: preferences.show_app_border,
            arrow_key_selection: preferences.arrow_key_selection,
            tag_manager_enabled: preferences.tag_manager_enabled,
            sound_enabled: preferences.sound_enabled,
            privacy_protection: preferences.privacy_protection,
            main_hotkeys: preferences.main_hotkeys,
            sequential_hotkey: preferences.sequential_hotkey,
            rich_paste_hotkey: preferences.rich_paste_hotkey,
            search_hotkey: preferences.search_hotkey,
            hide_tray_icon: preferences.hide_tray_icon,
            close_to_tray: preferences.close_to_tray,
            edge_docking: preferences.edge_docking,
            follow_mouse: preferences.follow_mouse,
            default_text_app: preferences.default_text_app,
            default_url_app: preferences.default_url_app,
            default_code_app: preferences.default_code_app,
            default_file_app: preferences.default_file_app,
            default_image_app: preferences.default_image_app,
            default_video_app: preferences.default_video_app,
            paste_method: preferences.paste_method,
            database_path_input: current_database_path.clone(),
            current_database_path,
            text_app_choices,
            url_app_choices,
            code_app_choices,
            file_app_choices,
            image_app_choices,
            video_app_choices,
            recording_hotkey: None,
            image_textures: HashMap::new(),
            hotkey_handle,
            tray_handle,
            window_level_applied: false,
            window_visible: true,
            edge_hidden: false,
            edge_hide_armed: true,
            current_edge_dock: DockMode::Off,
            edge_restore_pos: None,
            edge_restore_size: None,
            pending_edge_hide: None,
            last_edge_transition: Instant::now(),
            pending_paste: None,
            saved_tags,
            dev_mode,
            show_dev_panel: false,
            event_count: 0,
            saved_count: 0,
            error_count: 0,
            frame_count: 0,
            show_inspection: false,
            show_memory: false,
            force_quit: false,
        };
        app.refresh_entries();
        app
    }

    fn refresh_entries(&mut self) {
        let active_tag_filter = self
            .tag_filter
            .as_deref()
            .filter(|_| self.tag_manager_enabled);
        let entries = if self.kind_filter.is_none() && active_tag_filter.is_none() {
            self.storage.list(&self.query)
        } else {
            self.storage
                .list_filtered(&self.query, self.kind_filter.as_ref(), active_tag_filter)
        };
        match entries {
            Ok(entries) => {
                self.entries = entries;
                self.ensure_selection();
            }
            Err(err) => self.status = format!("读取历史失败: {err}"),
        }
    }

    fn ensure_selection(&mut self) {
        let selected_exists = self
            .selected_id
            .is_some_and(|id| self.entries.iter().any(|entry| entry.id == id));
        if !selected_exists {
            self.selected_id = self.entries.first().map(|entry| entry.id);
        }
        self.sync_tag_editor();
    }

    fn selected_entry(&self) -> Option<&ClipboardEntry> {
        self.selected_id
            .and_then(|id| self.entries.iter().find(|entry| entry.id == id))
    }

    fn sync_tag_editor(&mut self) {
        self.tag_editor = self
            .selected_entry()
            .map(|entry| entry.tags.join(", "))
            .unwrap_or_default();
    }

    fn select_entry(&mut self, id: i64) {
        self.selected_id = Some(id);
        self.sync_tag_editor();
    }

    fn drain_events(&mut self, ctx: &egui::Context) {
        let mut changed = false;
        while let Ok(event) = self.events.try_recv() {
            self.event_count += 1;
            match event {
                ClipboardEvent::Captured(entry) => {
                    if matches!(entry.kind, ClipboardKind::File | ClipboardKind::Video)
                        && !self.capture_files
                    {
                        self.status = "已忽略文件剪贴板：设置中已关闭捕获文件".to_string();
                        continue;
                    }
                    if matches!(entry.kind, ClipboardKind::Image)
                        && entry.is_external
                        && !self.capture_files
                    {
                        self.status = "已忽略图片文件剪贴板：设置中已关闭捕获文件".to_string();
                        continue;
                    }
                    if matches!(entry.kind, ClipboardKind::RichText) && !self.capture_rich_text {
                        if let Some(text_entry) = ClipboardEntry::captured_text(
                            entry.content.clone(),
                            entry.source_app.clone(),
                        ) {
                            let result = if self.deduplicate {
                                self.storage.save_entry(&text_entry)
                            } else {
                                self.storage.save_entry_with_dedup(&text_entry, false)
                            };
                            match result {
                                Ok(_) => {
                                    self.saved_count += 1;
                                    self.status = "已按纯文本捕获富文本剪贴板".to_string();
                                    changed = true;
                                }
                                Err(err) => {
                                    self.error_count += 1;
                                    self.status = format!("保存富文本纯文本回退失败: {err}");
                                }
                            }
                        }
                        continue;
                    }
                    let result = if self.deduplicate {
                        self.storage.save_entry(&entry)
                    } else {
                        self.storage.save_entry_with_dedup(&entry, false)
                    };
                    match result {
                        Ok(_) => {
                            self.saved_count += 1;
                            self.status = format!("已捕获：{}", entry.preview);
                            changed = true;
                        }
                        Err(err) => {
                            self.error_count += 1;
                            self.status = format!("保存剪贴板失败: {err}");
                        }
                    }
                }
                ClipboardEvent::Error(err) => {
                    self.error_count += 1;
                    self.status = format!("剪贴板暂不可用: {err}");
                }
                ClipboardEvent::Status(message) => self.status = message,
                ClipboardEvent::ToggleWindow => self.toggle_window_visibility(ctx),
                ClipboardEvent::FocusSearch => self.focus_search_from_hotkey(ctx),
                ClipboardEvent::PasteLatestRich => self.paste_latest_rich(ctx),
                ClipboardEvent::SequentialPaste => self.sequential_paste(ctx),
                ClipboardEvent::OpenSettings => self.open_settings_from_tray(ctx),
                ClipboardEvent::Quit => {
                    self.force_quit = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
        if changed {
            self.refresh_entries();
        }
    }

    fn paste_entry(
        &mut self,
        ctx: &egui::Context,
        entry: &ClipboardEntry,
        paste_with_format: bool,
    ) {
        match clipboard::set_entry(entry, paste_with_format) {
            Ok(()) => {
                let prefer_formatted = paste_with_format
                    || matches!(
                        entry.kind,
                        ClipboardKind::Image | ClipboardKind::File | ClipboardKind::Video
                    );
                self.pending_paste = Some(PendingPaste {
                    entry_id: entry.id,
                    prefer_formatted,
                    due_at: Instant::now() + Duration::from_millis(120),
                    restore_pinned_window: self.window_pinned,
                });
                self.window_visible = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                    egui::WindowLevel::Normal,
                ));
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                self.status = "已写入剪贴板，准备粘贴".to_string();
                ctx.request_repaint_after(Duration::from_millis(130));
            }
            Err(err) => self.status = err,
        }
    }

    fn process_pending_paste(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.pending_paste.as_ref() else {
            return;
        };
        if Instant::now() < pending.due_at {
            ctx.request_repaint_after(pending.due_at.saturating_duration_since(Instant::now()));
            return;
        }

        let pending = self.pending_paste.take().expect("pending paste checked");
        match platform::simulate_paste(
            pending.prefer_formatted,
            platform::PasteMethod::from_str(&self.paste_method),
        ) {
            Ok(()) => {
                let result = if self.delete_after_paste {
                    self.storage.delete(pending.entry_id).map(|_| {
                        self.selected_id = None;
                    })
                } else if self.move_to_top_after_paste {
                    self.storage.mark_used(pending.entry_id)
                } else {
                    self.storage.increment_use_count(pending.entry_id)
                };
                if let Err(err) = result {
                    self.status = format!("已粘贴，但更新历史失败: {err}");
                } else {
                    self.status = if self.delete_after_paste {
                        "已粘贴并删除该记录".to_string()
                    } else {
                        "已粘贴到目标窗口".to_string()
                    };
                }
                if pending.restore_pinned_window {
                    self.show_window(ctx, false);
                    ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                        egui::WindowLevel::AlwaysOnTop,
                    ));
                }
                self.refresh_entries();
            }
            Err(err) => self.status = err,
        }
    }

    fn paste_selected(&mut self, ctx: &egui::Context) {
        if let Some(entry) = self.selected_entry().cloned() {
            self.paste_entry(ctx, &entry, false);
        }
    }

    fn paste_latest_rich(&mut self, ctx: &egui::Context) {
        if let Some(entry) = self.entries.first().cloned() {
            self.select_entry(entry.id);
            self.paste_entry(ctx, &entry, true);
        } else {
            self.status = "没有可富文本粘贴的历史".to_string();
        }
    }

    fn sequential_paste(&mut self, ctx: &egui::Context) {
        let Some(entry) = self
            .selected_entry()
            .cloned()
            .or_else(|| self.entries.first().cloned())
        else {
            self.status = "没有可顺序粘贴的历史".to_string();
            return;
        };
        self.select_entry(entry.id);
        self.paste_entry(ctx, &entry, false);
        self.move_selection(1);
    }

    fn focus_search_from_hotkey(&mut self, ctx: &egui::Context) {
        if !self.window_visible {
            self.show_window(ctx, true);
        } else if self.edge_hidden {
            self.reveal_edge_hidden(ctx, true);
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        self.current_page = AppPage::Clipboard;
        self.focus_search = true;
        self.status = "已通过快捷键聚焦搜索".to_string();
        ctx.request_repaint();
    }

    fn open_settings_from_tray(&mut self, ctx: &egui::Context) {
        self.show_window(ctx, true);
        self.current_page = AppPage::Settings;
        self.status = "已从托盘打开设置".to_string();
        ctx.request_repaint();
    }

    fn update_hotkeys(&mut self) {
        if let Err(err) = self.hotkey_handle.update(self.hotkey_config()) {
            self.status = err;
        }
    }

    fn apply_tray_visibility(&mut self, ctx: &egui::Context) {
        if self.hide_tray_icon {
            if let Some(handle) = self.tray_handle.take() {
                handle.stop();
            }
            self.status = "系统托盘已隐藏".to_string();
        } else if self.tray_handle.is_none() {
            self.tray_handle = platform::start_tray(self.event_sender.clone(), ctx.clone(), true);
            self.status = if self.tray_handle.is_some() {
                "系统托盘已启用".to_string()
            } else {
                "当前平台不支持系统托盘".to_string()
            };
        }
    }

    fn should_close_to_tray(&self) -> bool {
        self.close_to_tray && !self.hide_tray_icon && self.tray_handle.is_some()
    }

    fn close_or_hide_window(&mut self, ctx: &egui::Context) {
        if self.should_close_to_tray() {
            self.hide_window_to_tray(ctx);
        } else {
            self.force_quit = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn hide_window_to_tray(&mut self, ctx: &egui::Context) {
        self.window_visible = false;
        self.edge_hidden = false;
        self.edge_hide_armed = true;
        self.current_edge_dock = DockMode::Off;
        self.edge_restore_pos = None;
        self.edge_restore_size = None;
        self.pending_edge_hide = None;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        self.status = "已隐藏到托盘".to_string();
    }

    fn hotkey_config(&self) -> platform::HotkeyConfig {
        platform::HotkeyConfig {
            main_hotkeys: self.main_hotkeys.clone(),
            sequential_hotkey: self.sequential_hotkey.clone(),
            rich_paste_hotkey: self.rich_paste_hotkey.clone(),
            search_hotkey: self.search_hotkey.clone(),
        }
    }

    fn position_invoked_window(&mut self, ctx: &egui::Context) {
        if self.follow_mouse {
            self.position_near_mouse(ctx);
        }
    }

    fn show_window(&mut self, ctx: &egui::Context, focus: bool) {
        self.window_visible = true;
        self.edge_hide_armed = false;
        self.pending_edge_hide = None;
        if self.edge_hidden {
            self.reveal_edge_hidden(ctx, focus);
            return;
        }
        self.position_invoked_window(ctx);
        self.send_window_level(ctx);
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        if focus {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        self.last_edge_transition = Instant::now();
    }

    fn position_near_mouse(&mut self, ctx: &egui::Context) {
        let Some((mouse_x, mouse_y)) = platform::mouse_position() else {
            return;
        };
        let screen = platform::screen_geometry().unwrap_or(platform::ScreenGeometry {
            x: 0.0,
            y: 0.0,
            width: 1280.0,
            height: 800.0,
        });
        let size = self.viewport_size(ctx);
        let margin = 8.0;
        let gap = 12.0;
        let screen_right = screen.x + screen.width;
        let screen_bottom = screen.y + screen.height;
        let mut x = if mouse_x > screen.x + screen.width / 2.0 {
            mouse_x - size.x - gap
        } else {
            mouse_x + gap
        };
        let mut y = mouse_y + 12.0;
        if y + size.y > screen_bottom - margin {
            y = mouse_y - size.y - 12.0;
        }
        x = x.clamp(
            screen.x + margin,
            (screen_right - size.x - margin).max(screen.x + margin),
        );
        y = y.clamp(
            screen.y + margin,
            (screen_bottom - size.y - margin).max(screen.y + margin),
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
    }

    fn viewport_size(&self, ctx: &egui::Context) -> egui::Vec2 {
        ctx.input(|input| {
            input
                .viewport()
                .outer_rect
                .map(|rect| rect.size())
                .unwrap_or_else(|| egui::vec2(380.0, 680.0))
        })
    }

    fn process_edge_docking(&mut self, ctx: &egui::Context) {
        if self.edge_docking == DockMode::Off {
            if let Some(pending) = self.pending_edge_hide.take() {
                self.restore_from_pending_edge_hide(ctx, pending);
            }
            if self.edge_hidden {
                self.reveal_edge_hidden(ctx, false);
            }
            return;
        }
        self.process_pending_edge_hide(ctx);
        if self.pending_edge_hide.is_some() {
            return;
        }
        if self.last_edge_transition.elapsed() < Duration::from_millis(500) {
            return;
        }
        let screen = platform::screen_geometry().unwrap_or(platform::ScreenGeometry {
            x: 0.0,
            y: 0.0,
            width: 1280.0,
            height: 800.0,
        });
        if self.edge_hidden {
            if self.mouse_near_hidden_edge(screen) {
                self.reveal_edge_hidden(ctx, false);
            }
            return;
        }
        if !self.window_visible || ctx.input(|input| input.pointer.any_down()) {
            return;
        }
        let Some(rect) = self.viewport_rect(ctx) else {
            return;
        };
        let size = self.viewport_size(ctx);
        let dock = self.detect_edge_dock(rect, screen);
        if dock == DockMode::Off {
            self.edge_hide_armed = true;
            return;
        }
        if !self.edge_hide_armed {
            return;
        }
        let visible_pos = self.visible_edge_position(dock, rect, size, screen);
        let (hidden_pos, hidden_size) = hidden_edge_target(dock, visible_pos, size, screen);
        self.edge_hide_armed = false;
        self.last_edge_transition = Instant::now();
        let now = Instant::now();
        self.pending_edge_hide = Some(PendingEdgeHide {
            dock,
            visible_pos,
            target_pos: hidden_pos,
            restore_size: size,
            target_size: hidden_size,
            requested_at: now,
            last_attempt: now,
            attempts: 1,
        });
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
            egui::WindowLevel::AlwaysOnTop,
        ));
        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(1.0, 1.0)));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(hidden_size));
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(hidden_pos));
        self.status = "正在贴边隐藏…".to_string();
    }

    fn process_pending_edge_hide(&mut self, ctx: &egui::Context) {
        let Some(mut pending) = self.pending_edge_hide.take() else {
            return;
        };
        let Some(rect) = self.viewport_rect(ctx) else {
            self.pending_edge_hide = Some(pending);
            return;
        };
        let size_matches = (rect.width() - pending.target_size.x).abs() <= 24.0
            && (rect.height() - pending.target_size.y).abs() <= 24.0;
        if rect.min.distance(pending.target_pos) <= 18.0 && size_matches {
            self.current_edge_dock = pending.dock;
            self.edge_restore_pos = Some(pending.visible_pos);
            self.edge_restore_size = Some(pending.restore_size);
            self.edge_hidden = true;
            self.edge_hide_armed = false;
            self.last_edge_transition = Instant::now();
            self.status = "已贴边隐藏，鼠标靠近屏幕边缘可展开".to_string();
            return;
        }
        if pending.requested_at.elapsed() > Duration::from_secs(2) || pending.attempts >= 8 {
            self.restore_from_pending_edge_hide(ctx, pending);
            self.status = "贴边隐藏未完成：窗口管理器可能阻止窗口调整为边条".to_string();
            return;
        }
        if pending.last_attempt.elapsed() >= Duration::from_millis(150) {
            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(1.0, 1.0)));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(pending.target_size));
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pending.target_pos));
            pending.last_attempt = Instant::now();
            pending.attempts += 1;
        }
        self.pending_edge_hide = Some(pending);
    }

    fn restore_from_pending_edge_hide(&mut self, ctx: &egui::Context, pending: PendingEdgeHide) {
        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(
            320.0, 400.0,
        )));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(pending.restore_size));
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pending.visible_pos));
        self.pending_edge_hide = None;
        self.edge_restore_size = None;
        self.edge_restore_pos = None;
        self.current_edge_dock = DockMode::Off;
        self.edge_hidden = false;
        self.edge_hide_armed = true;
        self.last_edge_transition = Instant::now();
    }

    fn viewport_rect(&self, ctx: &egui::Context) -> Option<egui::Rect> {
        ctx.input(|input| input.viewport().outer_rect)
    }

    fn detect_edge_dock(&self, rect: egui::Rect, screen: platform::ScreenGeometry) -> DockMode {
        let threshold = 5.0;
        if rect.top() <= screen.y + threshold {
            DockMode::Top
        } else if rect.left() <= screen.x + threshold {
            DockMode::Left
        } else if rect.right() >= screen.x + screen.width - threshold {
            DockMode::Right
        } else {
            DockMode::Off
        }
    }

    fn visible_edge_position(
        &self,
        dock: DockMode,
        rect: egui::Rect,
        size: egui::Vec2,
        screen: platform::ScreenGeometry,
    ) -> egui::Pos2 {
        let margin = 8.0;
        match dock {
            DockMode::Left => egui::pos2(
                screen.x,
                rect.top().clamp(
                    screen.y + margin,
                    (screen.y + screen.height - size.y - margin).max(screen.y + margin),
                ),
            ),
            DockMode::Right => egui::pos2(
                (screen.x + screen.width - size.x).max(screen.x),
                rect.top().clamp(
                    screen.y + margin,
                    (screen.y + screen.height - size.y - margin).max(screen.y + margin),
                ),
            ),
            DockMode::Top => egui::pos2(
                rect.left().clamp(
                    screen.x + margin,
                    (screen.x + screen.width - size.x - margin).max(screen.x + margin),
                ),
                screen.y,
            ),
            DockMode::Bottom => rect.min,
            DockMode::Off => rect.min,
        }
    }

    fn mouse_near_hidden_edge(&self, screen: platform::ScreenGeometry) -> bool {
        let Some((x, y)) = platform::mouse_position() else {
            return false;
        };
        let threshold = 6.0;
        match self.current_edge_dock {
            DockMode::Top => y <= screen.y + threshold,
            DockMode::Left => x <= screen.x + threshold,
            DockMode::Right => x >= screen.x + screen.width - threshold,
            DockMode::Bottom => false,
            DockMode::Off => false,
        }
    }

    fn reveal_edge_hidden(&mut self, ctx: &egui::Context, focus: bool) {
        let screen = platform::screen_geometry().unwrap_or(platform::ScreenGeometry {
            x: 0.0,
            y: 0.0,
            width: 1280.0,
            height: 800.0,
        });
        let restore_size = self
            .edge_restore_size
            .unwrap_or_else(|| egui::vec2(380.0, 680.0));
        let pos = self.edge_restore_pos.unwrap_or_else(|| {
            visible_position_for_dock(self.current_edge_dock, restore_size, screen)
        });
        self.edge_hidden = false;
        self.edge_hide_armed = false;
        self.current_edge_dock = DockMode::Off;
        self.edge_restore_pos = None;
        self.edge_restore_size = None;
        self.pending_edge_hide = None;
        self.window_visible = true;
        self.last_edge_transition = Instant::now();
        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(
            320.0, 400.0,
        )));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(restore_size));
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        self.send_window_level(ctx);
        if focus {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        self.status = "已展开贴边窗口".to_string();
    }

    fn open_entry(&mut self, entry: &ClipboardEntry) {
        match self.entry_open_target(entry) {
            Ok(target) => {
                let app = self.default_app_for_kind(&entry.kind).trim().to_string();
                let result = if app.is_empty() {
                    open::that(&target)
                } else {
                    open::with(&target, &app)
                };
                match result {
                    Ok(()) => {
                        let _ = self.storage.increment_use_count(entry.id);
                        self.status = format!("已打开：{target}");
                    }
                    Err(err) => self.status = format!("打开失败: {err}"),
                }
            }
            Err(err) => self.status = err,
        }
    }

    fn default_app_for_kind(&self, kind: &ClipboardKind) -> &str {
        match kind {
            ClipboardKind::Text | ClipboardKind::RichText => &self.default_text_app,
            ClipboardKind::Url => &self.default_url_app,
            ClipboardKind::Code => {
                if self.default_code_app.trim().is_empty() {
                    &self.default_text_app
                } else {
                    &self.default_code_app
                }
            }
            ClipboardKind::File => &self.default_file_app,
            ClipboardKind::Image => &self.default_image_app,
            ClipboardKind::Video => &self.default_video_app,
        }
    }

    fn entry_open_target(&self, entry: &ClipboardEntry) -> Result<String, String> {
        match entry.kind {
            ClipboardKind::Url => Ok(entry.content.trim().to_string()),
            ClipboardKind::File | ClipboardKind::Video => entry
                .content
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .map(ToOwned::to_owned)
                .ok_or_else(|| "文件条目为空".to_string()),
            ClipboardKind::Image if entry.content.starts_with("data:image/") => {
                write_data_url_to_temp_file(&entry.content, "png")
                    .map(|path| path.display().to_string())
            }
            ClipboardKind::Image => entry
                .content
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .map(ToOwned::to_owned)
                .ok_or_else(|| "图片条目为空".to_string()),
            ClipboardKind::Code => write_text_to_temp_file(&entry.content, "txt")
                .map(|path| path.display().to_string()),
            ClipboardKind::Text | ClipboardKind::RichText => {
                write_text_to_temp_file(&entry.content, "txt")
                    .map(|path| path.display().to_string())
            }
        }
    }

    fn toggle_window_visibility(&mut self, ctx: &egui::Context) {
        if !self.window_visible || self.edge_hidden {
            self.show_window(ctx, true);
            self.current_page = AppPage::Clipboard;
            self.status = "Win+V：已呼出剪贴板".to_string();
        } else {
            self.window_visible = false;
            self.edge_hidden = false;
            self.edge_hide_armed = true;
            self.current_edge_dock = DockMode::Off;
            self.edge_restore_pos = None;
            self.pending_edge_hide = None;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            self.status = "Win+V：已隐藏剪贴板".to_string();
        }
        ctx.request_repaint();
    }

    fn handle_native_close_request(&mut self, ctx: &egui::Context) {
        if self.force_quit || !ctx.input(|input| input.viewport().close_requested()) {
            return;
        }
        if self.should_close_to_tray() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.hide_window_to_tray(ctx);
        }
    }

    fn delete_selected(&mut self) {
        if let Some(id) = self.selected_id {
            match self.storage.delete(id) {
                Ok(()) => {
                    self.status = "已删除选中记录".to_string();
                    self.selected_id = None;
                    self.refresh_entries();
                }
                Err(err) => self.status = format!("删除失败: {err}"),
            }
        }
    }

    fn toggle_selected_pin(&mut self) {
        if let Some(id) = self.selected_id {
            match self.storage.toggle_pin(id) {
                Ok(()) => self.refresh_entries(),
                Err(err) => self.status = format!("置顶失败: {err}"),
            }
        }
    }

    fn preferences(&self) -> AppPreferences {
        AppPreferences {
            show_sensitive: self.show_sensitive,
            show_detail_panel: self.show_detail_panel,
            window_pinned: self.window_pinned,
            compact_rows: self.compact_rows,
            kind_filter: self.kind_filter.clone(),
            tag_filter: if self.tag_manager_enabled {
                self.tag_filter.clone()
            } else {
                None
            },
            emoji_panel_enabled: self.emoji_panel_enabled,
            emoji_tab: self.emoji_tab.clone(),
            persistent: self.persistent,
            deduplicate: self.deduplicate,
            capture_files: self.capture_files,
            capture_rich_text: self.capture_rich_text,
            delete_after_paste: self.delete_after_paste,
            move_to_top_after_paste: self.move_to_top_after_paste,
            show_search_box: self.show_search_box,
            show_app_border: self.show_app_border,
            arrow_key_selection: self.arrow_key_selection,
            tag_manager_enabled: self.tag_manager_enabled,
            sound_enabled: self.sound_enabled,
            privacy_protection: self.privacy_protection,
            main_hotkeys: self.main_hotkeys.clone(),
            sequential_hotkey: self.sequential_hotkey.clone(),
            rich_paste_hotkey: self.rich_paste_hotkey.clone(),
            search_hotkey: self.search_hotkey.clone(),
            hide_tray_icon: self.hide_tray_icon,
            close_to_tray: self.close_to_tray,
            edge_docking: self.edge_docking.clone(),
            follow_mouse: self.follow_mouse,
            default_text_app: self.default_text_app.clone(),
            default_url_app: self.default_url_app.clone(),
            default_code_app: self.default_code_app.clone(),
            default_file_app: self.default_file_app.clone(),
            default_image_app: self.default_image_app.clone(),
            default_video_app: self.default_video_app.clone(),
            paste_method: self.paste_method.clone(),
            window_level_applied: false,
        }
    }

    fn send_window_level(&self, ctx: &egui::Context) {
        let level = if self.window_pinned {
            egui::WindowLevel::AlwaysOnTop
        } else {
            egui::WindowLevel::Normal
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
    }

    fn persist_preferences(&mut self) {
        match serde_json::to_string(&self.preferences()) {
            Ok(payload) => match self.storage.set_setting(PREFERENCES_KEY, &payload) {
                Ok(()) => self.status = "设置已保存".to_string(),
                Err(err) => self.status = format!("保存设置失败: {err}"),
            },
            Err(err) => self.status = format!("序列化设置失败: {err}"),
        }
    }

    fn apply_window_level(&mut self, ctx: &egui::Context) {
        self.send_window_level(ctx);
        self.status = if self.window_pinned {
            "窗口已置顶".to_string()
        } else {
            "窗口已取消置顶".to_string()
        };
    }

    fn apply_preferences(&mut self, preferences: AppPreferences, ctx: &egui::Context) {
        self.show_sensitive = preferences.show_sensitive;
        self.show_detail_panel = preferences.show_detail_panel;
        self.window_pinned = preferences.window_pinned;
        self.compact_rows = preferences.compact_rows;
        self.kind_filter = preferences.kind_filter;
        self.tag_filter = if preferences.tag_manager_enabled {
            preferences.tag_filter
        } else {
            None
        };
        self.emoji_panel_enabled = preferences.emoji_panel_enabled;
        self.emoji_tab = preferences.emoji_tab;
        self.persistent = preferences.persistent;
        self.deduplicate = preferences.deduplicate;
        self.capture_files = preferences.capture_files;
        self.capture_rich_text = preferences.capture_rich_text;
        self.delete_after_paste = preferences.delete_after_paste;
        self.move_to_top_after_paste = preferences.move_to_top_after_paste;
        self.show_search_box = preferences.show_search_box;
        self.show_app_border = preferences.show_app_border;
        self.arrow_key_selection = preferences.arrow_key_selection;
        self.tag_manager_enabled = preferences.tag_manager_enabled;
        self.sound_enabled = preferences.sound_enabled;
        self.privacy_protection = preferences.privacy_protection;
        self.main_hotkeys = preferences.main_hotkeys;
        self.sequential_hotkey = preferences.sequential_hotkey;
        self.rich_paste_hotkey = preferences.rich_paste_hotkey;
        self.search_hotkey = preferences.search_hotkey;
        self.hide_tray_icon = preferences.hide_tray_icon;
        self.close_to_tray = preferences.close_to_tray;
        self.edge_docking = preferences.edge_docking;
        self.follow_mouse = preferences.follow_mouse;
        self.default_text_app = preferences.default_text_app;
        self.default_url_app = preferences.default_url_app;
        self.default_code_app = preferences.default_code_app;
        self.default_file_app = preferences.default_file_app;
        self.default_image_app = preferences.default_image_app;
        self.default_video_app = preferences.default_video_app;
        self.paste_method = preferences.paste_method;
        self.apply_window_level(ctx);
        self.update_hotkeys();
        self.apply_tray_visibility(ctx);
        self.position_invoked_window(ctx);
        self.persist_preferences();
        self.refresh_entries();
    }

    fn save_selected_tags(&mut self) {
        if !self.tag_manager_enabled {
            self.status = "标签管理已关闭".to_string();
            return;
        }
        let Some(id) = self.selected_id else {
            return;
        };
        let tags = parse_tags(&self.tag_editor);
        match self.storage.set_tags(id, &tags) {
            Ok(()) => {
                self.status = "标签已保存".to_string();
                self.saved_tags = self.storage.saved_tags().unwrap_or_default();
                self.refresh_entries();
            }
            Err(err) => self.status = format!("保存标签失败: {err}"),
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if self.capture_hotkey_recording(ctx) {
            return;
        }
        let (focus_search, copy, delete, clear, up, down, toggle_sensitive) = ctx.input(|input| {
            (
                input.modifiers.ctrl && input.key_pressed(egui::Key::F),
                input.key_pressed(egui::Key::Enter),
                input.key_pressed(egui::Key::Delete),
                input.key_pressed(egui::Key::Escape),
                input.key_pressed(egui::Key::ArrowUp),
                input.key_pressed(egui::Key::ArrowDown),
                input.modifiers.ctrl && input.key_pressed(egui::Key::H),
            )
        });
        if focus_search {
            self.current_page = AppPage::Clipboard;
            self.focus_search = true;
        }
        if ctx.wants_keyboard_input() {
            return;
        }
        if clear && self.current_page != AppPage::Clipboard {
            self.current_page = AppPage::Clipboard;
            return;
        }
        if copy {
            self.paste_selected(ctx);
        }
        if delete {
            self.delete_selected();
        }
        if clear && !self.query.is_empty() {
            self.query.clear();
            self.refresh_entries();
        }
        if up && self.arrow_key_selection {
            self.move_selection(-1);
        }
        if down && self.arrow_key_selection {
            self.move_selection(1);
        }
        if toggle_sensitive {
            self.show_sensitive = !self.show_sensitive;
            self.persist_preferences();
        }
    }

    fn capture_hotkey_recording(&mut self, ctx: &egui::Context) -> bool {
        let Some(target) = self.recording_hotkey else {
            return false;
        };
        let recorded = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } => hotkey_string_from_key(*key, *modifiers),
                egui::Event::PointerButton {
                    button: egui::PointerButton::Middle,
                    pressed: true,
                    ..
                } => Some("MouseMiddle".to_string()),
                _ => None,
            })
        });
        let Some(recorded) = recorded else {
            return true;
        };
        if recorded == "Escape" {
            self.recording_hotkey = None;
            self.status = "已取消快捷键录制".to_string();
            return true;
        }
        self.apply_recorded_hotkey(target, recorded);
        self.recording_hotkey = None;
        true
    }

    fn apply_recorded_hotkey(&mut self, target: HotkeyTarget, recorded: String) {
        match target {
            HotkeyTarget::Main => {
                let mut hotkeys = self
                    .main_hotkeys
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>();
                if !hotkeys.iter().any(|item| hotkey_equal(item, &recorded)) {
                    hotkeys.push(recorded.clone());
                }
                self.main_hotkeys = hotkeys.join("\n");
            }
            HotkeyTarget::Sequential => self.sequential_hotkey = recorded.clone(),
            HotkeyTarget::RichPaste => self.rich_paste_hotkey = recorded.clone(),
            HotkeyTarget::Search => self.search_hotkey = recorded.clone(),
        }
        self.update_hotkeys();
        self.persist_preferences();
        self.status = format!("已录制快捷键：{recorded}");
    }

    fn add_tag_to_editor(&mut self, tag: &str) {
        if !self.tag_manager_enabled {
            return;
        }
        let mut tags = parse_tags(&self.tag_editor);
        if !tags.iter().any(|existing| existing == tag) {
            tags.push(tag.to_string());
        }
        self.tag_editor = tags.join(", ");
    }

    fn refresh_saved_tags(&mut self) {
        match self.storage.saved_tags() {
            Ok(tags) => self.saved_tags = tags,
            Err(err) => self.status = format!("读取标签目录失败: {err}"),
        }
    }

    fn add_saved_tag_from_input(&mut self) {
        if !self.tag_manager_enabled {
            self.status = "标签管理已关闭".to_string();
            return;
        }
        let tag = self.new_tag_input.trim().to_string();
        if tag.is_empty() {
            self.status = "标签名不能为空".to_string();
            return;
        }
        match self.storage.add_saved_tag(&tag) {
            Ok(()) => {
                self.new_tag_input.clear();
                self.refresh_saved_tags();
                self.status = "标签已加入目录".to_string();
            }
            Err(err) => self.status = format!("新增标签失败: {err}"),
        }
    }

    fn delete_saved_tag(&mut self, tag: &str) {
        if !self.tag_manager_enabled {
            self.status = "标签管理已关闭".to_string();
            return;
        }
        match self.storage.delete_saved_tag(tag) {
            Ok(()) => {
                if self.tag_filter.as_deref() == Some(tag) {
                    self.tag_filter = None;
                    self.persist_preferences();
                }
                self.refresh_saved_tags();
                self.refresh_entries();
                self.status = "标签已从目录移除，已有条目标签不受影响".to_string();
            }
            Err(err) => self.status = format!("删除目录标签失败: {err}"),
        }
    }

    fn set_kind_filter(&mut self, kind: Option<ClipboardKind>) {
        self.kind_filter = kind;
        self.persist_preferences();
        self.refresh_entries();
    }

    fn set_tag_filter(&mut self, tag: Option<String>) {
        if !self.tag_manager_enabled {
            self.tag_filter = None;
            self.persist_preferences();
            self.refresh_entries();
            return;
        }
        self.tag_filter = tag;
        self.persist_preferences();
        self.refresh_entries();
    }

    fn move_selection(&mut self, delta: isize) {
        if self.entries.is_empty() {
            self.selected_id = None;
            return;
        }
        let current = self
            .selected_id
            .and_then(|id| self.entries.iter().position(|entry| entry.id == id))
            .unwrap_or(0);
        let next = (current as isize + delta).clamp(0, self.entries.len() as isize - 1) as usize;
        self.select_entry(self.entries[next].id);
    }

    fn apply_debug_overlays(&self, ctx: &egui::Context) {
        self.suppress_egui_debug_overlays(ctx);
    }

    fn suppress_egui_debug_overlays(&self, ctx: &egui::Context) {
        ctx.options_mut(|options| {
            options.warn_on_id_clash = false;
        });
        #[cfg(debug_assertions)]
        {
            ctx.style_mut(|style| {
                style.debug = Default::default();
            });
        }

        #[cfg(not(debug_assertions))]
        let _ = ctx;
    }

    fn draw_header(&mut self, ctx: &egui::Context) {
        let can_start_drag = self.can_start_window_drag(ctx);
        let app_border = if self.show_app_border {
            egui::Stroke::new(1.0, egui::Color32::from_rgb(78, 82, 96))
        } else {
            egui::Stroke::NONE
        };
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::none()
                    .fill(TIEZ_HEADER)
                    .stroke(app_border)
                    .rounding(egui::Rounding {
                        nw: 16.0,
                        ne: 16.0,
                        sw: 0.0,
                        se: 0.0,
                    })
                    .inner_margin(egui::Margin {
                        left: 14.0,
                        right: 14.0,
                        top: 10.0,
                        bottom: 10.0,
                    }),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if self.current_page != AppPage::Clipboard
                        && toolbar_button(ui, "‹", "返回剪贴板").clicked()
                    {
                        self.current_page = AppPage::Clipboard;
                    }

                    if page_title(
                        ui,
                        match self.current_page {
                            AppPage::Clipboard => "TieZ",
                            AppPage::Emoji => "表情包",
                            AppPage::Settings => "设置",
                        },
                    )
                    .drag_started()
                        && can_start_drag
                    {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }

                    let mut button_count = 2.0; // 关闭 + 置顶
                    if self.current_page == AppPage::Clipboard {
                        button_count += if self.emoji_panel_enabled { 3.0 } else { 2.0 };
                    }
                    if self.dev_mode {
                        button_count += 1.0;
                    }
                    let reserved_for_buttons = button_count * 38.0 + 18.0;
                    let drag_width = (ui.available_width() - reserved_for_buttons).max(18.0);
                    let (drag_rect, drag_response) = ui.allocate_exact_size(
                        egui::vec2(drag_width, 32.0),
                        egui::Sense::click_and_drag(),
                    );
                    if ui.is_rect_visible(drag_rect) {
                        ui.painter().rect_filled(
                            drag_rect,
                            egui::Rounding::same(10.0),
                            egui::Color32::TRANSPARENT,
                        );
                    }
                    if drag_response.drag_started() && can_start_drag {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if toolbar_button(ui, "×", "最小化到任务栏").clicked() {
                            self.close_or_hide_window(ctx);
                        }
                        if self.current_page == AppPage::Clipboard {
                            if toolbar_button(ui, "⚙", "设置").clicked() {
                                self.current_page = AppPage::Settings;
                            }
                            if self.emoji_panel_enabled
                                && toolbar_button(ui, "☺", "表情包").clicked()
                            {
                                self.current_page = AppPage::Emoji;
                            }
                            if toolbar_button(ui, "⌫", "清空非置顶").clicked() {
                                match self.storage.clear_unpinned() {
                                    Ok(()) => {
                                        self.status = "已清空非置顶记录".to_string();
                                        self.refresh_entries();
                                    }
                                    Err(err) => self.status = format!("清空失败: {err}"),
                                }
                            }
                        }
                        let pin_label = if self.window_pinned { "⚐" } else { "⚑" };
                        if toolbar_button(ui, pin_label, "窗口置顶/取消置顶").clicked() {
                            self.window_pinned = !self.window_pinned;
                            self.apply_window_level(ctx);
                            self.persist_preferences();
                        }
                        if self.dev_mode && toolbar_button(ui, "DEV", "开发工具").clicked() {
                            self.show_dev_panel = !self.show_dev_panel;
                        }
                    });
                });

                if self.current_page == AppPage::Clipboard && self.show_search_box {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let search = ui.add_sized(
                            [ui.available_width().min(400.0), 34.0],
                            egui::TextEdit::singleline(&mut self.query)
                                .font(egui::TextStyle::Body)
                                .hint_text("搜索..."),
                        );
                        if self.focus_search {
                            search.request_focus();
                            self.focus_search = false;
                        }
                        if search.changed() {
                            self.refresh_entries();
                        }
                        if !self.query.is_empty() && toolbar_button(ui, "清", "清除搜索").clicked()
                        {
                            self.query.clear();
                            self.refresh_entries();
                        }
                    });
                    ui.add_space(6.0);
                    self.draw_type_filters(ui);
                    if self.tag_manager_enabled && !self.saved_tags.is_empty() {
                        ui.add_space(3.0);
                        self.draw_tag_filters(ui);
                    }
                }
            });
    }

    fn can_start_window_drag(&self, ctx: &egui::Context) -> bool {
        !self.edge_hidden
            && self.pending_edge_hide.is_none()
            && ctx.input(|input| {
                input
                    .viewport()
                    .outer_rect
                    .map(|rect| rect.min.x >= 0.0 && rect.min.y >= 0.0)
                    .unwrap_or(true)
            })
    }

    fn draw_type_filters(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            let all_selected = self.kind_filter.is_none();
            if filter_chip(ui, "全部", all_selected).clicked() {
                self.set_kind_filter(None);
            }
            for kind in ClipboardKind::ALL {
                let selected = self.kind_filter.as_ref() == Some(&kind);
                if filter_chip(ui, kind.label(), selected).clicked() {
                    self.set_kind_filter(Some(kind));
                }
            }
        });
    }

    fn draw_tag_filters(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("标签").color(TEXT_MUTED));
            let all_selected = self.tag_filter.is_none();
            if filter_chip(ui, "全部", all_selected).clicked() {
                self.set_tag_filter(None);
            }
            let tags = self.saved_tags.clone();
            for tag in tags {
                let selected = self.tag_filter.as_ref() == Some(&tag);
                if filter_chip(ui, &tag, selected).clicked() {
                    self.set_tag_filter(Some(tag));
                }
            }
        });
    }

    fn draw_history(&mut self, ui: &mut egui::Ui) {
        if self.entries.is_empty() {
            let filtered = !self.query.trim().is_empty()
                || self.kind_filter.is_some()
                || (self.tag_manager_enabled && self.tag_filter.is_some());
            let (title, description) = if filtered {
                (
                    "没有匹配结果",
                    if self.tag_manager_enabled {
                        "当前搜索、类型或标签过滤没有命中；清除过滤后可查看全部历史。"
                    } else {
                        "当前搜索或类型过滤没有命中；清除过滤后可查看全部历史。"
                    },
                )
            } else {
                (
                    "暂无剪贴板历史",
                    "复制一段文字后，它会以 TieZ 风格卡片显示在这里。",
                )
            };
            empty_state(ui, title, description);
            return;
        }

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let entries = self.entries.clone();
                for entry in entries {
                    self.history_card(ui, &entry);
                    ui.add_space(if self.compact_rows { 2.0 } else { 5.0 });
                }
                if self.show_detail_panel {
                    ui.add_space(8.0);
                    self.draw_detail(ui);
                }
            });
    }

    fn history_card(&mut self, ui: &mut egui::Ui, entry: &ClipboardEntry) {
        let selected = self.selected_id == Some(entry.id);
        let sensitive = self.privacy_protection && entry.is_sensitive();
        let fill = if selected { CARD_ACTIVE } else { MACOS_CARD };
        let stroke = if selected {
            egui::Stroke::new(1.2, ACCENT)
        } else {
            egui::Stroke::new(1.0, BORDER_DARK)
        };

        let response = egui::Frame::none()
            .fill(fill)
            .stroke(stroke)
            .rounding(egui::Rounding::same(12.0))
            .inner_margin(egui::Margin {
                left: 12.0,
                right: 10.0,
                top: if self.compact_rows { 7.0 } else { 9.0 },
                bottom: if self.compact_rows { 6.0 } else { 9.0 },
            })
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if matches!(entry.kind, crate::model::ClipboardKind::Image) {
                        self.draw_image_thumbnail(ui, entry);
                    }
                    ui.vertical(|ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                egui::RichText::new(entry.formatted_time())
                                    .size(10.0)
                                    .strong()
                                    .color(TEXT_MUTED),
                            );
                            kind_badge(ui, entry.kind.label());
                            if entry.is_pinned {
                                ui.label(egui::RichText::new("⚑").size(12.0).color(ACCENT));
                            }
                        });
                        let text = if sensitive && !self.show_sensitive {
                            masked_preview(&entry.preview)
                        } else {
                            row_preview_text(entry)
                        };
                        ui.label(
                            egui::RichText::new(text)
                                .size(if self.compact_rows { 12.5 } else { 13.5 })
                                .monospace()
                                .color(if sensitive && !self.show_sensitive {
                                    egui::Color32::from_rgb(176, 176, 184)
                                } else {
                                    egui::Color32::from_rgb(245, 245, 247)
                                }),
                        );
                        if self.tag_manager_enabled && !entry.tags.is_empty() {
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing.y = 2.0;
                                for tag in &entry.tags {
                                    tag_chip(ui, tag);
                                }
                            });
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if sensitive {
                            sensitive_badge(ui);
                        }
                    });
                });
            })
            .response
            .interact(egui::Sense::click());

        if response.clicked() {
            self.select_entry(entry.id);
            self.paste_entry(ui.ctx(), entry, false);
        }
        if response.secondary_clicked() {
            self.select_entry(entry.id);
            self.paste_entry(ui.ctx(), entry, true);
        }
    }

    fn draw_image_thumbnail(&mut self, ui: &mut egui::Ui, entry: &ClipboardEntry) {
        if let Some(texture) = self.image_texture(ui.ctx(), entry) {
            let size = fit_texture_size(texture.size_vec2(), egui::vec2(68.0, 52.0));
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(29, 31, 37))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(65, 68, 78)))
                .rounding(egui::Rounding::same(10.0))
                .inner_margin(egui::Margin::same(3.0))
                .show(ui, |ui| {
                    ui.add(
                        egui::Image::new((texture.id(), size)).rounding(egui::Rounding::same(8.0)),
                    );
                });
        } else {
            thumbnail_placeholder(ui, "image");
        }
    }

    fn image_texture(
        &mut self,
        ctx: &egui::Context,
        entry: &ClipboardEntry,
    ) -> Option<egui::TextureHandle> {
        if let Some(texture) = self.image_textures.get(&entry.id) {
            return Some(texture.clone());
        }
        let bytes = image_bytes_for_entry(entry)?;
        let image = image::load_from_memory(&bytes).ok()?.to_rgba8();
        let size = [image.width() as usize, image.height() as usize];
        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
        let texture = ctx.load_texture(
            format!("clipboard-image-{}", entry.id),
            color_image,
            egui::TextureOptions::LINEAR,
        );
        self.image_textures.insert(entry.id, texture.clone());
        Some(texture)
    }

    fn draw_detail(&mut self, ui: &mut egui::Ui) {
        let Some(entry) = self.selected_entry().cloned() else {
            empty_state(
                ui,
                "未选择记录",
                "从左侧选择一条历史记录查看完整内容和操作。",
            );
            return;
        };

        ui.horizontal(|ui| {
            ui.heading("详情");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("删除").clicked() {
                    self.delete_selected();
                }
                if ui
                    .button(if entry.is_pinned {
                        "取消置顶"
                    } else {
                        "置顶"
                    })
                    .clicked()
                {
                    self.toggle_selected_pin();
                }
                if ui.button("复制并粘贴").clicked() {
                    self.paste_entry(ui.ctx(), &entry, false);
                }
                if ui.button("打开").clicked() {
                    self.open_entry(&entry);
                }
            });
        });

        ui.add_space(8.0);
        stat_grid(ui, &entry);
        ui.add_space(12.0);

        if self.tag_manager_enabled {
            ui.label(egui::RichText::new("标签").strong());
            ui.horizontal(|ui| {
                let tags = ui.add_sized(
                    [ui.available_width() - 72.0, 32.0],
                    egui::TextEdit::singleline(&mut self.tag_editor)
                        .hint_text("用逗号分隔，例如：工作, 代码, 临时"),
                );
                if tags.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    self.save_selected_tags();
                }
                if ui.button("保存").clicked() {
                    self.save_selected_tags();
                }
            });
            if !self.saved_tags.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("快速标签").color(TEXT_MUTED));
                    let tags = self.saved_tags.clone();
                    for tag in tags {
                        if filter_chip(
                            ui,
                            &tag,
                            parse_tags(&self.tag_editor).iter().any(|t| t == &tag),
                        )
                        .clicked()
                        {
                            self.add_tag_to_editor(&tag);
                        }
                    }
                });
            }
        } else {
            ui.label(egui::RichText::new("标签管理已关闭，可在设置中重新启用。").color(TEXT_MUTED));
        }

        ui.add_space(12.0);
        ui.label(egui::RichText::new("内容").strong());
        let content_is_masked =
            self.privacy_protection && entry.is_sensitive() && !self.show_sensitive;
        let display_content = if content_is_masked {
            masked_preview(&entry.content)
        } else {
            entry.content.clone()
        };
        egui::Frame::none()
            .fill(egui::Color32::from_rgb(22, 25, 30))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(55, 61, 70)))
            .rounding(egui::Rounding::same(10.0))
            .inner_margin(egui::Margin::same(10.0))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_height(ui.available_height())
                    .show(ui, |ui| {
                        if content_is_masked {
                            ui.colored_label(
                                egui::Color32::from_rgb(180, 180, 190),
                                "敏感内容已隐藏。可在设置中临时显示。",
                            );
                            ui.separator();
                        }
                        let mut content = display_content;
                        ui.add(
                            egui::TextEdit::multiline(&mut content)
                                .font(egui::TextStyle::Monospace)
                                .desired_rows(18)
                                .desired_width(f32::INFINITY)
                                .interactive(false),
                        );
                    });
            });
    }

    fn draw_emoji_page(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if filter_chip(ui, "EMOJI", self.emoji_tab == EmojiTab::Emoji).clicked() {
                self.emoji_tab = EmojiTab::Emoji;
                self.persist_preferences();
            }
            if filter_chip(ui, "收藏", self.emoji_tab == EmojiTab::Favorites).clicked() {
                self.emoji_tab = EmojiTab::Favorites;
                self.persist_preferences();
            }
        });
        ui.add_space(10.0);

        match self.emoji_tab {
            EmojiTab::Emoji => {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (group, emojis) in EMOJI_GROUPS {
                            ui.label(egui::RichText::new(*group).size(14.0).strong());
                            ui.separator();
                            ui.horizontal_wrapped(|ui| {
                                for emoji in *emojis {
                                    if emoji_button(ui, emoji).clicked() {
                                        match clipboard::set_text(emoji) {
                                            Ok(()) => {
                                                self.status = format!("已复制表情：{emoji}");
                                            }
                                            Err(err) => self.status = err,
                                        }
                                    }
                                }
                            });
                            ui.add_space(14.0);
                        }
                    });
            }
            EmojiTab::Favorites => {
                if self.emoji_favorites.is_empty() {
                    empty_state(
                        ui,
                        "暂无收藏",
                        "TieZ 收藏表情图会保存在 app.emoji_favorites；原生文件选择/拖拽将在平台能力阶段继续接入。",
                    );
                } else {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let favorites = self.emoji_favorites.clone();
                        for favorite in favorites {
                            if ui.button(&favorite).clicked() {
                                match clipboard::set_text(&favorite) {
                                    Ok(()) => self.status = "已复制收藏表情路径".to_string(),
                                    Err(err) => self.status = err,
                                }
                            }
                        }
                    });
                }
            }
        }
    }

    fn draw_dev_panel(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        if !self.dev_mode || !self.show_dev_panel {
            return;
        }

        egui::Window::new("开发工具")
            .default_width(360.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.label("运行模式：dev");
                if let Some(cpu_usage) = frame.info().cpu_usage {
                    ui.label(format!("CPU/frame：{:.2} ms", cpu_usage * 1000.0));
                } else {
                    ui.label("CPU/frame：采集中");
                }
                ui.label(format!("Frame：{}", self.frame_count));
                ui.label(format!("显示条目：{}", self.entries.len()));
                ui.label(format!("事件总数：{}", self.event_count));
                ui.label(format!("保存成功：{}", self.saved_count));
                ui.label(format!("错误次数：{}", self.error_count));
                ui.label(format!("当前搜索：{}", self.query));
                ui.label(format!("选中 ID：{:?}", self.selected_id));
                ui.separator();
                ui.collapsing("调试覆盖层", |ui| {
                    ui.label("已禁用 egui 红色调试覆盖层与 widget ID 冲突提示，避免污染正常界面。");
                });
                ui.separator();
                ui.checkbox(&mut self.show_inspection, "显示 egui Inspection");
                ui.checkbox(&mut self.show_memory, "显示 egui Memory");
                if self.show_inspection {
                    ui.collapsing("egui Inspection", |ui| ctx.inspection_ui(ui));
                }
                if self.show_memory {
                    ui.collapsing("egui Memory", |ui| ctx.memory_ui(ui));
                }
                ui.separator();
                ui.label("最近状态：");
                ui.monospace(&self.status);
            });
    }

    fn draw_settings_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.label(egui::RichText::new("设置会在切换时自动保存").color(TEXT_MUTED));
            ui.add_space(8.0);

        ui.collapsing("常规设置", |ui| {
            if ui.checkbox(&mut self.emoji_panel_enabled, "启用表情包入口").changed() {
                self.persist_preferences();
            }
            if ui.checkbox(&mut self.tag_manager_enabled, "启用标签管理能力").changed() {
                if !self.tag_manager_enabled {
                    self.tag_filter = None;
                    self.new_tag_input.clear();
                    self.tag_editor.clear();
                    self.refresh_entries();
                }
                self.persist_preferences();
            }
            if ui.checkbox(&mut self.show_search_box, "显示搜索框").changed() {
                self.persist_preferences();
            }
            if ui.checkbox(&mut self.arrow_key_selection, "方向键选择历史").changed() {
                self.persist_preferences();
            }
            if ui.checkbox(&mut self.hide_tray_icon, "隐藏系统托盘图标").changed() {
                self.apply_tray_visibility(ctx);
                self.persist_preferences();
            }
            let can_close_to_tray = !self.hide_tray_icon && self.tray_handle.is_some();
            if ui
                .add_enabled(
                    can_close_to_tray,
                    egui::Checkbox::new(&mut self.close_to_tray, "有托盘时关闭按钮隐藏到托盘"),
                )
                .changed()
            {
                self.persist_preferences();
            }
            ui.add_enabled(false, egui::Checkbox::new(&mut self.sound_enabled, "音效（预留，未接入）"));
        });

        ui.collapsing("快捷键设置", |ui| {
            ui.label(egui::RichText::new("点击“录制”后按键盘组合，或按鼠标中键；Esc 取消。主快捷键可录制多条。" ).color(TEXT_MUTED));
            let main_hotkeys = self.main_hotkeys.clone();
            let sequential_hotkey = self.sequential_hotkey.clone();
            let rich_paste_hotkey = self.rich_paste_hotkey.clone();
            let search_hotkey = self.search_hotkey.clone();
            hotkey_record_row(ui, "主呼出", &main_hotkeys, self.recording_hotkey == Some(HotkeyTarget::Main), |ui| {
                if ui.button("录制新增").clicked() {
                    self.recording_hotkey = Some(HotkeyTarget::Main);
                    self.status = "正在录制主快捷键，可按鼠标中键".to_string();
                }
                if ui.button("清空").clicked() {
                    self.main_hotkeys.clear();
                    self.update_hotkeys();
                    self.persist_preferences();
                }
            });
            hotkey_single_record_row(ui, "顺序粘贴", &sequential_hotkey, self.recording_hotkey == Some(HotkeyTarget::Sequential), || {
                self.recording_hotkey = Some(HotkeyTarget::Sequential);
                self.status = "正在录制顺序粘贴快捷键".to_string();
            });
            hotkey_single_record_row(ui, "富文本粘贴", &rich_paste_hotkey, self.recording_hotkey == Some(HotkeyTarget::RichPaste), || {
                self.recording_hotkey = Some(HotkeyTarget::RichPaste);
                self.status = "正在录制富文本粘贴快捷键".to_string();
            });
            hotkey_single_record_row(ui, "搜索聚焦", &search_hotkey, self.recording_hotkey == Some(HotkeyTarget::Search), || {
                self.recording_hotkey = Some(HotkeyTarget::Search);
                self.status = "正在录制搜索聚焦快捷键".to_string();
            });
        });

        ui.collapsing("剪贴板设置", |ui| {
            ui.add_enabled(false, egui::Checkbox::new(&mut self.persistent, "持久化保存历史（当前固定开启）"));
            if ui.checkbox(&mut self.deduplicate, "去重合并相同内容").changed() {
                self.persist_preferences();
            }
            if ui.checkbox(&mut self.capture_files, "捕获文件剪贴板（路径/URI）").changed() {
                self.persist_preferences();
            }
            if ui.checkbox(&mut self.capture_rich_text, "捕获富文本 HTML").changed() {
                self.persist_preferences();
            }
            if ui.checkbox(&mut self.delete_after_paste, "粘贴后删除").changed() {
                self.persist_preferences();
            }
            if ui.checkbox(&mut self.move_to_top_after_paste, "粘贴后移到顶部").changed() {
                self.persist_preferences();
            }
            egui::ComboBox::from_label("粘贴模拟方式")
                .selected_text(paste_method_label(&self.paste_method))
                .show_ui(ui, |ui| {
                    for (value, label) in [
                        ("shift_insert", "Shift+Insert（文本优先，文件/图片自动 Ctrl+V）"),
                        ("ctrl_v", "Ctrl+V"),
                        ("type_text", "逐字输入（仅文本兜底）"),
                    ] {
                        if ui
                            .selectable_value(&mut self.paste_method, value.to_string(), label)
                            .changed()
                        {
                            self.persist_preferences();
                        }
                    }
                });
            if ui.checkbox(&mut self.privacy_protection, "隐私保护/敏感内容识别").changed() {
                self.persist_preferences();
            }
            ui.label(egui::RichText::new("当前已落地：文本、富文本 HTML、图片、文件剪贴板捕获/写回；粘贴模拟按 TieZ 使用 Shift+Insert/Ctrl+V。" ).color(TEXT_MUTED));
        });

        ui.collapsing("界面设置", |ui| {
            if ui
                .checkbox(&mut self.show_sensitive, "显示敏感内容（Ctrl+H）")
                .changed()
            {
                self.persist_preferences();
            }
            if ui
                .checkbox(&mut self.show_detail_panel, "显示详情/标签侧栏")
                .changed()
            {
                self.persist_preferences();
            }
            if ui
                .checkbox(&mut self.compact_rows, "紧凑历史列表")
                .changed()
            {
                self.persist_preferences();
            }
            if ui.checkbox(&mut self.show_app_border, "显示应用边框").changed() {
                self.persist_preferences();
            }
            if ui.checkbox(&mut self.window_pinned, "窗口置顶").changed() {
                self.apply_window_level(ctx);
                self.persist_preferences();
            }
            if ui.checkbox(&mut self.follow_mouse, "呼出时跟随鼠标位置").changed() {
                self.persist_preferences();
            }
            let mut edge_docking_enabled = self.edge_docking != DockMode::Off;
            if ui.checkbox(&mut edge_docking_enabled, "边缘停靠隐藏").changed() {
                self.edge_docking = if edge_docking_enabled {
                    DockMode::Right
                } else {
                    DockMode::Off
                };
                if self.edge_docking == DockMode::Off && self.edge_hidden {
                    self.reveal_edge_hidden(ctx, false);
                }
                self.persist_preferences();
            }
            ui.label(egui::RichText::new("开启后会按窗口位置自动吸附到左、右、上屏幕边缘，并留下可见边条。" ).color(TEXT_MUTED));
            ui.label("左键点击/Enter：复制并粘贴；右键点击：带格式复制并粘贴；Delete 删除；↑/↓ 切换选中。");
        });

        ui.collapsing("默认打开程序", |ui| {
            ui.label(egui::RichText::new("自动扫描 XDG .desktop 应用；选择“系统默认”时使用 xdg-open/open crate。" ).color(TEXT_MUTED));
            let mut changed = false;
            changed |= app_combo_row(ui, "TEXT", &mut self.default_text_app, &self.text_app_choices);
            changed |= app_combo_row(ui, "URL", &mut self.default_url_app, &self.url_app_choices);
            changed |= app_combo_row(ui, "CODE", &mut self.default_code_app, &self.code_app_choices);
            changed |= app_combo_row(ui, "FILE", &mut self.default_file_app, &self.file_app_choices);
            changed |= app_combo_row(ui, "IMAGE", &mut self.default_image_app, &self.image_app_choices);
            changed |= app_combo_row(ui, "VIDEO", &mut self.default_video_app, &self.video_app_choices);
            if ui.button("重新扫描应用").clicked() {
                self.text_app_choices = platform::discover_apps_for_mime("text/plain");
                self.url_app_choices = platform::discover_apps_for_mime("x-scheme-handler/http");
                self.code_app_choices = platform::discover_apps_for_mime("text/plain");
                self.file_app_choices = platform::discover_apps_for_mime("application/octet-stream");
                self.image_app_choices = platform::discover_apps_for_mime("image/png");
                self.video_app_choices = platform::discover_apps_for_mime("video/mp4");
                self.status = "已重新扫描默认应用列表".to_string();
            }
            if changed {
                self.persist_preferences();
            }
        });

        ui.collapsing("过滤", |ui| {
            ui.label("内容类型过滤会立即影响主列表，并随设置保存。");
            self.draw_type_filters(ui);
            if self.tag_manager_enabled {
                ui.separator();
                ui.label("标签过滤会只显示带有指定标签的记录。");
                self.draw_tag_filters(ui);
            } else {
                ui.label(egui::RichText::new("标签管理关闭时不显示标签过滤。").color(TEXT_MUTED));
            }
        });

        ui.collapsing("标签目录", |ui| {
            if !self.tag_manager_enabled {
                ui.label(egui::RichText::new("标签管理已关闭，目录不会显示或编辑；已有条目标签保留在数据库中。").color(TEXT_MUTED));
                return;
            }
            ui.horizontal(|ui| {
                let response = ui.add_sized(
                    [ui.available_width() - 72.0, 28.0],
                    egui::TextEdit::singleline(&mut self.new_tag_input).hint_text("新增目录标签"),
                );
                let enter = response.lost_focus()
                    && ui.input(|input| input.key_pressed(egui::Key::Enter));
                if ui.button("新增").clicked() || enter {
                    self.add_saved_tag_from_input();
                }
            });
            ui.add_space(6.0);
            if self.saved_tags.is_empty() {
                ui.label("保存条目标签后会自动进入目录。");
            } else {
                ui.label("点击标签可加入当前选中条目的标签编辑框；× 只移出目录，不删除条目上的既有标签。");
                egui::Grid::new("saved_tags_grid")
                    .num_columns(2)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                    let tags = self.saved_tags.clone();
                    for tag in tags {
                        if tag_chip_button(ui, &tag).clicked() {
                            self.add_tag_to_editor(&tag);
                        }
                            if ui.small_button("×").on_hover_text("从目录移除").clicked() {
                                self.delete_saved_tag(&tag);
                            }
                            ui.end_row();
                    }
                    });
            }
        });

        ui.collapsing("数据管理", |ui| {
            ui.label(format!("当前数据库：{}", self.current_database_path));
            ui.horizontal(|ui| {
                ui.label("重启后数据库路径");
                ui.add_sized(
                    [ui.available_width(), 26.0],
                    egui::TextEdit::singleline(&mut self.database_path_input),
                );
            });
            ui.horizontal(|ui| {
                if ui.button("保存数据库位置").clicked() {
                    let path = PathBuf::from(self.database_path_input.trim());
                    match Storage::write_redirect_path(path) {
                        Ok(()) => self.status = "数据库位置已保存，重启后生效".to_string(),
                        Err(err) => self.status = format!("保存数据库位置失败: {err}"),
                    }
                }
                if ui.button("恢复默认位置").clicked() {
                    self.database_path_input = Storage::default_path().display().to_string();
                }
            });
            ui.label(egui::RichText::new("数据库连接已打开，移动位置需要重启；也可用 --db-path 或 MYCLIPBOARD_DB_PATH 临时覆盖。" ).color(TEXT_MUTED));
            if ui.button("清空非置顶历史").clicked() {
                match self.storage.clear_unpinned() {
                    Ok(()) => {
                        self.status = "已清空非置顶记录".to_string();
                        self.refresh_entries();
                    }
                    Err(err) => self.status = format!("清空失败: {err}"),
                }
            }
        });

        ui.collapsing("平台能力", |ui| {
            let capabilities = platform::capabilities();
            ui.label(format!("当前前台窗口：{}", platform::active_app_name()));
            ui.label(format!("活动窗口：{}", capabilities.active_window));
            ui.label(format!("窗口控制：{}", capabilities.window_management));
            ui.label(format!("全局热键：{}", capabilities.global_hotkey));
            ui.label(format!("系统托盘：{}", capabilities.tray));
            ui.label(format!("富剪贴板：{}", capabilities.rich_clipboard));
        });

        ui.collapsing("已对齐", |ui| {
            ui.label("• TieZ 风格顶部品牌栏与圆角工具按钮");
            ui.label("• 单列胶囊历史流、置顶/类型/敏感标签");
            ui.label("• Maple Mono NF CN 字体优先，CJK fallback");
            ui.label("• SQLite 标签、设置表、向前兼容迁移列");
            ui.label("• URL/代码/文件路径/图片数据 URL 类型识别");
        });

        ui.collapsing("待平台能力", |ui| {
            ui.label("• Wayland 全局热键/模拟粘贴需要 portal 或 evdev 后端");
            ui.label("• 加密和高级顺序粘贴队列属于下一阶段");
        });

            ui.add_space(14.0);
            ui.horizontal_centered(|ui| {
                if ui.button("问题反馈").clicked() {
                    match open::that("https://github.com/") {
                        Ok(()) => self.status = "已调用系统默认浏览器".to_string(),
                        Err(err) => self.status = format!("打开浏览器失败: {err}"),
                    }
                }
                if ui.button("恢复初始设置").clicked() {
                    let window_pinned = self.window_pinned;
                    let show_sensitive = self.show_sensitive;
                    let preferences = AppPreferences {
                        window_pinned,
                        show_sensitive,
                        ..AppPreferences::default()
                    };
                    self.apply_preferences(preferences, ctx);
                }
            });
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("TieZ v0.3.1 / MyClipboard v0.1.0").size(15.0).strong());
                ui.label(egui::RichText::new("让复制与粘贴，保持有序").color(TEXT_MUTED));
            });
        });
    }
}

fn load_preferences(storage: &Storage) -> AppPreferences {
    let mut preferences: AppPreferences = storage
        .get_setting(PREFERENCES_KEY)
        .ok()
        .flatten()
        .and_then(|value| serde_json::from_str(&value).ok())
        .unwrap_or_default();
    preferences.persistent = true;
    preferences.sound_enabled = false;
    preferences
}

fn hotkey_config_from_preferences(preferences: &AppPreferences) -> platform::HotkeyConfig {
    platform::HotkeyConfig {
        main_hotkeys: preferences.main_hotkeys.clone(),
        sequential_hotkey: preferences.sequential_hotkey.clone(),
        rich_paste_hotkey: preferences.rich_paste_hotkey.clone(),
        search_hotkey: preferences.search_hotkey.clone(),
    }
}

fn hotkey_record_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &str,
    recording: bool,
    mut actions: impl FnMut(&mut egui::Ui),
) {
    ui.horizontal_wrapped(|ui| {
        ui.label(label);
        let display = if recording {
            "录制中…按键或鼠标中键".to_string()
        } else if value.trim().is_empty() {
            "未设置".to_string()
        } else {
            value.lines().map(str::trim).collect::<Vec<_>>().join(" / ")
        };
        ui.monospace(display);
        actions(ui);
    });
}

fn hotkey_single_record_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &str,
    recording: bool,
    mut start_recording: impl FnMut(),
) {
    hotkey_record_row(ui, label, value, recording, |ui| {
        if ui.button("录制").clicked() {
            start_recording();
        }
    });
}

fn hotkey_string_from_key(key: egui::Key, modifiers: egui::Modifiers) -> Option<String> {
    let key_name = match key {
        egui::Key::Escape => "Escape".to_string(),
        egui::Key::Enter => "Enter".to_string(),
        egui::Key::Tab => "Tab".to_string(),
        egui::Key::Backspace => "Backspace".to_string(),
        egui::Key::Delete => "Delete".to_string(),
        egui::Key::Insert => "Insert".to_string(),
        egui::Key::Home => "Home".to_string(),
        egui::Key::End => "End".to_string(),
        egui::Key::PageUp => "PageUp".to_string(),
        egui::Key::PageDown => "PageDown".to_string(),
        egui::Key::ArrowUp => "Up".to_string(),
        egui::Key::ArrowDown => "Down".to_string(),
        egui::Key::ArrowLeft => "Left".to_string(),
        egui::Key::ArrowRight => "Right".to_string(),
        egui::Key::Space => "Space".to_string(),
        other => format!("{other:?}"),
    };
    if matches!(key, egui::Key::Escape) {
        return Some(key_name);
    }
    if key_name.starts_with("Num") || key_name.starts_with("F") || key_name.len() == 1 {
        let mut parts = Vec::new();
        if modifiers.ctrl {
            parts.push("Ctrl");
        }
        if modifiers.shift {
            parts.push("Shift");
        }
        if modifiers.alt {
            parts.push("Alt");
        }
        if modifiers.mac_cmd {
            parts.push("Super");
        }
        parts.push(&key_name);
        Some(parts.join("+"))
    } else {
        let mut parts = Vec::new();
        if modifiers.ctrl {
            parts.push("Ctrl".to_string());
        }
        if modifiers.shift {
            parts.push("Shift".to_string());
        }
        if modifiers.alt {
            parts.push("Alt".to_string());
        }
        if modifiers.mac_cmd {
            parts.push("Super".to_string());
        }
        parts.push(key_name);
        Some(parts.join("+"))
    }
}

fn hotkey_equal(left: &str, right: &str) -> bool {
    left.split('+')
        .map(|part| part.trim().to_ascii_lowercase())
        .collect::<Vec<_>>()
        == right
            .split('+')
            .map(|part| part.trim().to_ascii_lowercase())
            .collect::<Vec<_>>()
}

fn app_combo_row(
    ui: &mut egui::Ui,
    label: &str,
    selected: &mut String,
    choices: &[platform::AppChoice],
) -> bool {
    let before = selected.clone();
    ui.horizontal(|ui| {
        ui.label(label);
        egui::ComboBox::from_id_source(format!("default_app_{label}"))
            .selected_text(selected_app_label(selected, choices))
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                ui.selectable_value(selected, String::new(), "系统默认");
                for choice in choices {
                    ui.selectable_value(
                        selected,
                        choice.command.clone(),
                        format!("{}  ({})", choice.name, choice.command),
                    );
                }
            });
    });
    *selected != before
}

fn selected_app_label(selected: &str, choices: &[platform::AppChoice]) -> String {
    if selected.trim().is_empty() {
        return "系统默认".to_string();
    }
    choices
        .iter()
        .find(|choice| choice.command == selected)
        .map(|choice| choice.name.clone())
        .unwrap_or_else(|| selected.to_string())
}

fn paste_method_label(value: &str) -> &'static str {
    match value {
        "ctrl_v" => "Ctrl+V",
        "type_text" => "逐字输入",
        _ => "Shift+Insert",
    }
}

fn write_text_to_temp_file(content: &str, extension: &str) -> Result<PathBuf, String> {
    let dir = temp_open_dir()?;
    let path = dir.join(format!(
        "myclipboard-open-{}.{}",
        timestamp_millis(),
        extension
    ));
    fs::write(&path, content).map_err(|err| format!("写入临时文件失败: {err}"))?;
    Ok(path)
}

fn write_data_url_to_temp_file(content: &str, extension: &str) -> Result<PathBuf, String> {
    let (_, data) = content
        .split_once(',')
        .ok_or_else(|| "图片数据 URL 格式无效".to_string())?;
    let bytes = decode_base64(data)?;
    let dir = temp_open_dir()?;
    let path = dir.join(format!(
        "myclipboard-open-{}.{}",
        timestamp_millis(),
        extension
    ));
    fs::write(&path, bytes).map_err(|err| format!("写入临时图片失败: {err}"))?;
    Ok(path)
}

fn temp_open_dir() -> Result<PathBuf, String> {
    let base = dirs::cache_dir().unwrap_or_else(std::env::temp_dir);
    let dir = base.join("myclipboard").join("open");
    fs::create_dir_all(&dir).map_err(|err| format!("创建临时目录失败: {err}"))?;
    Ok(dir)
}

fn hidden_edge_target(
    dock: DockMode,
    visible_pos: egui::Pos2,
    size: egui::Vec2,
    screen: platform::ScreenGeometry,
) -> (egui::Pos2, egui::Vec2) {
    let sliver = 8.0;
    match dock {
        DockMode::Left => (
            egui::pos2(screen.x.max(0.0), visible_pos.y.max(0.0)),
            egui::vec2(sliver, size.y),
        ),
        DockMode::Right => (
            egui::pos2(
                (screen.x + screen.width - sliver).max(screen.x).max(0.0),
                visible_pos.y.max(0.0),
            ),
            egui::vec2(sliver, size.y),
        ),
        DockMode::Top => (
            egui::pos2(visible_pos.x.max(0.0), screen.y.max(0.0)),
            egui::vec2(size.x, sliver),
        ),
        DockMode::Bottom | DockMode::Off => (visible_pos, size),
    }
}

fn visible_position_for_dock(
    dock: DockMode,
    size: egui::Vec2,
    screen: platform::ScreenGeometry,
) -> egui::Pos2 {
    match dock {
        DockMode::Left => egui::pos2(
            screen.x,
            screen.y + ((screen.height - size.y) / 2.0).max(0.0),
        ),
        DockMode::Right => egui::pos2(
            (screen.x + screen.width - size.x).max(screen.x),
            screen.y + ((screen.height - size.y) / 2.0).max(0.0),
        ),
        DockMode::Top => egui::pos2(
            screen.x + ((screen.width - size.x) / 2.0).max(0.0),
            screen.y,
        ),
        DockMode::Bottom => egui::pos2(
            screen.x + ((screen.width - size.x) / 2.0).max(0.0),
            (screen.y + screen.height - size.y).max(screen.y),
        ),
        DockMode::Off => egui::pos2(
            screen.x + ((screen.width - size.x) / 2.0).max(0.0),
            screen.y + ((screen.height - size.y) / 2.0).max(0.0),
        ),
    }
}

fn timestamp_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bits = 0u8;
    for byte in input.bytes().filter(|byte| !byte.is_ascii_whitespace()) {
        if byte == b'=' {
            break;
        }
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err("图片 base64 数据无效".to_string()),
        } as u32;
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

fn load_emoji_favorites(storage: &Storage) -> Vec<String> {
    storage
        .get_setting("app.emoji_favorites")
        .ok()
        .flatten()
        .and_then(|value| serde_json::from_str(&value).ok())
        .unwrap_or_default()
}

fn page_title(ui: &mut egui::Ui, title: &str) -> egui::Response {
    egui::Frame::none()
        .fill(egui::Color32::from_rgb(245, 245, 247))
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin {
            left: 10.0,
            right: 10.0,
            top: 4.0,
            bottom: 4.0,
        })
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(title)
                    .size(14.0)
                    .strong()
                    .color(egui::Color32::from_rgb(24, 25, 28)),
            );
        })
        .response
        .interact(egui::Sense::click_and_drag())
}

impl eframe::App for ClipboardApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.frame_count += 1;
        self.suppress_egui_debug_overlays(ctx);
        self.handle_native_close_request(ctx);
        if !self.window_level_applied {
            self.send_window_level(ctx);
            self.window_level_applied = true;
        }
        self.apply_debug_overlays(ctx);
        self.handle_shortcuts(ctx);
        self.drain_events(ctx);
        self.process_pending_paste(ctx);
        self.process_edge_docking(ctx);

        self.draw_header(ctx);
        let app_border = if self.show_app_border {
            egui::Stroke::new(1.0, egui::Color32::from_rgb(78, 82, 96))
        } else {
            egui::Stroke::NONE
        };

        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::none()
                    .fill(PANEL)
                    .stroke(app_border)
                    .rounding(egui::Rounding {
                        nw: 0.0,
                        ne: 0.0,
                        sw: 16.0,
                        se: 16.0,
                    })
                    .inner_margin(egui::Margin {
                        left: 18.0,
                        right: 18.0,
                        top: 6.0,
                        bottom: 6.0,
                    }),
            )
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new(&self.status).color(TEXT_MUTED));
                    ui.separator();
                    ui.label(
                        egui::RichText::new(format!("显示 {} 条", self.entries.len()))
                            .color(TEXT_MUTED),
                    );
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(TIEZ_BG).stroke(app_border))
            .show(ctx, |ui| {
                egui::Frame::none()
                    .inner_margin(egui::Margin {
                        left: 10.0,
                        right: 10.0,
                        top: 10.0,
                        bottom: 10.0,
                    })
                    .show(ui, |ui| match self.current_page {
                        AppPage::Clipboard => self.draw_history(ui),
                        AppPage::Emoji => self.draw_emoji_page(ui),
                        AppPage::Settings => self.draw_settings_panel(ui, ctx),
                    });
            });

        self.draw_dev_panel(ctx, frame);
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
    }
}

fn configure_style(ctx: &egui::Context) {
    ctx.options_mut(|options| {
        options.warn_on_id_clash = false;
    });
    ctx.set_visuals(egui::Visuals::dark());
    ctx.style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 7.0);
        style.spacing.button_padding = egui::vec2(8.0, 5.0);
        style.spacing.window_margin = egui::Margin::same(12.0);
        style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(52, 54, 62);
        style.visuals.widgets.hovered.bg_fill = MACOS_CARD_HOVER;
        style.visuals.widgets.active.bg_fill = ACCENT;
        style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, BORDER_DARK);
        style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, ACCENT);
        style.visuals.selection.bg_fill = ACCENT_SOFT;
        style.visuals.window_fill = TIEZ_HEADER;
        style.visuals.panel_fill = TIEZ_BG;
        style.visuals.extreme_bg_color = egui::Color32::from_rgb(18, 19, 22);
        style.visuals.override_text_color = Some(egui::Color32::from_rgb(245, 245, 247));
    });
}

fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    if let Some((name, bytes)) = load_cjk_font() {
        fonts
            .font_data
            .insert(name.clone(), egui::FontData::from_owned(bytes));
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, name.clone());
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .insert(0, name);
    }
    ctx.set_fonts(fonts);
}

fn load_cjk_font() -> Option<(String, Vec<u8>)> {
    let candidates = [
        "/usr/share/fonts/truetype/MapleMono-NF-CN-unhinted/MapleMono-NF-CN-Regular.ttf",
        "/usr/share/fonts/truetype/MapleMono-NF-CN/MapleMono-NF-CN-Regular.ttf",
        "/usr/share/fonts/TTF/MapleMono-NF-CN-Regular.ttf",
        "/usr/local/share/fonts/MapleMono-NF-CN-Regular.ttf",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJKsc-Regular.otf",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJKsc-Regular.otf",
        "/usr/share/fonts/opentype/adobe-source-han-sans/SourceHanSansCN-Regular.otf",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
        "/usr/share/fonts/truetype/arphic/uming.ttc",
    ];

    candidates
        .iter()
        .find_map(|path| read_font_path(path))
        .or_else(load_maple_mono_via_fontconfig)
}

fn read_font_path(path: &str) -> Option<(String, Vec<u8>)> {
    fs::read(path)
        .ok()
        .map(|bytes| (font_name_from_path(path), bytes))
}

#[cfg(target_os = "linux")]
fn load_maple_mono_via_fontconfig() -> Option<(String, Vec<u8>)> {
    let output = Command::new("fc-match")
        .args(["-f", "%{file}", "Maple Mono NF CN"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let path = String::from_utf8(output.stdout).ok()?;
    let path = path.trim();
    if path.is_empty() {
        return None;
    }

    read_font_path(path)
}

#[cfg(not(target_os = "linux"))]
fn load_maple_mono_via_fontconfig() -> Option<(String, Vec<u8>)> {
    None
}

fn font_name_from_path(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("cjk-fallback")
        .to_string()
}

fn parse_tags(value: &str) -> Vec<String> {
    value
        .split([',', '，', ';', '；'])
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn tag_chip(ui: &mut egui::Ui, tag: &str) {
    egui::Frame::none()
        .fill(ACCENT_SOFT)
        .rounding(egui::Rounding::same(99.0))
        .inner_margin(egui::Margin {
            left: 5.0,
            right: 5.0,
            top: 1.0,
            bottom: 1.0,
        })
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(tag)
                    .size(10.0)
                    .color(egui::Color32::WHITE),
            );
        });
}

fn filter_chip(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let fill = if selected {
        ACCENT_SOFT
    } else {
        egui::Color32::from_rgb(50, 54, 63)
    };
    let stroke = if selected {
        egui::Stroke::new(1.2, ACCENT)
    } else {
        egui::Stroke::new(1.0, BORDER_DARK)
    };
    ui.add(
        egui::Button::new(egui::RichText::new(label).size(10.5).color(if selected {
            egui::Color32::WHITE
        } else {
            TEXT_MUTED
        }))
        .fill(fill)
        .stroke(stroke)
        .rounding(egui::Rounding::same(99.0))
        .min_size(egui::vec2(0.0, 20.0)),
    )
}

fn tag_chip_button(ui: &mut egui::Ui, tag: &str) -> egui::Response {
    ui.add(
        egui::Button::new(
            egui::RichText::new(tag)
                .size(10.5)
                .color(egui::Color32::WHITE),
        )
        .fill(ACCENT_SOFT)
        .stroke(egui::Stroke::new(1.0, ACCENT))
        .rounding(egui::Rounding::same(99.0))
        .min_size(egui::vec2(0.0, 20.0)),
    )
}

fn emoji_button(ui: &mut egui::Ui, emoji: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(emoji).size(20.0))
            .fill(TIEZ_ROW)
            .stroke(egui::Stroke::new(1.0, BORDER_DARK))
            .rounding(egui::Rounding::same(10.0))
            .min_size(egui::vec2(34.0, 34.0)),
    )
}

fn toolbar_button(ui: &mut egui::Ui, label: &str, tooltip: &str) -> egui::Response {
    let response = if let Some(icon) = ToolbarIcon::from_label(label) {
        vector_toolbar_button(ui, icon)
    } else {
        ui.add(
            egui::Button::new(egui::RichText::new(label).size(16.0))
                .min_size(egui::vec2(32.0, 32.0))
                .fill(CARD_ACTIVE)
                .stroke(egui::Stroke::new(2.0, egui::Color32::from_rgb(88, 91, 103)))
                .rounding(egui::Rounding::same(10.0)),
        )
    };
    response.on_hover_text(tooltip)
}

#[derive(Clone, Copy)]
enum ToolbarIcon {
    Back,
    Close,
    Settings,
    Emoji,
    Clear,
    Pin,
    Unpin,
    Dev,
}

impl ToolbarIcon {
    fn from_label(label: &str) -> Option<Self> {
        match label {
            "‹" => Some(Self::Back),
            "×" => Some(Self::Close),
            "⚙" => Some(Self::Settings),
            "☺" => Some(Self::Emoji),
            "⌫" | "清" => Some(Self::Clear),
            "📌" | "⚐" => Some(Self::Pin),
            "📍" | "⚑" => Some(Self::Unpin),
            "DEV" => Some(Self::Dev),
            _ => None,
        }
    }
}

fn vector_toolbar_button(ui: &mut egui::Ui, icon: ToolbarIcon) -> egui::Response {
    let desired_size = egui::vec2(34.0, 34.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let fill = if response.hovered() {
            egui::Color32::from_rgb(55, 58, 66)
        } else {
            egui::Color32::TRANSPARENT
        };
        ui.painter().rect(
            rect,
            egui::Rounding::same(10.0),
            fill,
            egui::Stroke::new(1.0, egui::Color32::from_rgb(76, 80, 92)),
        );
        paint_toolbar_icon(
            ui.painter(),
            rect.shrink(7.0),
            icon,
            egui::Color32::from_rgb(245, 245, 247),
        );
    }
    response
}

fn paint_toolbar_icon(
    painter: &egui::Painter,
    rect: egui::Rect,
    icon: ToolbarIcon,
    color: egui::Color32,
) {
    let stroke = egui::Stroke::new(2.2, color);
    let c = rect.center();
    let l = rect.left();
    let r = rect.right();
    let t = rect.top();
    let b = rect.bottom();
    match icon {
        ToolbarIcon::Back => {
            painter.line_segment(
                [egui::pos2(r - 2.0, t + 1.5), egui::pos2(l + 4.0, c.y)],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(l + 4.0, c.y), egui::pos2(r - 2.0, b - 1.5)],
                stroke,
            );
        }
        ToolbarIcon::Close => {
            painter.line_segment(
                [egui::pos2(l + 3.0, t + 3.0), egui::pos2(r - 3.0, b - 3.0)],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(r - 3.0, t + 3.0), egui::pos2(l + 3.0, b - 3.0)],
                stroke,
            );
        }
        ToolbarIcon::Settings => {
            painter.circle_stroke(c, 5.0, stroke);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let inner = c + egui::vec2(a.cos() * 8.0, a.sin() * 8.0);
                let outer = c + egui::vec2(a.cos() * 10.5, a.sin() * 10.5);
                painter.line_segment([inner, outer], stroke);
            }
        }
        ToolbarIcon::Emoji => {
            painter.circle_stroke(c, 10.0, stroke);
            painter.circle_filled(egui::pos2(c.x - 4.0, c.y - 3.0), 1.6, color);
            painter.circle_filled(egui::pos2(c.x + 4.0, c.y - 3.0), 1.6, color);
            painter.line_segment(
                [
                    egui::pos2(c.x - 4.0, c.y + 4.0),
                    egui::pos2(c.x + 4.0, c.y + 4.0),
                ],
                stroke,
            );
        }
        ToolbarIcon::Clear => {
            painter.line_segment(
                [egui::pos2(l + 4.0, t + 6.0), egui::pos2(r - 4.0, t + 6.0)],
                stroke,
            );
            painter.rect_stroke(
                egui::Rect::from_min_max(
                    egui::pos2(l + 6.0, t + 8.0),
                    egui::pos2(r - 6.0, b - 3.0),
                ),
                egui::Rounding::same(2.0),
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x - 3.0, t + 3.0),
                    egui::pos2(c.x + 3.0, t + 3.0),
                ],
                stroke,
            );
        }
        ToolbarIcon::Pin | ToolbarIcon::Unpin => {
            painter.line_segment([egui::pos2(c.x, t + 3.0), egui::pos2(c.x, b - 4.0)], stroke);
            painter.line_segment(
                [
                    egui::pos2(l + 5.0, c.y - 3.0),
                    egui::pos2(r - 5.0, c.y - 3.0),
                ],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(l + 8.0, t + 4.0), egui::pos2(r - 8.0, t + 4.0)],
                stroke,
            );
            if matches!(icon, ToolbarIcon::Unpin) {
                painter.line_segment(
                    [egui::pos2(l + 3.0, b - 3.0), egui::pos2(r - 3.0, t + 3.0)],
                    stroke,
                );
            }
        }
        ToolbarIcon::Dev => {
            painter.line_segment(
                [egui::pos2(l + 4.0, c.y), egui::pos2(l + 9.0, t + 5.0)],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(l + 4.0, c.y), egui::pos2(l + 9.0, b - 5.0)],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(r - 4.0, c.y), egui::pos2(r - 9.0, t + 5.0)],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(r - 4.0, c.y), egui::pos2(r - 9.0, b - 5.0)],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x + 2.0, t + 3.0),
                    egui::pos2(c.x - 2.0, b - 3.0),
                ],
                stroke,
            );
        }
    }
}

fn kind_badge(ui: &mut egui::Ui, label: &str) {
    egui::Frame::none()
        .fill(egui::Color32::from_rgb(55, 58, 68))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(78, 82, 94)))
        .rounding(egui::Rounding::same(99.0))
        .inner_margin(egui::Margin {
            left: 7.0,
            right: 7.0,
            top: 3.0,
            bottom: 3.0,
        })
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(label)
                    .size(11.0)
                    .italics()
                    .color(TEXT_MUTED),
            );
        });
}

fn sensitive_badge(ui: &mut egui::Ui) {
    egui::Frame::none()
        .fill(egui::Color32::from_rgb(73, 72, 223))
        .stroke(egui::Stroke::new(
            1.0,
            egui::Color32::from_rgb(106, 102, 255),
        ))
        .rounding(egui::Rounding::same(99.0))
        .inner_margin(egui::Margin {
            left: 8.0,
            right: 8.0,
            top: 3.0,
            bottom: 3.0,
        })
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("sensitive")
                    .size(13.0)
                    .strong()
                    .color(egui::Color32::WHITE),
            );
        });
}

fn thumbnail_placeholder(ui: &mut egui::Ui, label: &str) {
    egui::Frame::none()
        .fill(egui::Color32::from_rgb(29, 31, 37))
        .stroke(egui::Stroke::new(2.0, egui::Color32::from_rgb(65, 68, 78)))
        .rounding(egui::Rounding::same(12.0))
        .inner_margin(egui::Margin::same(10.0))
        .show(ui, |ui| {
            ui.set_min_size(egui::vec2(64.0, 46.0));
            ui.centered_and_justified(|ui| {
                ui.label(egui::RichText::new(label).size(12.0).color(TEXT_MUTED));
            });
        });
}

fn row_preview_text(entry: &ClipboardEntry) -> String {
    match entry.kind {
        ClipboardKind::File | ClipboardKind::Video => file_preview_text(&entry.content),
        ClipboardKind::Image if !entry.content.starts_with("data:image/") => {
            file_preview_text(&entry.content)
        }
        _ => entry.preview.clone(),
    }
}

fn file_preview_text(content: &str) -> String {
    let paths = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    match paths.as_slice() {
        [] => "空文件条目".to_string(),
        [path] => {
            let path = Path::new(path);
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_else(|| path.to_str().unwrap_or("文件"));
            let dir = path.parent().and_then(|value| value.to_str()).unwrap_or("");
            if dir.is_empty() {
                name.to_string()
            } else {
                format!("{name}\n{dir}")
            }
        }
        many => {
            let first = Path::new(many[0])
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or(many[0]);
            format!("{} 个文件 · {first} …", many.len())
        }
    }
}

fn fit_texture_size(size: egui::Vec2, max: egui::Vec2) -> egui::Vec2 {
    if size.x <= 0.0 || size.y <= 0.0 {
        return max;
    }
    let scale = (max.x / size.x).min(max.y / size.y).min(1.0);
    egui::vec2((size.x * scale).max(20.0), (size.y * scale).max(20.0))
}

fn image_bytes_for_entry(entry: &ClipboardEntry) -> Option<Vec<u8>> {
    if entry.content.starts_with("data:image/") {
        return decode_base64(entry.content.split_once(',')?.1).ok();
    }
    let path = entry
        .content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    fs::read(path).ok()
}

fn masked_preview(value: &str) -> String {
    let chars = value.chars().count();
    let prefix = value.chars().take(4).collect::<String>();
    format!("{prefix}...  ({chars} 字符)")
}

fn stat_grid(ui: &mut egui::Ui, entry: &ClipboardEntry) {
    egui::Grid::new("entry_stats")
        .num_columns(2)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            muted(ui, "来源");
            ui.label(&entry.source_app);
            ui.end_row();
            muted(ui, "时间");
            ui.label(entry.formatted_time());
            ui.end_row();
            muted(ui, "使用次数");
            ui.label(entry.use_count.to_string());
            ui.end_row();
            muted(ui, "字符数");
            ui.label(entry.content.chars().count().to_string());
            ui.end_row();
            muted(ui, "状态");
            ui.label(if entry.is_pinned {
                "已置顶"
            } else {
                "普通"
            });
            ui.end_row();
        });
}

fn muted(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).color(TEXT_MUTED));
}

fn empty_state(ui: &mut egui::Ui, title: &str, body: &str) {
    ui.vertical_centered_justified(|ui| {
        ui.add_space(80.0);
        ui.label(egui::RichText::new(title).size(18.0).strong());
        ui.label(egui::RichText::new(body).color(TEXT_MUTED));
    });
}
