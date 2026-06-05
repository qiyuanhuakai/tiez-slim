use crate::clipboard::ClipboardEvent;
use crate::platform::{
    AppChoice, HotkeyAction, HotkeyConfig, HotkeyUpdateHandle, KeyboardModifiers, PasteMethod,
    PlatformCapabilities, ScreenGeometry, TrayHandle,
};
use crossbeam_channel::Sender;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::thread;
use std::time::Duration;
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{
    AtomEnum, ButtonIndex, ConnectionExt, EventMask, GrabMode, Keycode, ModMask, Window,
};

x11rb::atom_manager! {
    Atoms: AtomsCookie {
        _NET_ACTIVE_WINDOW,
        _NET_WM_NAME,
        WM_NAME,
        UTF8_STRING,
    }
}

/// Cached X11 connection state shared across calls on the same thread.
///
/// `x11rb::connect(None)` opens a fresh socket per call which is wasteful when
/// every mouse-pointer query re-handshakes. We keep one connection alive per
/// thread (UI thread, clipboard watcher, paste worker) and reconnect only
/// when an I/O call fails.
struct SharedX11 {
    conn: Rc<x11rb::rust_connection::RustConnection>,
    screen_num: usize,
}

thread_local! {
    static X11_SHARED: RefCell<Option<SharedX11>> = const { RefCell::new(None) };
}

fn ensure_x11_shared() -> Option<()> {
    X11_SHARED.with(|cell| {
        if cell.borrow().is_some() {
            return Some(());
        }
        let (conn, screen_num) = x11rb::connect(None).ok()?;
        *cell.borrow_mut() = Some(SharedX11 {
            conn: Rc::new(conn),
            screen_num,
        });
        Some(())
    })
}

fn x11_connection() -> Option<Rc<x11rb::rust_connection::RustConnection>> {
    ensure_x11_shared()?;
    X11_SHARED.with(|cell| cell.borrow().as_ref().map(|s| Rc::clone(&s.conn)))
}

fn x11_screen_num() -> Option<usize> {
    ensure_x11_shared()?;
    X11_SHARED.with(|cell| cell.borrow().as_ref().map(|s| s.screen_num))
}

/// Drop the cached connection so the next call re-establishes it.
///
/// Called when an X11 I/O call fails to recover from a dropped server socket
/// (e.g. X server restart, screen locked for too long).
fn reset_x11_connection() {
    X11_SHARED.with(|cell| {
        *cell.borrow_mut() = None;
    });
}

pub fn active_app_name() -> String {
    active_window_title()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| {
            std::env::var("XDG_CURRENT_DESKTOP")
                .or_else(|_| std::env::var("DESKTOP_SESSION"))
                .unwrap_or_else(|_| "Linux".to_string())
        })
}

pub fn platform_note() -> &'static str {
    "Linux 原生模式：支持文本/富文本/图片/文件剪贴板历史、X11 可配置全局热键、系统托盘、边缘停靠、模拟粘贴、前台窗口识别。"
}

#[allow(dead_code)]
pub fn capabilities() -> PlatformCapabilities {
    PlatformCapabilities {
        active_window: "已接入 X11 _NET_ACTIVE_WINDOW，失败时回退桌面环境名",
        window_management: "已接入 egui WindowLevel/OuterPosition 置顶、隐藏和边缘停靠",
        global_hotkey: "已接入 X11 可配置全局热键；Wayland 需后续 evdev/portal 后端",
        tray: "已接入 ksni StatusNotifierItem 托盘；取决于桌面 SNI/AppIndicator 支持",
        rich_clipboard: "arboard 已接入文本、text/html、image/png、file_list targets",
    }
}

pub fn start_hotkey_listener(
    sender: Sender<ClipboardEvent>,
    ctx: egui::Context,
    config: HotkeyConfig,
) -> HotkeyUpdateHandle {
    let (update_sender, update_receiver) = crossbeam_channel::unbounded();
    thread::Builder::new()
        .name("x11-hotkey-listener".to_string())
        .spawn(move || {
            if let Err(err) = hotkey_loop(sender.clone(), ctx.clone(), config, update_receiver) {
                let _ = sender.send(ClipboardEvent::Status(format!("全局快捷键不可用: {err}")));
                ctx.request_repaint();
            }
        })
        .expect("spawn x11 hotkey listener");
    HotkeyUpdateHandle::new(update_sender)
}

