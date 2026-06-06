mod app;
mod clipboard;
mod emoji_data;
mod model;
mod platform;
mod sound;
mod storage;
mod ui;

use anyhow::Context;
use app::ClipboardApp;
use std::path::PathBuf;
use std::sync::Arc;
use storage::Storage;

const APP_DISPLAY_NAME: &str = "tiez-slim";
const APP_ID: &str = "tiez-slim-linux";
const DB_PATH_ENV: &str = "TIEZ_SLIM_LINUX_DB_PATH";
const DEV_MODE_ENV: &str = "TIEZ_SLIM_LINUX_DEV";
const LEGACY_DB_PATH_ENV: &str = "MYCLIPBOARD_DB_PATH";
const LEGACY_DEV_MODE_ENV: &str = "MYCLIPBOARD_DEV";

fn main() -> anyhow::Result<()> {
    let dev_mode = dev_mode_enabled();
    let minimized = minimized_start_enabled();
    let storage = Storage::open(resolve_db_path()).context("打开剪贴板数据库失败")?;
    storage.cleanup_expired().context("清理过期历史失败")?;

    let mut viewport = egui::ViewportBuilder::default()
        .with_title(APP_DISPLAY_NAME)
        .with_inner_size([380.0, 680.0])
        .with_min_inner_size([320.0, 400.0])
        .with_position(initial_window_position())
        .with_transparent(true)
        .with_decorations(false)
        .with_resizable(true)
        .with_visible(!minimized);
    if let Some(icon) = load_window_icon() {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        APP_ID,
        options,
        Box::new(move |cc| {
            Ok(Box::new(ClipboardApp::new(
                cc, storage, dev_mode, !minimized,
            )))
        }),
    )
    .map_err(|err| anyhow::anyhow!(err.to_string()))
}

fn minimized_start_enabled() -> bool {
    std::env::args().skip(1).any(|arg| arg == "--minimized")
}

fn initial_window_position() -> egui::Pos2 {
    let screen = platform::screen_geometry().unwrap_or(platform::ScreenGeometry {
        x: 0.0,
        y: 0.0,
        width: 1280.0,
        height: 800.0,
    });
    egui::pos2(
        screen.x + ((screen.width - 380.0) / 2.0).max(8.0),
        screen.y + ((screen.height - 680.0) / 2.0).max(8.0),
    )
}

fn resolve_db_path() -> PathBuf {
    parse_db_path_from_args()
        .or_else(|| std::env::var(DB_PATH_ENV).ok().map(PathBuf::from))
        .or_else(|| std::env::var(LEGACY_DB_PATH_ENV).ok().map(PathBuf::from))
        .or_else(Storage::path_from_redirect_file)
        .unwrap_or_else(Storage::default_path)
}

fn parse_db_path_from_args() -> Option<PathBuf> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--db-path" {
            return args.next().map(PathBuf::from);
        }
    }
    None
}

fn dev_mode_enabled() -> bool {
    let flag_enabled = std::env::args().skip(1).any(|arg| arg == "--dev");
    let env_enabled = std::env::var(DEV_MODE_ENV)
        .or_else(|_| std::env::var(LEGACY_DEV_MODE_ENV))
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false);

    flag_enabled || env_enabled || cfg!(feature = "devtools")
}

fn load_window_icon() -> Option<Arc<egui::IconData>> {
    let image = image::load_from_memory(include_bytes!("../assets/icons/tiez-slim-linux.png"))
        .ok()?
        .into_rgba8();
    let (width, height) = image.dimensions();
    Some(Arc::new(egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    }))
}
