#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct PlatformCapabilities {
    pub active_window: &'static str,
    pub window_management: &'static str,
    pub global_hotkey: &'static str,
    pub tray: &'static str,
    pub rich_clipboard: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenGeometry {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HotkeyConfig {
    pub main_hotkeys: String,
    pub sequential_hotkey: String,
    pub rich_paste_hotkey: String,
    pub search_hotkey: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyboardModifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub super_key: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteMethod {
    Auto,
    ShiftInsert,
    CtrlV,
    TypeText,
}

impl PasteMethod {
    pub fn from_str(value: &str) -> Self {
        match value {
            "shift_insert" => PasteMethod::ShiftInsert,
            "ctrl_v" => PasteMethod::CtrlV,
            "type_text" => PasteMethod::TypeText,
            _ => PasteMethod::Auto,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppChoice {
    pub name: String,
    pub command: String,
    pub is_default: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotkeyAction {
    ToggleWindow,
    SequentialPaste,
    RichPaste,
    FocusSearch,
}

#[derive(Clone)]
pub struct HotkeyUpdateHandle {
    sender: crossbeam_channel::Sender<HotkeyConfig>,
}

pub struct TrayHandle {
    stop: Option<Box<dyn FnOnce() + Send>>,
}

impl TrayHandle {
    pub fn new(stop: impl FnOnce() + Send + 'static) -> Self {
        Self {
            stop: Some(Box::new(stop)),
        }
    }

    pub fn stop(mut self) {
        if let Some(stop) = self.stop.take() {
            stop();
        }
    }
}

impl HotkeyUpdateHandle {
    pub fn new(sender: crossbeam_channel::Sender<HotkeyConfig>) -> Self {
        Self { sender }
    }

    pub fn update(&self, config: HotkeyConfig) -> Result<(), String> {
        self.sender
            .send(config)
            .map_err(|err| format!("更新快捷键失败: {err}"))
    }
}

#[cfg(target_os = "linux")]
pub use linux::current_keyboard_modifiers;
#[cfg(target_os = "linux")]
pub use linux::validate_hotkey;
#[cfg(target_os = "linux")]
pub use linux::{
    active_app_name, discover_apps_for_mime, platform_note, simulate_paste, start_hotkey_listener,
};
#[cfg(target_os = "linux")]
pub use linux::{autostart_enabled, set_autostart};
#[cfg(target_os = "linux")]
pub use linux::{mouse_position, screen_geometry, start_tray};
#[cfg(target_os = "windows")]
pub use windows::current_keyboard_modifiers;
#[cfg(target_os = "windows")]
pub use windows::validate_hotkey;
#[cfg(target_os = "windows")]
pub use windows::{
    active_app_name, discover_apps_for_mime, platform_note, simulate_paste, start_hotkey_listener,
};
#[cfg(target_os = "windows")]
pub use windows::{autostart_enabled, set_autostart};
#[cfg(target_os = "windows")]
pub use windows::{mouse_position, screen_geometry, start_tray};

#[cfg(target_os = "linux")]
#[allow(dead_code)]
pub fn capabilities() -> PlatformCapabilities {
    linux::capabilities()
}

#[cfg(target_os = "windows")]
#[allow(dead_code)]
pub fn capabilities() -> PlatformCapabilities {
    windows::capabilities()
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn active_app_name() -> String {
    "Unknown".to_string()
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn platform_note() -> &'static str {
    "当前平台使用通用实现。"
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
#[allow(dead_code)]
pub fn capabilities() -> PlatformCapabilities {
    PlatformCapabilities {
        active_window: "通用占位",
        window_management: "egui viewport 基础窗口控制",
        global_hotkey: "未接入",
        tray: "未接入",
        rich_clipboard: "文本优先",
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn start_hotkey_listener(
    _sender: crossbeam_channel::Sender<crate::clipboard::ClipboardEvent>,
    _ctx: egui::Context,
    _config: HotkeyConfig,
) -> HotkeyUpdateHandle {
    let (sender, _receiver) = crossbeam_channel::unbounded();
    HotkeyUpdateHandle::new(sender)
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn current_keyboard_modifiers() -> KeyboardModifiers {
    KeyboardModifiers::default()
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn validate_hotkey(_combo: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn autostart_enabled() -> Result<bool, String> {
    Ok(false)
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn set_autostart(_enabled: bool) -> Result<(), String> {
    Err("当前平台暂不支持开机启动".to_string())
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn simulate_paste(_prefer_formatted: bool, _method: PasteMethod) -> Result<(), String> {
    Err("当前平台暂未接入模拟粘贴".to_string())
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn discover_apps_for_mime(_mime: &str) -> Vec<AppChoice> {
    Vec::new()
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn start_tray(
    _sender: crossbeam_channel::Sender<crate::clipboard::ClipboardEvent>,
    _ctx: egui::Context,
    _enabled: bool,
) -> Option<TrayHandle> {
    None
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn screen_geometry() -> Option<ScreenGeometry> {
    None
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn mouse_position() -> Option<(f32, f32)> {
    None
}