pub fn current_keyboard_modifiers() -> KeyboardModifiers {
    read_current_keyboard_modifiers().unwrap_or_default()
}

pub fn validate_hotkey(combo: &str) -> Result<(), String> {
    let (conn, _) = x11rb::connect(None).map_err(|err| err.to_string())?;
    parse_hotkey(&conn, combo, HotkeyAction::ToggleWindow).map(|_| ())
}

pub fn autostart_enabled() -> Result<bool, String> {
    Ok(autostart_desktop_path()?.exists())
}

pub fn set_autostart(enabled: bool) -> Result<(), String> {
    let path = autostart_desktop_path()?;
    if enabled {
        let parent = path
            .parent()
            .ok_or_else(|| "无法定位 autostart 目录".to_string())?;
        fs::create_dir_all(parent).map_err(|err| format!("创建开机启动目录失败: {err}"))?;
        let exe = std::env::current_exe().map_err(|err| format!("读取程序路径失败: {err}"))?;
        let exec = desktop_exec_arg(&exe)?;
        let desktop = format!(
            "[Desktop Entry]\nType=Application\nName=tiez-slim\nComment=Native clipboard manager\nExec={exec}\nHidden=false\nNoDisplay=false\nX-GNOME-Autostart-enabled=true\nStartupNotify=false\nStartupWMClass=tiez-slim-linux\n",
        );
        fs::write(&path, desktop).map_err(|err| format!("写入开机启动配置失败: {err}"))?;
    } else if path.exists() {
        fs::remove_file(&path).map_err(|err| format!("删除开机启动配置失败: {err}"))?;
    }
    Ok(())
}

fn autostart_desktop_path() -> Result<PathBuf, String> {
    let config_dir = dirs::config_dir().ok_or_else(|| "无法定位 XDG 配置目录".to_string())?;
    Ok(config_dir.join("autostart").join("tiez-slim-linux.desktop"))
}

fn desktop_exec_arg(path: &Path) -> Result<String, String> {
    let value = path
        .to_str()
        .ok_or_else(|| "程序路径不是有效 UTF-8，无法写入开机启动配置".to_string())?;
    if value.chars().any(char::is_control) {
        return Err("程序路径包含控制字符，无法写入开机启动配置".to_string());
    }
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for ch in value.chars() {
        match ch {
            '"' | '\\' | '`' | '$' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            '%' => escaped.push_str("%%"),
            _ => escaped.push(ch),
        }
    }
    escaped.push('"');
    Ok(escaped)
}

fn read_current_keyboard_modifiers() -> Option<KeyboardModifiers> {
    let (conn, _) = x11rb::connect(None).ok()?;
    let keymap = conn.query_keymap().ok()?.reply().ok()?.keys;
    let mapping = conn.get_modifier_mapping().ok()?.reply().ok()?;
    let per_modifier = mapping.keycodes_per_modifier() as usize;

    Some(KeyboardModifiers {
        shift: modifier_group_pressed(&keymap, &mapping.keycodes, per_modifier, 0),
        ctrl: modifier_group_pressed(&keymap, &mapping.keycodes, per_modifier, 2),
        alt: modifier_group_pressed(&keymap, &mapping.keycodes, per_modifier, 3),
        super_key: modifier_group_pressed(&keymap, &mapping.keycodes, per_modifier, 6),
    })
}

fn modifier_group_pressed(
    keymap: &[u8; 32],
    keycodes: &[Keycode],
    per_modifier: usize,
    group: usize,
) -> bool {
    let start = group.saturating_mul(per_modifier);
    keycodes
        .iter()
        .skip(start)
        .take(per_modifier)
        .copied()
        .filter(|keycode| *keycode != 0)
        .any(|keycode| keycode_pressed(keymap, keycode))
}

