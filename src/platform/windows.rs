use super::PlatformCapabilities;
use crate::clipboard::ClipboardEvent;
use crate::platform::{
    AppChoice, HotkeyConfig, HotkeyUpdateHandle, KeyboardModifiers, PasteMethod, ScreenGeometry,
    TrayHandle,
};
use crossbeam_channel::Sender;
use rust_i18n::t;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VIRTUAL_KEY, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};

pub fn active_app_name() -> String {
    "Windows".to_string()
}

pub fn platform_note() -> String {
    t!("platform.note.windows")
}

#[allow(dead_code)]
pub fn capabilities() -> PlatformCapabilities {
    PlatformCapabilities {
        active_window: t!("platform.capability.active_window_windows"),
        window_management: t!("platform.capability.window_mgmt_generic"),
        global_hotkey: t!("platform.capability.hotkey_windows"),
        tray: t!("platform.capability.tray_windows"),
        rich_clipboard: t!("platform.capability.rich_clipboard_windows"),
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
    Err(t!("platform.windows_hotkey_not_implemented"))
}

pub fn autostart_enabled() -> Result<bool, String> {
    Err(t!("platform.windows_autostart_not_implemented"))
}

pub fn set_autostart(_enabled: bool) -> Result<(), String> {
    Err(t!("platform.windows_autostart_not_implemented"))
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
    Err(t!("platform.windows_paste_not_implemented"))
}

pub fn discover_apps_for_mime(_mime: &str) -> Vec<AppChoice> {
    Vec::new()
}
