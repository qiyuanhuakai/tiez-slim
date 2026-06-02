mod app;
mod clipboard;
mod model;
mod platform;
mod storage;

use anyhow::Context;
use app::ClipboardApp;
use std::path::PathBuf;
use storage::Storage;

fn main() -> anyhow::Result<()> {
    let dev_mode = dev_mode_enabled();
    let storage = Storage::open(resolve_db_path()).context("打开剪贴板数据库失败")?;
    storage.cleanup_expired().context("清理过期历史失败")?;

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("MyClipboard")
            .with_inner_size([380.0, 680.0])
            .with_min_inner_size([320.0, 400.0])
            .with_position(initial_window_position())
            .with_transparent(true)
            .with_decorations(false),
        ..Default::default()
    };

    eframe::run_native(
        "MyClipboard",
        options,
        Box::new(move |cc| Ok(Box::new(ClipboardApp::new(cc, storage, dev_mode)))),
    )
    .map_err(|err| anyhow::anyhow!(err.to_string()))
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
        .or_else(|| std::env::var("MYCLIPBOARD_DB_PATH").ok().map(PathBuf::from))
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
    let env_enabled = std::env::var("MYCLIPBOARD_DEV")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false);

    flag_enabled || env_enabled || cfg!(feature = "devtools")
}