fn keycode_pressed(keymap: &[u8; 32], keycode: Keycode) -> bool {
    let index = (keycode / 8) as usize;
    let mask = 1u8 << (keycode % 8);
    keymap.get(index).is_some_and(|value| value & mask != 0)
}

pub fn start_tray(
    sender: Sender<ClipboardEvent>,
    ctx: egui::Context,
    enabled: bool,
) -> Option<TrayHandle> {
    if !enabled {
        return None;
    }
    match start_status_notifier(sender.clone(), ctx.clone()) {
        Ok(handle) => Some(TrayHandle::new(move || handle.shutdown().wait())),
        Err(err) => {
            let _ = sender.send(ClipboardEvent::Status(format!("系统托盘不可用: {err}")));
            ctx.request_repaint();
            None
        }
    }
}

pub fn screen_size() -> Option<(f32, f32)> {
    let conn = x11_connection()?;
    let screen_num = x11_screen_num()?;
    let screen = &conn.setup().roots[screen_num];
    Some((
        screen.width_in_pixels as f32,
        screen.height_in_pixels as f32,
    ))
}

pub fn mouse_position() -> Option<(f32, f32)> {
    let conn = x11_connection()?;
    let screen_num = x11_screen_num()?;
    let screen = &conn.setup().roots[screen_num];
    let reply = match conn.query_pointer(screen.root) {
        Ok(cookie) => match cookie.reply() {
            Ok(reply) => reply,
            Err(_) => {
                reset_x11_connection();
                return None;
            }
        },
        Err(_) => {
            reset_x11_connection();
            return None;
        }
    };
    Some((reply.root_x as f32, reply.root_y as f32))
}

pub fn screen_geometry() -> Option<ScreenGeometry> {
    let mouse = mouse_position();
    if let Some(geometry) = xrandr_screen_geometries()
        .into_iter()
        .find(|geometry| match mouse {
            Some((x, y)) => {
                x >= geometry.x
                    && x < geometry.x + geometry.width
                    && y >= geometry.y
                    && y < geometry.y + geometry.height
            }
            None => false,
        })
    {
        return Some(geometry);
    }
    let (width, height) = screen_size()?;
    Some(ScreenGeometry {
        x: 0.0,
        y: 0.0,
        width,
        height,
    })
}

fn xrandr_screen_geometries() -> Vec<ScreenGeometry> {
    let Ok(output) = Command::new("xrandr").arg("--query").output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.contains(" connected"))
        .filter_map(parse_xrandr_geometry)
        .collect()
}

fn parse_xrandr_geometry(line: &str) -> Option<ScreenGeometry> {
    line.split_whitespace().find_map(parse_geometry_token)
}

fn parse_geometry_token(token: &str) -> Option<ScreenGeometry> {
    let x_index = token.find('x')?;
    let offset_x = find_signed_offset(token, x_index + 1)?;
    let offset_y = find_signed_offset(token, offset_x + 1)?;
    let width = token[..x_index].parse::<f32>().ok()?;
    let height = token[x_index + 1..offset_x].parse::<f32>().ok()?;
    let x = token[offset_x..offset_y].parse::<f32>().ok()?;
    let y = token[offset_y..].parse::<f32>().ok()?;
    Some(ScreenGeometry {
        x,
        y,
        width,
        height,
    })
}

fn find_signed_offset(token: &str, start: usize) -> Option<usize> {
    token[start..]
        .char_indices()
        .find_map(|(offset, ch)| (ch == '+' || ch == '-').then_some(start + offset))
}

#[cfg(test)]
mod tests {
    use super::parse_geometry_token;

    #[test]
    fn parses_xrandr_geometry_with_positive_offsets() {
        let geometry = parse_geometry_token("2560x1440+1920+0").expect("geometry");
        assert_eq!(geometry.x, 1920.0);
        assert_eq!(geometry.y, 0.0);
        assert_eq!(geometry.width, 2560.0);
        assert_eq!(geometry.height, 1440.0);
    }

