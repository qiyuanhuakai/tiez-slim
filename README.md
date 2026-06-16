<a name="chinese"></a>

![i18n](https://img.shields.io/badge/i18n-754%20keys%20%7C%20zh--CN%20100%25%20%7C%20en--US%20100%25-blue)
[English](#i18n) | [中文](#tiez-slim-linux)

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
- 应用黑名单与私有模式：可配置应用黑名单（支持通配符匹配 WM_CLASS），在指定应用中复制时自动跳过记录；私有模式可通过快捷键（默认 Ctrl+Alt+P）一键切换，启用后暂停所有剪贴板捕获，状态栏显示锁定图标。
- Primary Selection 跟踪：X11 环境下通过 XFixes 扩展监听鼠标选区（PRIMARY selection），与剪贴板（CLIPBOARD）独立存储；选区条目标注 `🖱️ 选区` 徽章，支持仅选区过滤；中键粘贴自动走 PRIMARY 通道。
- 正则 Actions 系统：基于正则表达式的自动化规则引擎，匹配剪贴板内容后可自动执行外部命令（如 URL 用浏览器打开、邮箱用邮件客户端打开）；支持工具栏 ⚡ 按钮快速触发、右键上下文菜单集成、自动触发模式（5 秒撤销窗口）；设置面板提供完整 CRUD、实时测试和测试运行。
- 导出/导入与自动备份：支持将全部历史、标签和设置导出为 JSON 文件，可从 JSON 文件导入（自动去重）；关闭应用时可自动备份，保留份数可配置；数据管理面板含导出/导入/备份/立即备份/打开备份目录等操作。
- 模糊搜索：基于 nucleo-matcher 的高性能模糊搜索，支持拼写纠错（如 `cllpboard` 匹配 `clipboard`）和中文模糊匹配；搜索结果按相关度排序，匹配字符高亮显示；可在设置中切换回传统子串搜索。
- 数据库加密（opt-in）：通过 `secure_storage` feature gate 启用，使用 AES-256-GCM 加密敏感条目；密钥通过系统 keyring（GNOME Keyring / KWallet）管理；启用后标记为敏感的条目自动加密存储，读取时自动解密；支持批量加密/解密迁移，带 LRU 缓存优化读取性能。
- KDE Connect 同步：默认编译启用，支持与 Android 设备通过 KDE Connect 协议同步剪贴板；设置面板含启用开关、设备 ID 显示、已发现设备列表；通过手机端设备列表配对后双向同步，带 echo 防重复机制。
- 国际化（i18n）：完整双语支持（zh-CN + en-US），754 个翻译键，100% 覆盖率；使用 rust-i18n v4，启动时自动检测系统语言，支持手动切换；所有用户可见字符串均通过 `t!()` 宏引用，无硬编码。

## 使用方法

从`release`中下载对应平台的安装包（支持`amd64`和`aarch64`平台，`deb`、`rpm`、`appimage`三种安装方式）并安装后即可使用

## 构建与运行

```bash
cargo run
cargo dev          # 等价于 cargo run --bin tiez-slim-linux -- dev
cargo ci           # 串行运行 cargo fmt/check/test/clippy/i18n
cargo run -- --db-path /path/to/clipboard.db
TIEZ_SLIM_LINUX_DB_PATH=/path/to/clipboard.db cargo run
cargo test         # 局部调试可用；提交前优先 cargo ci
cargo build --release
```

GUI 调试模式：

```bash
cargo dev
# 或显式转发参数
cargo run --bin tiez-slim-linux -- dev
# 兼容旧写法
cargo run -- --dev
# 或
TIEZ_SLIM_LINUX_DEV=1 cargo run
# 兼容旧变量
MYCLIPBOARD_DEV=1 cargo run
# 或编译期启用
cargo run --features devtools
```

## CLI 与脚本集成

`tiez-slim` 提供命令行工具 `tiez-cli`，可通过 Unix socket 与运行中的 GUI 实例通信。需要先启动 `tiez-slim` 才能使用 `tiez-cli`。

```bash
# 列出最近的剪贴板记录
tiez-cli list

# 搜索历史
tiez-cli search "关键词"

# 将指定条目复制到剪贴板
tiez-cli paste 42

# 查看服务器状态（含 KDE Connect 同步状态）
tiez-cli status

# 切换置顶状态
tiez-cli pin 42

# 设置标签
tiez-cli tag 42 work important

# 删除条目
tiez-cli delete 42

# 添加新条目
tiez-cli add "要保存的文本"

# JSON 格式输出（适合脚本处理）
tiez-cli --json list
tiez-cli --json status | jq '.sync'
```

配合 rofi/wofi 可实现键盘驱动的剪贴板选择器，详见 [docs/rofi-script.sh](docs/rofi-script.sh) 和 [Sway/Hyprland 集成指南](docs/sway-integration.md)。

---

`tiez-slim` ships with `tiez-cli`, a command-line tool that talks to a running GUI instance over a Unix domain socket. The `tiez-slim` app must be running first.

```bash
# List recent clipboard entries
tiez-cli list

# Search history
tiez-cli search "query"

# Copy an entry to clipboard by ID
tiez-cli paste 42

# Show server status (including KDE Connect sync state)
tiez-cli status

# Toggle pin state
tiez-cli pin 42

# Set tags on an entry
tiez-cli tag 42 work important

# Delete an entry
tiez-cli delete 42

# Add a new entry
tiez-cli add "text to save"

# JSON output (for scripting)
tiez-cli --json list
tiez-cli --json status | jq '.sync'
```

For rofi/wofi keyboard-driven clipboard picker integration, see [docs/rofi-script.sh](docs/rofi-script.sh) and the [Sway/Hyprland integration guide](docs/sway-integration.md).

## 与旧版差异

原始 `tiez-clipboard` 使用 React + Tauri 2 + WebView。`tiez-slim-linux` 对齐个人 `qiyuanhuakai/tiez-clipboard` 分支中的主界面视觉和核心数据模型，并用 Rust 原生能力补齐文本/富文本/图片/文件剪贴板、X11 全局呼出、鼠标中键、点击/键盘粘贴流程、系统托盘、边缘停靠、默认打开应用设置、彩色 emoji/符号入口、音效、字体 fallback 和可配置数据位置。

## KDE Connect 配对教程 / KDE Connect Pairing Guide

> KDE Connect 同步默认编译启用；无需额外 feature。Android 端 KDE Connect 没有通用二维码配对入口，请使用设备列表配对。

### 中文

1. 在 Android 手机安装 [KDE Connect](https://play.google.com/store/apps/details?id=org.kde.kdeconnect_tp)
2. 启动 tiez-slim，进入 **设置 → 同步** 面板
3. 打开「启用 KDE Connect」开关
4. 在手机 KDE Connect 的设备列表中选择 tiez-slim，并确认配对请求
5. 配对成功后，设备列表显示已连接设备名和状态
6. 之后在任意一端复制文本，另一端剪贴板会自动同步

### English

1. Install [KDE Connect](https://play.google.com/store/apps/details?id=org.kde.kdeconnect_tp) on your Android phone
2. Launch tiez-slim, go to **Settings → Sync** panel
3. Enable the "KDE Connect" toggle
4. Select tiez-slim from Android KDE Connect's device list and confirm the pairing request
5. After pairing, the device list shows the connected device name and status
6. From now on, copying text on either side automatically syncs to the other

> **注意 / Note**: 同步依赖两个设备在同一局域网。加密标记的敏感条目同步前会提示确认。
> Sync requires both devices on the same network. Sensitive (encrypted) entries prompt for confirmation before syncing.

## 加密模式启用 / Enabling Encryption

> 此功能需要编译时启用 `secure_storage` feature：`cargo build --features secure_storage`

加密模式使用 AES-256-GCM 加密标记为敏感的剪贴板条目，密钥由系统 keyring 管理。

```bash
# 编译时启用加密
cargo build --features secure_storage

# 或运行时启用
cargo run --features secure_storage
```

启用步骤：
1. 编译带 `secure_storage` feature 的版本
2. 启动应用，进入 **设置 → 隐私** 面板
3. 打开「安全存储」开关（系统 keyring 需可用）
4. 之后标记为 `sensitive`/`password`/`secret` 的条目会自动加密存储
5. 读取时自动解密，UI 无感知

> **注意**: 如果 keyring 不可用（如 SSH 会话），启动时会输出警告但不会 panic。关闭加密后重启会自动批量解密。

Encryption uses AES-256-GCM for sensitive clipboard entries, with keys managed by the system keyring (GNOME Keyring / KWallet).

```bash
# Build with encryption support
cargo build --features secure_storage
```

Steps:
1. Build with the `secure_storage` feature
2. Launch app, go to **Settings → Privacy**
3. Enable "Secure Storage" (system keyring must be available)
4. Entries tagged `sensitive`/`password`/`secret` are automatically encrypted at rest
5. Decryption is transparent on read

<a name="i18n"></a>

## 国际化 / Internationalization

`tiez-slim-linux` 现已支持国际化（i18n）。当前支持以下语言：

- **简体中文 (zh-CN)** — 源语言，完整覆盖
- **English (en-US)** — 完整翻译

翻译文件位于 `locales/` 目录：

```text
locales/
├── zh-CN.yml   # 源语言文件
└── en-US.yml   # 英文翻译
```

### 如何贡献翻译

欢迎贡献新的翻译或改进现有翻译：

1. **添加新键**：如果修改代码时新增了 UI 字符串，先在 `locales/zh-CN.yml` 中添加，再同步到 `locales/en-US.yml`
2. **添加新语言**：在 `locales/` 目录下创建 YAML 文件，命名格式为 `<语言代码>.yml`（如 `ja-JP.yml`），参考 `zh-CN.yml` 的键结构进行翻译
3. **验证**：运行以下命令检查一致性：

   ```bash
   cargo ci
   # 或仅检查翻译键
   bash scripts/i18n-check.sh
   ```

### 翻译规范

- 键名使用 `section.subsection.label` 命名空间格式
- 所有字符串值使用 `"双引号"` 括起
- 占位符使用 rust-i18n v4 的 `%{placeholder}` 格式（如 `%{count}`、`%{err}`）
- en-US 文件中**不能**出现中文（CJK）字符
