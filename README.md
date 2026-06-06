# tiez-slim-linux

Rust 原生的轻量剪贴板管理器。原始上游为 [`jimuzhe/tiez-clipboard v0.3.1`](https://github.com/jimuzhe/tiez-clipboard/tree/v0.3.1)，我曾经在此基础上进行了大量修改，并以个人的方式将其完整迁移到了 Linux 上：[`qiyuanhuakai/tiez-clipboard`](https://github.com/qiyuanhuakai/tiez-clipboard)。但我受够了`Tauri`了。它看似使用性能最强的 Rust，实际上还是在运行一个高性能开销（能使我的n100增加80%占用）、无比巨大（内存占用200Mb，峰值可达600－800Mb）的 Webview。相比`electron`，它的跨平台兼容性也堪称糟糕（在 Linux 和Windows 上，同一套主题的观感是完全不同的）。所以我新开了这个项目，目标是在 Linux 上去掉 React/Tauri/WebView 所有的开销，并保留 TieZ 的紧凑视觉，我喜爱的`macos`风格与高频剪贴板工作流。

## 当前实现

- 原生自绘 UI：`eframe/egui`，无系统标题栏，自绘 `tiez-slim` 顶栏可拖拽，支持圆角窗口、可切换应用边框和统一尺寸的 egui 手绘矢量工具图标。
- 字体默认优先使用个人喜好的 [`Maple Mono NF CN`](https://github.com/subframe7536/maple-font) ，自动回退 Noto / 思源 / WenQuanYi 等 CJK 字体；界面设置中可搜索系统字体并**自定义**“主要字体”和“备用字体”，缺字时自动走 egui fallback 链，内置 GNU Unifont 作为最后 Unicode 回退。
- Linux 剪贴板：`arboard` 轮询监听文本、富文本 HTML、图片和文件列表；文本自动识别 URL、代码、文件路径、图片/视频 data URL 等类型。文件条目写回时会尽力写入 GNOME `x-special/gnome-copied-files`，并始终写入通用文件列表/`text/uri-list`，便于 Nautilus/Thunar/Dolphin/PCManFM 等 GUI 文件管理器粘贴。
- 持久化：`rusqlite` + bundled SQLite；默认数据目录为 XDG 数据目录下的 `tiez-slim-linux`，并兼容读取旧 `myclipboard` 数据位置。
- 历史能力：搜索、类型过滤、标签过滤、置顶、删除、清空、标签编辑、左键/右键/Enter 按 TieZ 语义复制并粘贴。
- `tiez-slim` 风格主界面：380×680 竖向剪贴板浮窗、紧凑标签胶囊、单列历史流、类型徽标、敏感内容遮罩、左/右/上三向贴边边条隐藏；富文本、图片和文件历史项悬停时提供内容预览浮层。
- 表情包页面：顶部真实 emoji 按钮进入 `表情包` 全页，使用 Twemoji SVG 渲染完整 Twemoji 集合，并按 Unicode `emoji-test.txt` / CLDR 权威分组显示；组内继续分页以避免大组卡顿。收藏 Tab 支持文件选择、拖放图片和粘贴 data URL 添加表情包，保存到当前数据库旁的 `emoji_favorites/` 目录，点击表情或收藏会直接写入剪贴板并粘贴。
- 符号页面：顶部 `∑` 按钮进入 `符号` 全页，提供常用、箭头、数学、货币、框线、希腊、上下标/分数、技术、几何、块元素、标点/括号、星标/装饰、音乐/棋牌等 Unicode 符号，点击即可直接粘贴。
- 设置页面：顶部齿轮按钮进入全页设置，包含常规设置、快捷键设置、剪贴板设置、界面设置、默认打开程序、标签目录和数据管理；已接通项即时生效并持久化，字体选择支持可搜索下拉和系统字体重新扫描。
- macOS 视觉风格：`MacosTokens` 映射 TieZ CSS 变量为 Rust 常量，支持 Light/Dark/跟随系统 模式；设置页使用 `macos_toggle` 开关和 `macos_range_slider` 滑块。
- Linux 平台能力：X11 前台窗口识别、录制式全局快捷键（含鼠标中键）、StatusNotifierItem 系统托盘（支持热隐藏/重建）、窗口置顶、跟随鼠标呼出、四向边缘隐藏停靠、XDG 开机启动、可配置 `xdotool` 粘贴方式、XDG 默认打开程序下拉。
- 音效：可在常规设置中启用复制/粘贴提示音、调整音量，并可单独关闭粘贴音效；Linux 下优先调用 `aplay`，再回退到 `paplay`，不可用时静默降级。
- 简洁模式：隐藏复制时间，将标签和文本压缩到同一行，并仅在卡片悬停时显示操作工具栏；非简洁模式会保留复制时间和独立标签行，并在卡片标题元信息中显示来源应用，操作工具栏常显。

## 使用方法

从`release`中下载对应平台的安装包（支持`amd64`和`aarch64`平台，`deb`、`rpm`、`appimage`三种安装方式）并安装后即可使用

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

## 与旧版差异

原始 `tiez-clipboard` 使用 React + Tauri 2 + WebView。`tiez-slim-linux` 对齐个人 `qiyuanhuakai/tiez-clipboard` 分支中的主界面视觉和核心数据模型，并用 Rust 原生能力补齐文本/富文本/图片/文件剪贴板、X11 全局呼出、鼠标中键、点击/键盘粘贴流程、系统托盘、边缘停靠、默认打开应用设置、彩色 emoji/符号入口、音效、字体 fallback 和可配置数据位置。