    #[test]
    fn parses_xrandr_geometry_with_signed_offsets() {
        let geometry = parse_geometry_token("1920x1080-1920-120").expect("geometry");
        assert_eq!(geometry.x, -1920.0);
        assert_eq!(geometry.y, -120.0);
        assert_eq!(geometry.width, 1920.0);
        assert_eq!(geometry.height, 1080.0);
    }
}

pub fn discover_apps_for_mime(mime: &str) -> Vec<AppChoice> {
    let default_id = Command::new("xdg-mime")
        .args(["query", "default", mime])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| !value.is_empty());
    let desktop_entries = scan_desktop_entries();
    let mut choices = Vec::new();
    if let Some(default_id) = default_id.as_deref()
        && let Some(choice) = desktop_entries.get(default_id)
    {
        choices.push(AppChoice {
            name: format!("系统默认：{}", choice.name),
            command: choice.command.clone(),
            is_default: true,
        });
    }
    let mut seen = choices
        .iter()
        .map(|choice| choice.command.clone())
        .collect::<HashSet<_>>();
    for choice in desktop_entries.values() {
        if seen.insert(choice.command.clone()) {
            choices.push(choice.clone());
        }
    }
    choices.sort_by(|a, b| {
        b.is_default
            .cmp(&a.is_default)
            .then_with(|| a.name.cmp(&b.name))
    });
    choices.truncate(96);
    choices
}

fn scan_desktop_entries() -> BTreeMap<String, AppChoice> {
    let mut entries = BTreeMap::new();
    for dir in desktop_dirs() {
        let Ok(read_dir) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("desktop") {
                continue;
            }
            if let Some((id, choice)) = parse_desktop_entry(&path) {
                entries.entry(id).or_insert(choice);
            }
        }
    }
    entries
}

fn desktop_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![PathBuf::from("/usr/share/applications")];
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".local/share/applications"));
    }
    dirs
}

fn parse_desktop_entry(path: &Path) -> Option<(String, AppChoice)> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut in_desktop_entry = false;
    let mut name = None;
    let mut exec = None;
    let mut no_display = false;
    let mut terminal = false;
    for line in content.lines().map(str::trim) {
        if line.starts_with('[') && line.ends_with(']') {
            in_desktop_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_desktop_entry || line.starts_with('#') {
            continue;
        }
        if let Some(value) = line.strip_prefix("Name[zh_CN]=") {
            name = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("Name=") {
            name.get_or_insert_with(|| value.to_string());
        } else if let Some(value) = line.strip_prefix("Exec=") {
            exec = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("NoDisplay=") {
            no_display = value.eq_ignore_ascii_case("true");
        } else if let Some(value) = line.strip_prefix("Terminal=") {
            terminal = value.eq_ignore_ascii_case("true");
        }
    }
    if no_display || terminal {
        return None;
    }
    let command = executable_from_exec(&exec?)?;
    let id = path.file_name()?.to_string_lossy().to_string();
    Some((
        id,
        AppChoice {
            name: name.unwrap_or_else(|| command.clone()),
            command,
            is_default: false,
        },
    ))
}

fn executable_from_exec(exec: &str) -> Option<String> {
    exec.split_whitespace()
        .find(|token| !token.starts_with('%') && !token.contains('='))
        .map(|token| token.trim_matches('"').to_string())
        .filter(|token| !token.is_empty())
}

/// Non-blocking: dispatches the xdotool work to a worker thread so the UI
/// thread is not stalled for the 5-200ms the paste takes. The result of the
/// paste is intentionally discarded — the caller cannot observe it from the
/// UI thread, and any xdotool error is dropped.
pub fn simulate_paste(prefer_formatted: bool, method: PasteMethod) -> Result<(), String> {
    thread::Builder::new()
        .name("simulate-paste".to_string())
        .spawn(move || {
            let _ = run_simulate_paste(prefer_formatted, method);
        })
        .map_err(|err| format!("启动模拟粘贴线程失败: {err}"))?;
    Ok(())
}

fn run_simulate_paste(prefer_formatted: bool, method: PasteMethod) -> Result<(), String> {
    release_active_modifiers();
    let key = match method {
        PasteMethod::Auto => {
            if prefer_formatted {
                "ctrl+v"
            } else {
                "Shift+Insert"
            }
        }
        PasteMethod::ShiftInsert => {
            if prefer_formatted {
                "ctrl+v"
            } else {
                "Shift+Insert"
            }
        }
        PasteMethod::CtrlV => "ctrl+v",
        PasteMethod::TypeText if prefer_formatted => "ctrl+v",
        PasteMethod::TypeText => return simulate_type_paste(),
    };
    let status = Command::new("xdotool")
        .args(["key", "--clearmodifiers", key])
        .status()
        .map_err(|err| format!("模拟粘贴失败：未能执行 xdotool: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("模拟粘贴失败：xdotool 返回 {status}"))
    }
}

fn simulate_type_paste() -> Result<(), String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|err| format!("读取剪贴板失败：{err}"))?;
    let text = clipboard
        .get_text()
        .map_err(|err| format!("读取剪贴板文本失败：{err}"))?;
    let mut child = Command::new("xdotool")
        .args(["type", "--clearmodifiers", "--file", "-"])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|err| format!("模拟输入失败：未能执行 xdotool: {err}"))?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|err| format!("写入 xdotool 输入失败: {err}"))?;
    }
    let status = child
        .wait()
        .map_err(|err| format!("等待 xdotool type 失败: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("模拟输入失败：xdotool 返回 {status}"))
    }
}

