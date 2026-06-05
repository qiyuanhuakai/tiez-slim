# tiez-slim-linux

Rust 原生轻量版 TieZ 剪贴板管理器。原始上游来自 [`jimuzhe/tiez-clipboard`](https://github.com/jimuzhe/tiez-clipboard)，本项目基于我的大量修改分支（[`qiyuanhuakai/tiez-clipboard`](https://github.com/qiyuanhuakai/tiez-clipboard)）迁移核心能力，目标是在 Linux 上去掉 React/Tauri/WebView 开销，并保留 TieZ 的紧凑视觉，我喜爱的macos风格与高频剪贴板工作流。

## 当前实现

- 原生自绘 UI：`eframe/egui`，无系统标题栏，自绘 `tiez-slim` 顶栏可拖拽，支持圆角窗口、可切换应用边框和统一尺寸的 egui 手绘矢量工具图标。
- 字体默认优先使用个人喜好的 `Maple Mono NF CN` / Noto / 思源 / WenQuanYi 等 CJK 字体；界面设置中可搜索系统字体并分别选择“主要字体”和“备用字体”，缺字时自动走 egui fallback 链，内置 GNU Unifont 作为最后 Unicode 回退。
- Linux 剪贴板：`arboard` 轮询监听文本、富文本 HTML、图片和文件列表；文本自动识别 URL、代码、文件路径、图片/视频 data URL 等类型。文件条目写回时会尽力写入 GNOME `x-special/gnome-copied-files`，并始终写入通用文件列表/`text/uri-list`，便于 Nautilus/Thunar/Dolphin/PCManFM 等 GUI 文件管理器粘贴。
- 持久化：`rusqlite` + bundled SQLite；默认数据目录为 XDG 数据目录下的 `tiez-slim-linux`，并兼容读取旧 `myclipboard` 数据位置。
- 历史能力：搜索、类型过滤、标签过滤、置顶、删除、清空、标签编辑、左键/右键/Enter 按 TieZ 语义复制并粘贴。
- `tiez-slim` 风格主界面：380×680 竖向剪贴板浮窗、紧凑标签胶囊、单列历史流、类型徽标、敏感内容遮罩、左/右/上三向贴边边条隐藏；富文本历史项悬停时提供富文本内容预览浮层。
- 表情包页面：顶部真实 emoji 按钮进入 `表情包` 全页，使用 Twemoji SVG 渲染彩色 emoji，支持 EMOJI/收藏 Tab，内置 TieZ 常用 emoji 分组，Tab 状态随设置保存。
- 符号页面：顶部 `∑` 按钮进入 `符号` 全页，提供常用、箭头、数学、货币、框线、希腊、上下标/分数、技术、几何、块元素、标点/括号、星标/装饰、音乐/棋牌等 Unicode 符号，点击即可复制。
- 设置页面：顶部齿轮按钮进入全页设置，包含常规设置、快捷键设置、剪贴板设置、界面设置、默认打开程序、过滤/标签目录和数据管理；已接通项即时生效并持久化，字体选择支持可搜索下拉和系统字体重新扫描。
- macOS 视觉风格：`MacosTokens` 映射 TieZ CSS 变量为 Rust 常量，支持 Light/Dark 双模式；设置页使用 `macos_toggle` 开关和 `macos_range_slider` 滑块。
- Linux 平台能力：X11 前台窗口识别、录制式全局快捷键（含鼠标中键）、StatusNotifierItem 系统托盘（支持热隐藏/重建）、窗口置顶、跟随鼠标呼出、四向边缘隐藏停靠、XDG 开机启动、可配置 `xdotool` 粘贴方式、XDG 默认打开程序下拉。
- 音效：可在常规设置中启用复制/粘贴提示音、调整音量，并可单独关闭粘贴音效；Linux 下优先调用 `aplay`，再回退到 `paplay`，不可用时静默降级。

## 构建与运行

```bash
cargo run
cargo run -- --db-path /path/to/clipboard.db
TIEZ_SLIM_LINUX_DB_PATH=/path/to/clipboard.db cargo run
cargo test
cargo build --release
```

GUI 调试模式：

```bash
cargo run -- --dev
# 或
TIEZ_SLIM_LINUX_DEV=1 cargo run
# 兼容旧变量
MYCLIPBOARD_DEV=1 cargo run
# 或编译期启用
cargo run --features devtools
```

设置页面位于顶部矢量齿轮按钮。常规设置可启用开机启动（写入 `~/.config/autostart/tiez-slim-linux.desktop`，以 `--minimized` 静默启动）。搜索框可设置为“始终显示搜索框”；关闭后，在历史列表顶端继续向上滚动可临时唤出搜索框，无搜索/过滤时向下滚动会隐藏。历史项左键会写入剪贴板并粘贴，右键会尽量带格式写入并粘贴，方向键选择后按 Enter 走同一粘贴流程；`粘贴后删除` 优先于 `粘贴后移到顶部`。

简洁模式会隐藏复制时间，将标签和文本压缩到同一行，并仅在卡片悬停时显示操作工具栏；非简洁模式会保留复制时间和独立标签行，并在卡片标题元信息中显示来源应用，操作工具栏常显。

字体设置位于 `界面设置`。`主要字体` 为空时使用自动 CJK 优先策略；`备用字体` 为空时优先扫描系统 Unifont/GNU Unifont，找不到时使用内置 `assets/fonts/unifont-17.0.04.otf`。备用字体位于 fallback 链末尾，用于当前字体不支持的符号；非等宽字体不会污染 Monospace 链。

Linux 需要图形环境。当前优先支持 X11；全局键盘快捷键使用 X11 `grab_key` 注册，鼠标中键使用 `grab_button(Button2)` 注册，跟随鼠标与边缘停靠使用 X11 `query_pointer` + egui `ViewportCommand::OuterPosition`。粘贴模拟使用 `xdotool`，因此运行环境需安装 `xdotool`。音效需要 `aplay` 或 `paplay` 之一（可选）。数据库默认位于 XDG 数据目录，也可通过 `--db-path`、`TIEZ_SLIM_LINUX_DB_PATH` 或设置页保存重启后路径来配置；旧 `MYCLIPBOARD_DB_PATH` 仍作为兼容别名读取。`--minimized` 会让应用启动后保持隐藏，可配合 XDG autostart 使用。

## GitHub

项目名和仓库名统一为 `tiez-slim-linux`：

```text
https://github.com/qiyuanhuakai/tiez-slim-linux
```

## 与旧版差异

原始 `tiez-clipboard` 使用 React + Tauri 2 + WebView。`tiez-slim-linux` 对齐个人 `qiyuanhuakai/tiez-clipboard` 分支中的主界面视觉和核心数据模型，并用 Rust 原生能力补齐文本/富文本/图片/文件剪贴板、X11 全局呼出、鼠标中键、点击/键盘粘贴流程、系统托盘、边缘停靠、默认打开应用设置、彩色 emoji/符号入口、音效、字体 fallback 和可配置数据位置。
