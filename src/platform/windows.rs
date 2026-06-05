use super::PlatformCapabilities;
use crate::clipboard::ClipboardEvent;
use crate::platform::{
    AppChoice, HotkeyConfig, HotkeyUpdateHandle, KeyboardModifiers, PasteMethod, ScreenGeometry,
    TrayHandle,
};
use crossbeam_channel::Sender;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VIRTUAL_KEY, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};

pub fn active_app_name() -> String {
    "Windows".to_string()
}

pub fn platform_note() -> &'static str {
    "Windows 预留模式：平台抽象已存在，后续可接入 Win32 剪贴板、窗口追踪和全局快捷键。"
}

#[allow(dead_code)]
pub fn capabilities() -> PlatformCapabilities {
    PlatformCapabilities {
        active_window: "预留：GetForegroundWindow + GetWindowTextW",
        window_management: "egui viewport 基础窗口控制",
        global_hotkey: "预留：RegisterHotKey 或 global-hotkey",
        tray: "预留：tray-icon Windows backend",
        rich_clipboard: "预留：Win32 Clipboard formats",
    }
}

pub fn start_hotkey_listener(
    _sender: Sender<ClipboardEvent>,
    _ctx: egui::Context,
    _config: HotkeyConfig,
) -> HotkeyUpdateHandle {
    let (sender, _receiver) = crossbeam_channel::unbounded();
    HotkeyUpdateHandle::new(sender)
}

pub fn current_keyboard_modifiers() -> KeyboardModifiers {
    KeyboardModifiers {
        ctrl: key_is_pressed(VK_CONTROL),
        shift: key_is_pressed(VK_SHIFT),
        alt: key_is_pressed(VK_MENU),
        super_key: key_is_pressed(VK_LWIN) || key_is_pressed(VK_RWIN),
    }
}

fn key_is_pressed(key: VIRTUAL_KEY) -> bool {
    unsafe { GetAsyncKeyState(key.0 as i32) < 0 }
}

pub fn validate_hotkey(_combo: &str) -> Result<(), String> {
    Err("Windows 全局快捷键后端尚未实现，暂不支持保存快捷键".to_string())
}

pub fn autostart_enabled() -> Result<bool, String> {
    Err("Windows 开机启动仍使用预留 Win32 后端".to_string())
}

pub fn set_autostart(_enabled: bool) -> Result<(), String> {
    Err("Windows 开机启动仍使用预留 Win32 后端".to_string())
}

pub fn start_tray(
    _sender: Sender<ClipboardEvent>,
    _ctx: egui::Context,
    _enabled: bool,
) -> Option<TrayHandle> {
    None
}

pub fn screen_size() -> Option<(f32, f32)> {
    None
}

pub fn mouse_position() -> Option<(f32, f32)> {
    None
}

pub fn screen_geometry() -> Option<ScreenGeometry> {
    None
}

pub fn simulate_paste(_prefer_formatted: bool, _method: PasteMethod) -> Result<(), String> {
    Err("Windows 粘贴模拟仍使用预留 Win32 后端".to_string())
}

pub fn discover_apps_for_mime(_mime: &str) -> Vec<AppChoice> {
    Vec::new()
}