fn release_active_modifiers() {
    let _ = Command::new("xdotool")
        .args([
            "keyup",
            "Control_L",
            "Control_R",
            "Shift_L",
            "Shift_R",
            "Alt_L",
            "Alt_R",
            "Super_L",
            "Super_R",
            "Meta_L",
            "Meta_R",
        ])
        .status();
}

#[derive(Clone, Debug)]
struct GrabbedHotkey {
    keycode: Option<Keycode>,
    button: Option<ButtonIndex>,
    modifiers: ModMask,
    action: HotkeyAction,
    label: String,
}

fn hotkey_loop(
    sender: Sender<ClipboardEvent>,
    ctx: egui::Context,
    config: HotkeyConfig,
    updates: crossbeam_channel::Receiver<HotkeyConfig>,
) -> Result<(), String> {
    let (conn, screen_num) = x11rb::connect(None).map_err(|err| err.to_string())?;
    let screen = &conn.setup().roots[screen_num];
    let root = screen.root;
    conn.change_window_attributes(
        root,
        &x11rb::protocol::xproto::ChangeWindowAttributesAux::new().event_mask(EventMask::KEY_PRESS),
    )
    .map_err(|err| err.to_string())?
    .check()
    .map_err(|err| err.to_string())?;

    let mut grabbed = configure_hotkeys(&conn, root, &[], &config)?;
    let _ = sender.send(ClipboardEvent::Status(format_hotkey_status(&grabbed)));
    ctx.request_repaint();

    loop {
        while let Ok(new_config) = updates.try_recv() {
            match configure_hotkeys(&conn, root, &grabbed, &new_config) {
                Ok(next) => {
                    grabbed = next;
                    let _ = sender.send(ClipboardEvent::Status(format_hotkey_status(&grabbed)));
                    ctx.request_repaint();
                }
                Err(err) => {
                    let _ =
                        sender.send(ClipboardEvent::Status(format!("更新全局快捷键失败: {err}")));
                    ctx.request_repaint();
                }
            }
        }

        while let Some(event) = conn.poll_for_event().map_err(|err| err.to_string())? {
            match event {
                Event::KeyPress(event) => {
                    let state = ModMask::from(u16::from(event.state));
                    if let Some(hotkey) = grabbed.iter().find(|hotkey| {
                        hotkey.keycode == Some(event.detail) && state.contains(hotkey.modifiers)
                    }) {
                        send_hotkey_action(&sender, hotkey.action);
                        ctx.request_repaint();
                    }
                }
                Event::ButtonPress(event) => {
                    let state = ModMask::from(u16::from(event.state));
                    if let Some(hotkey) = grabbed.iter().find(|hotkey| {
                        hotkey.button == Some(event.detail.into())
                            && state.contains(hotkey.modifiers)
                    }) {
                        send_hotkey_action(&sender, hotkey.action);
                        ctx.request_repaint();
                    }
                }
                _ => {}
            }
        }
        thread::sleep(Duration::from_millis(35));
    }
}

