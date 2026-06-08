//! XFixes-based PRIMARY selection monitoring for Linux/X11.
//!
//! When XFixes 4.0+ is available, subscribes to `SelectionNotify` events
//! for instant PRIMARY change detection with 200 ms debounce.
//! Falls back to xclip polling when XFixes is unavailable or the
//! connection drops.

use crate::clipboard::ClipboardEvent;
use crate::model::{ClipboardEntry, SelectionSource};
use crate::platform;
use crossbeam_channel::Sender;
use rust_i18n::t;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol::xfixes::SelectionEventMask;

const DEBOUNCE_WINDOW: Duration = Duration::from_millis(200);
const POLL_INTERVAL: Duration = Duration::from_millis(700);

pub struct XFixesProbe;

impl XFixesProbe {
    pub fn is_available() -> bool {
        let Ok((conn, _screen_num)) = x11rb::connect(None) else {
            return false;
        };
        x11rb::protocol::xfixes::query_version(&conn, 4, 0)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .is_some()
    }
}

pub fn start_primary_watcher(sender: Sender<ClipboardEvent>, primary_enabled: Arc<AtomicBool>) {
    thread::Builder::new()
        .name("primary-watcher".to_string())
        .spawn(move || {
            if XFixesProbe::is_available() {
                eprintln!("primary_watcher: XFixes event mode");
                run_xfixes_loop(&sender, &primary_enabled);
            } else {
                eprintln!("primary_watcher: degraded to arboard polling");
                let _ = sender.try_send(ClipboardEvent::Status(
                    t!("status.primary_degraded").to_string(),
                ));
                run_polling_loop(&sender, &primary_enabled);
            }
        })
        .expect("spawn primary watcher");
}

fn run_xfixes_loop(sender: &Sender<ClipboardEvent>, primary_enabled: &Arc<AtomicBool>) {
    loop {
        if !primary_enabled.load(Ordering::Acquire) {
            thread::sleep(Duration::from_millis(500));
            continue;
        }
        match try_xfixes_session(sender, primary_enabled) {
            Ok(()) => return,
            Err(err) => {
                eprintln!("primary_watcher: XFixes session error: {err}, retrying in 2s");
                thread::sleep(Duration::from_secs(2));
            }
        }
    }
}

fn try_xfixes_session(
    sender: &Sender<ClipboardEvent>,
    primary_enabled: &Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (conn, screen_num) = x11rb::connect(None)?;
    let screen = &conn.setup().roots[screen_num];
    let root = screen.root;

    x11rb::protocol::xfixes::query_version(&conn, 4, 0)?.reply()?;

    let primary_atom = x11rb::protocol::xproto::intern_atom(&conn, false, b"PRIMARY")?
        .reply()?
        .atom;

    x11rb::protocol::xfixes::select_selection_input(
        &conn,
        root,
        primary_atom,
        SelectionEventMask::SET_SELECTION_OWNER,
    )?
    .check()?;
    conn.flush()?;

    let mut last_event_time = Instant::now() - DEBOUNCE_WINDOW;
    let mut last_text = String::new();

    loop {
        if !primary_enabled.load(Ordering::Acquire) {
            thread::sleep(Duration::from_millis(200));
            continue;
        }

        match conn.poll_for_event()? {
            Some(_event) => {
                let now = Instant::now();
                if now.duration_since(last_event_time) < DEBOUNCE_WINDOW {
                    continue;
                }
                last_event_time = now;

                if let Some(text) = read_primary_text()
                    && text != last_text
                {
                    last_text = text.clone();
                    if let Some(entry) = ClipboardEntry::captured_text_with_source(
                        text,
                        platform::active_app_name(),
                        Some(SelectionSource::Primary),
                    ) {
                        let _ = sender.try_send(ClipboardEvent::Captured(entry));
                    }
                }
            }
            None => {
                thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

fn run_polling_loop(sender: &Sender<ClipboardEvent>, primary_enabled: &Arc<AtomicBool>) {
    let mut last_text = String::new();
    loop {
        if !primary_enabled.load(Ordering::Acquire) {
            thread::sleep(POLL_INTERVAL);
            continue;
        }
        if let Some(text) = read_primary_text()
            && text != last_text
        {
            last_text = text.clone();
            if let Some(entry) = ClipboardEntry::captured_text_with_source(
                text,
                platform::active_app_name(),
                Some(SelectionSource::Primary),
            ) {
                let _ = sender.try_send(ClipboardEvent::Captured(entry));
            }
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn read_primary_text() -> Option<String> {
    let output = Command::new("xclip")
        .args(["-selection", "primary", "-o"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::XFixesProbe;

    #[test]
    fn is_available_returns_bool_without_panic() {
        let _available = XFixesProbe::is_available();
    }
}