fn configure_hotkeys<C: Connection>(
    conn: &C,
    root: Window,
    old: &[GrabbedHotkey],
    config: &HotkeyConfig,
) -> Result<Vec<GrabbedHotkey>, String> {
    for hotkey in old {
        ungrab_hotkey(conn, root, hotkey)?;
    }

    let mut next = Vec::new();
    for combo in parse_main_hotkeys(&config.main_hotkeys) {
        if let Some(hotkey) = parse_hotkey(conn, &combo, HotkeyAction::ToggleWindow)? {
            grab_hotkey(conn, root, &hotkey)?;
            next.push(hotkey);
        }
    }
    for (combo, action) in [
        (&config.sequential_hotkey, HotkeyAction::SequentialPaste),
        (&config.rich_paste_hotkey, HotkeyAction::RichPaste),
        (&config.search_hotkey, HotkeyAction::FocusSearch),
    ] {
        if let Some(hotkey) = parse_hotkey(conn, combo, action)? {
            grab_hotkey(conn, root, &hotkey)?;
            next.push(hotkey);
        }
    }
    conn.flush().map_err(|err| err.to_string())?;
    Ok(next)
}

fn grab_hotkey<C: Connection>(
    conn: &C,
    root: Window,
    hotkey: &GrabbedHotkey,
) -> Result<(), String> {
    for modifier in ignored_mods() {
        if let Some(keycode) = hotkey.keycode {
            conn.grab_key(
                false,
                root,
                hotkey.modifiers | modifier,
                keycode,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            )
            .map_err(|err| err.to_string())?
            .check()
            .map_err(|err| format!("{} 注册失败: {err}", hotkey.label))?;
        }
        if let Some(button) = hotkey.button {
            conn.grab_button(
                false,
                root,
                EventMask::BUTTON_PRESS,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
                x11rb::NONE,
                x11rb::NONE,
                button,
                hotkey.modifiers | modifier,
            )
            .map_err(|err| err.to_string())?
            .check()
            .map_err(|err| format!("{} 注册失败: {err}", hotkey.label))?;
        }
    }
    Ok(())
}

fn ungrab_hotkey<C: Connection>(
    conn: &C,
    root: Window,
    hotkey: &GrabbedHotkey,
) -> Result<(), String> {
    for modifier in ignored_mods() {
        if let Some(keycode) = hotkey.keycode {
            conn.ungrab_key(keycode, root, hotkey.modifiers | modifier)
                .map_err(|err| err.to_string())?
                .check()
                .map_err(|err| err.to_string())?;
        }
        if let Some(button) = hotkey.button {
            conn.ungrab_button(button, root, hotkey.modifiers | modifier)
                .map_err(|err| err.to_string())?
                .check()
                .map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

fn ignored_mods() -> [ModMask; 4] {
    [
        ModMask::default(),
        ModMask::M2,
        ModMask::LOCK,
        ModMask::M2 | ModMask::LOCK,
    ]
}

fn parse_main_hotkeys(value: &str) -> Vec<String> {
    value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn parse_hotkey<C: Connection>(
    conn: &C,
    combo: &str,
    action: HotkeyAction,
) -> Result<Option<GrabbedHotkey>, String> {
    let combo = combo.trim();
    if combo.is_empty() {
        return Ok(None);
    }
    let mut modifiers = ModMask::default();
    let mut key = None;
    for part in combo
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers |= ModMask::CONTROL,
            "alt" | "option" => modifiers |= ModMask::M1,
            "shift" => modifiers |= ModMask::SHIFT,
            "super" | "win" | "meta" | "cmd" => modifiers |= ModMask::M4,
            "mousemiddle" | "mbutton" | "middlemouse" | "button2" => {
                return Ok(Some(GrabbedHotkey {
                    keycode: None,
                    button: Some(ButtonIndex::M2),
                    modifiers,
                    action,
                    label: combo.to_string(),
                }));
            }
            _ => key = Some(part.to_string()),
        }
    }
    let key = key.ok_or_else(|| format!("快捷键缺少按键: {combo}"))?;
    let keysym = key_to_keysym(&key).ok_or_else(|| format!("暂不支持的快捷键按键: {key}"))?;
    let keycode =
        keysym_to_keycode(conn, keysym)?.ok_or_else(|| format!("未找到按键 {key} 的 keycode"))?;
    Ok(Some(GrabbedHotkey {
        keycode: Some(keycode),
        button: None,
        modifiers,
        action,
        label: combo.to_string(),
    }))
}

fn key_to_keysym(key: &str) -> Option<u32> {
    let normalized = key.trim();
    if normalized.chars().count() == 1 {
        let ch = normalized.chars().next()?.to_ascii_lowercase();
        if ch.is_ascii_graphic() && ch != '+' {
            return Some(ch as u32);
        }
    }
    match normalized.to_ascii_lowercase().as_str() {
        "space" => Some(0x0020),
        "tab" => Some(0xff09),
        "backspace" => Some(0xff08),
        "enter" | "return" => Some(0xff0d),
        "escape" | "esc" => Some(0xff1b),
        "insert" => Some(0xff63),
        "delete" => Some(0xffff),
        "up" | "arrowup" => Some(0xff52),
        "down" | "arrowdown" => Some(0xff54),
        "left" | "arrowleft" => Some(0xff51),
        "right" | "arrowright" => Some(0xff53),
        "home" => Some(0xff50),
        "end" => Some(0xff57),
        "pageup" => Some(0xff55),
        "pagedown" => Some(0xff56),
        "plus" => Some(0x002b),
        _ => parse_function_key(normalized),
    }
}

fn parse_function_key(key: &str) -> Option<u32> {
    let number = key
        .strip_prefix('F')
        .or_else(|| key.strip_prefix('f'))?
        .parse::<u32>()
        .ok()?;
    if (1..=35).contains(&number) {
        Some(0xffbe + number - 1)
    } else {
        None
    }
}

fn send_hotkey_action(sender: &Sender<ClipboardEvent>, action: HotkeyAction) {
    let event = match action {
        HotkeyAction::ToggleWindow => ClipboardEvent::ToggleWindow,
        HotkeyAction::SequentialPaste => ClipboardEvent::SequentialPaste,
        HotkeyAction::RichPaste => ClipboardEvent::PasteLatestRich,
        HotkeyAction::FocusSearch => ClipboardEvent::FocusSearch,
    };
    let _ = sender.send(event);
}

fn format_hotkey_status(hotkeys: &[GrabbedHotkey]) -> String {
    if hotkeys.is_empty() {
        "没有可用的全局快捷键".to_string()
    } else {
        format!(
            "全局快捷键已更新：{}",
            hotkeys
                .iter()
                .map(|key| key.label.as_str())
                .collect::<Vec<_>>()
                .join(" / ")
        )
    }
}

fn keysym_to_keycode<C: Connection>(conn: &C, keysym: u32) -> Result<Option<Keycode>, String> {
    let setup = conn.setup();
    let min_keycode = setup.min_keycode;
    let count = setup.max_keycode - min_keycode + 1;
    let mapping = conn
        .get_keyboard_mapping(min_keycode, count)
        .map_err(|err| err.to_string())?
        .reply()
        .map_err(|err| err.to_string())?;
    let keysyms_per_keycode = mapping.keysyms_per_keycode as usize;
    for (index, keysyms) in mapping.keysyms.chunks(keysyms_per_keycode).enumerate() {
        if keysyms.contains(&keysym) {
            return Ok(Some(min_keycode + index as u8));
        }
    }
    Ok(None)
}

fn start_status_notifier(
    sender: Sender<ClipboardEvent>,
    ctx: egui::Context,
) -> Result<ksni::blocking::Handle<TiezSlimLinuxTray>, String> {
    use ksni::blocking::TrayMethods;

    let tray = TiezSlimLinuxTray { sender, ctx };
    tray.assume_sni_available(true)
        .spawn()
        .map_err(|err| err.to_string())
}

struct TiezSlimLinuxTray {
    sender: Sender<ClipboardEvent>,
    ctx: egui::Context,
}

impl ksni::Tray for TiezSlimLinuxTray {
    fn id(&self) -> String {
        "tiez-slim-linux".to_string()
    }

    fn title(&self) -> String {
        "tiez-slim".to_string()
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::ApplicationStatus
    }

    fn icon_name(&self) -> String {
        "tiez-slim-linux".to_string()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![tray_icon_pixmap()]
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.sender.send(ClipboardEvent::ToggleWindow);
        self.ctx.request_repaint();
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        vec![
            StandardItem {
                label: "显示/隐藏".to_string(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.sender.send(ClipboardEvent::ToggleWindow);
                    tray.ctx.request_repaint();
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "设置".to_string(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.sender.send(ClipboardEvent::OpenSettings);
                    tray.ctx.request_repaint();
                }),
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
            StandardItem {
                label: "退出".to_string(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.sender.send(ClipboardEvent::Quit);
                    tray.ctx.request_repaint();
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

fn tray_icon_pixmap() -> ksni::Icon {
    let width = 32;
    let height = 32;
    let mut data = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            let in_round = (2..=29).contains(&x) && (2..=29).contains(&y);
            let navy = (31, 58, 95);
            let accent = (72, 123, 219);
            let on_mark = ((9..=22).contains(&x) && (y == 8 || y == 23))
                || ((8..=23).contains(&y) && (x == 9 || x == 22))
                || ((13..=18).contains(&x) && (y == 5 || y == 6))
                || ((11..=20).contains(&y) && (x == 15 || x == 16))
                || (y == 20 && (12..=19).contains(&x))
                || ((18..=23).contains(&x) && (11..=16).contains(&y) && x + y >= 33);
            let on_slim = y == 26 && (10..=21).contains(&x);
            let (a, r, g, b) = if on_mark {
                (255, navy.0, navy.1, navy.2)
            } else if on_slim {
                (255, accent.0, accent.1, accent.2)
            } else if in_round {
                (255, 255, 255, 255)
            } else {
                (0, 0, 0, 0)
            };
            data.extend_from_slice(&[a, r, g, b]);
        }
    }
    ksni::Icon {
        width: width as i32,
        height: height as i32,
        data,
    }
}

fn active_window_title() -> Option<String> {
    let conn = x11_connection()?;
    let screen_num = x11_screen_num()?;
    let screen = &conn.setup().roots[screen_num];
    let atoms = match Atoms::new(conn.as_ref()).ok()?.reply() {
        Ok(atoms) => atoms,
        Err(_) => {
            reset_x11_connection();
            return None;
        }
    };
    let active_window = read_window_property(conn.as_ref(), screen.root, atoms._NET_ACTIVE_WINDOW)?;

    let result = read_string_property(
        conn.as_ref(),
        active_window,
        atoms._NET_WM_NAME,
        atoms.UTF8_STRING,
    )
    .or_else(|| {
        read_string_property(
            conn.as_ref(),
            active_window,
            atoms.WM_NAME,
            AtomEnum::STRING.into(),
        )
    });
    if result.is_none() {
        reset_x11_connection();
    }
    result
}

fn read_window_property<C: Connection>(conn: &C, window: Window, atom: u32) -> Option<Window> {
    conn.get_property(false, window, atom, AtomEnum::WINDOW, 0, 1)
        .ok()?
        .reply()
        .ok()?
        .value32()?
        .next()
}

fn read_string_property<C: Connection>(
    conn: &C,
    window: Window,
    atom: u32,
    kind: u32,
) -> Option<String> {
    let reply = conn
        .get_property(false, window, atom, kind, 0, 2048)
        .ok()?
        .reply()
        .ok()?;
    String::from_utf8(reply.value).ok()
}
