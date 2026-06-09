# tiez-slim-linux 产品路线图（2026 H2）

> **生成日期**：2026-06-06  
> **最新更新**：2026-06-09  
> **版本**：v0.3.2（Phase 0-3 已完成，10/10 功能交付；性能与布局维护完成）  
> **范围**：基于 2026-06 的功能差距分析与同步方案调研，规划 10 项优先功能的实施路径  
> **v0.3 变更**：确认 i18n 作为 Phase 0 首要任务；详细规划按真实交付顺序重排；Phase 1 改为隐私/备份/自动化/X11/搜索并行补强；后续功能一律不得新增硬编码用户可见字符串  
> **v0.3.1 变更**：Phase 0 #8 i18n 已交付：752 keys（zh-CN 源 + en-US 翻译，100% 覆盖），rust-i18n v4 + `%{var}` 占位符，启动时 `set_app_locale()` 修复 follow-system 语义，统一 `searchable_combo_row` 自定义下拉控件。  
> **v0.3.2 变更**：KDE Connect 改为默认编译启用并修复 feature 测试；Primary Selection 去重为单一平台 watcher；搜索刷新加 debounce；Emoji 收藏页取消每帧扫盘；设置折叠状态补齐迁移；卡片标签限量显示 +N，顶部内容类型过滤固定三行布局。  
> **关联文档**：`AGENTS.md`（项目知识库）、`README.md`（用户文档）

---

## 0. 执行摘要

`tiez-slim-linux` 已经完成原生 UI、富文本/图片/文件历史、系统托盘、X11 快捷键、贴边隐藏、字体 fallback、emoji/符号页、音效和数据路径迁移等基础能力。下一阶段会持续新增大量设置项、菜单、状态提示和错误消息；如果继续硬编码中文，后续 Actions、Primary Selection、同步、CLI、Snippet 都会产生重复返工。因此本路线图确认 **i18n 是 Phase 0 首要前置**：先建立字符串治理，再推进功能扩张。

| 优先级 | 项 | 功能 | 价值 | 工作量 | 阶段 | 排序理由 |
|---|---|---|---|---|---|---|
| ✅ P0-1 | 8 | i18n（zh + en） | **极高** | L | **Phase 0 完成** | 已交付：752 keys 100% 覆盖，rust-i18n v4 + `%{var}`（见 §2 #8 实际产出） |
| ✅ P1-1 | 3 | 应用黑名单 + 私有模式 | **极高** | S | **Phase 1 完成** | 已交付：黑名单、私有模式、热键与设置联动 |
| ✅ P1-2 | 5 | Export/Import + 自动备份 | **极高** | S | **Phase 1 完成** | 已交付：导入导出、自动备份、保留策略 |
| ✅ P1-3 | 1 | 正则→命令 Actions 系统 | **极高** | M+ | **Phase 1 完成** | 已交付：工具栏、右键菜单、auto-trigger、设置编辑器 |
| ✅ P1-4 | 2 | Primary Selection (X11) 跟踪 | 高 | M | **Phase 1 完成** | 已交付：XFixes 事件订阅 + 轮询降级；v0.3.2 去除重复轮询 |
| ✅ P1-5 | 10 | fuzzy search | 高 | S-M | **Phase 1 完成** | 已交付：nucleo fuzzy + 高亮；v0.3.2 加搜索 debounce |
| ✅ P2-1 | 4 | KDE Connect 集成 | 中-高 | M-L | **Phase 2 完成** | 已交付：默认编译启用，设置页同步开关与 QR/状态 UI |
| ✅ P2-2 | 7 | 数据库加密 | 中-高 | M | **Phase 2 完成** | 已交付：feature-gated secure storage 与敏感内容策略 |
| ✅ P2-3 | 6 | CLI 配套（`tiez-cli`） | 中 | M | **Phase 2 完成** | 已交付：Unix socket IPC 与 CLI 子命令 |
| ✅ P3-1 | 9 | Snippet 模板 | 中 | M | **Phase 3 完成** | 已交付：模板、变量插值、设置 UI、picker 热键、IPC |

**总工作量估算**：约 **53 个工作日**（约 10.5 周全职）；截至 v0.3.2，10 项规划功能均已进入源码实现，后续重点转为稳定性、性能与真实设备 QA。

**为什么 i18n 必须最先做**：tiez-slim 当前用户可见字符串仍以中文硬编码为主。ROADMAP 后续每个功能都会新增设置页、toast、错误提示、托盘菜单和 README 章节；若等功能做完再抽取，会同时重写 UI、文案和测试。Phase 0 投入 1-2 周建立 `tr!()`/locale/缺失键检查后，后续功能只需按规范新增 key。

---

## 1. 实施阶段与依赖图

### 推荐阶段划分

```
Phase 0（i18n 基础设施，1-2 周）
────────────────────────────
• #8 i18n（zh + en）  ← 所有后续用户可见字符串必须用 tr!() / locale key

Phase 1（本地核心能力，4-6 周）      Phase 2（扩展能力，4-6 周）        Phase 3（差异化，3-4 周）
──────────────────────             ─────────────────────             ──────────────────
• #3 应用黑名单 + 私有模式          • #4 KDE Connect                 • #9 Snippet 模板
• #5 Export/Import + 备份           • #7 数据库加密                  （模式镜像 saved_tags）
• #1 正则 Actions (Option D)        • #6 CLI 配套
• #2 Primary Selection (XFixes)
• #10 fuzzy search
```

### 依赖关系（Mermaid）

```mermaid
graph TD
    P0[Phase 0<br/>i18n 基础设施] --> P1[Phase 1<br/>本地核心能力]
    P0 --> P2[Phase 2<br/>扩展能力]
    P0 --> P3[Phase 3<br/>差异化]
    P1 --> P2
    P1 --> P3
    P2 --> P3

    P0 --> A8[8. i18n]

    P1 --> A3[3. App 黑名单 + 私有]
    P1 --> A5[5. Export/Import]
    P1 --> A1[1. 正则 Actions<br/>Option D]
    P1 --> A2[2. Primary Selection<br/>XFixes]
    P1 --> A10[10. fuzzy search]

    A8 -.所有新增文案必须接入.-> A3
    A8 -.所有新增文案必须接入.-> A5
    A8 -.所有新增文案必须接入.-> A1
    A8 -.所有新增文案必须接入.-> A2
    A8 -.所有新增文案必须接入.-> A10
    A5 -.配置可导出.-> A1
    A3 -.复用窗口识别.-> A2

    P2 --> A4[4. KDE Connect]
    P2 --> A7[7. DB 加密]
    P2 --> A6[6. CLI 配套]
    A8 -.所有新增文案必须接入.-> A4
    A8 -.所有新增文案必须接入.-> A7
    A8 -.CLI 文案命名空间.-> A6
    A4 -.状态查询.-> A6
    A7 -.敏感数据策略.-> A4

    P3 --> A9[9. Snippets]
    A8 -.settings.snippets.*.-> A9
    A1 -.执行/插入能力.-> A9
    A6 -.snippet 子命令.-> A9
```

### 关键决策点

| 决策 | 选项 | 推荐 | 影响范围 |
|---|---|---|---|
| **排序基准** | 功能优先 / 文案治理优先 | **先 i18n，再功能扩张** | Phase 0 |
| **i18n 框架** | `rust-i18n` (YAML) / `fluent-rs` (Fluent) / 手卷 | `rust-i18n`（轻量、YAML 友好） | **Phase 0 完成** |
| **后续功能文案策略** | 允许临时硬编码 / 必须先加 key | **必须先加 locale key** | Phase 1+ 强制 |
| **Actions UI 模式** | A 右键菜单 / B 工具栏 chip / C 弹出 / D 三者兼有 | **D**（Klipper+CopyQ 组合） | Phase 1 |
| **Actions 命令执行** | `std::process::Command` / `tokio::process` 异步 | `tokio::process`（不阻塞 UI） | Phase 1 |
| **右键键位迁移** | 保留右键=富文本粘贴 / 右键=菜单+Shift+右键=原行为 | **右键=菜单，Shift+右键=原行为** | Phase 1 |
| **Primary Selection 监听** | `x11rb` XFixes 事件 / `arboard` 轮询 | **`x11rb` XFixes**（事件驱动、零开销） | Phase 1 |
| **KDE Connect 集成深度** | 完整 daemon（替代 kded） / 仅剪贴板子集 | 仅剪贴板子集（最小可用） | Phase 2 |
| **DB 加密密钥存储** | `keyring` crate（OS keyring） / 用户密码派生 | `keyring`（透明、用户体验好） | Phase 2 |
| **CLI IPC 通道** | Unix domain socket / 命名管道 / HTTP localhost | Unix domain socket（XDG_RUNTIME_DIR） | Phase 2 |
| **Snippets 架构** | 复用 saved_tags 代码 / 模式镜像 | **模式镜像**（独立表 saved_snippets，独立 UI 模式） | Phase 3 |

---

## 2. 详细规划（按 v0.3 推荐交付顺序）

---

### #8 i18n（zh + en）— **Phase 0 基础设施** ✅ 已完成（2026-06-07）

> **状态**：✅ 完成（752 keys, 100% 覆盖）  
> **实际工作量**：~1.5 周（含 review 修复）  
> **v0.3.1 之前**：原文按 Phase A-E 规划交付；实际一次性集成 5 个阶段（框架 + 提取 + UI + CLI + 文档）+ 2 个用户肉眼 review 修复 + 1 个微调 + 1 个 fmt 修复

#### 实际产出

**Commits**（按时间顺序）:
```
ecbf7dd chore(i18n): integrate rust-i18n v4 framework with log-miss-tr
07c07b9 feat(i18n): extract zh-CN + en-US translations
dafca2a chore(i18n): add CI check + bilingual README + coverage badge
edbc942 feat(i18n): add language dropdown UI + emoji group locale switching
1d10801 fix(i18n): apply locale at startup to fix default English UI
a79c0bf fix(i18n): trash icon mapping and %{var} placeholder syntax
1ad6501 fix(i18n): default to follow-system language and unify dropdown widget
cba4f1b style: apply cargo fmt across codebase
7dfc04c chore: address code review feedback
```

**文件改动**:
| 模块 | 改动 |
|---|---|
| `Cargo.toml` | 新增 `rust-i18n = "4"`,`log-miss-tr` feature gate |
| `src/i18n.rs` | 新模块（100 行）:`set_app_locale` / `current_locale` / `detect_system_locale` / `tr` helper |
| `src/app.rs` | 305 处 CJK → `t!()`；新增 `language_search: String`；默认 `follow-system`；设置面板改用 `searchable_combo_row`；`set_app_locale` 在 `ClipboardApp::new` 启动时调用 |
| `src/clipboard.rs` | 21 处 CJK → `t!()` |
| `src/model.rs` | 21 处 CJK → `t!()`（含 sensitive 错误消息） |
| `src/storage.rs` | 11 处 CJK → `t!()` |
| `src/main.rs` | 2 处 + `i18n!("locales", fallback = "en-US")` |
| `src/platform/{mod,linux,windows}.rs` | 51 处 CJK → `t!()`（tray 菜单、错误消息） |
| `locales/zh-CN.yml` | **752 keys 源文件**（`common.*` / `settings.*` / `status.*` / `error.*` / `history.*` / `emoji.*` / `symbol.*` / `clipboard.*` / `platform.*` / `tooltip.*` / `sound.*` 等） |
| `locales/en-US.yml` | **752 keys 翻译文件**，100% 覆盖 |
| `scripts/i18n-check.sh` | 用 `en_nonempty / total` 计算真实 coverage，1 位小数 |
| `README.md` | 中英双语，加 `text` language tag（markdownlint MD040） |

**关键设计决策**:
- **rust-i18n v4 占位符**:用 `%{var}`（不是 `{var}`），由 4.1.0 文档明确
- **默认语言**:`follow-system`，启动时 `detect_system_locale()` 从 LANG / LC_MESSAGES 解析
- **持久化语义修复**:选择 "follow-system" 存储原始字符串而非立即 resolve 为具体 locale；启动时再次 resolve 以跟踪系统变更
- **下拉控件统一**:语言选择改用 `searchable_combo_row` 自定义控件（与 paste_method / font 等一致），不用 `egui::ComboBox`
- **缺失 key 处理**:`tr()` helper 改用 `log-miss-tr` feature 替代 debug 短路分支；前者日志到 stderr，后者会误导调用者
- **CJK 输入**:zh-CN 中可保留少量程序内常量（如标点符号 `"："` → ASCII `":"` 是 locale 无关的标点）

**验证**（commit 7dfc04c 之后）:
- `cargo fmt --all -- --check` exit 0
- `cargo clippy -- -D warnings` exit 0
- `cargo test` 77/77 passed
- `bash scripts/i18n-check.sh` 输出 `zh-CN: 752 keys, en-US: 752 non-empty, coverage: 100.0%`
- 启动日志（5 场景全验证）:
  - 默认 DB + LANG=zh_CN → `locale=zh-CN, zh-CN=100%, en-US=100%`
  - DB 显式 en-US → `locale=en-US`
  - follow-system + LANG=en_US → `locale=en-US`
  - follow-system + LANG=zh_CN → `locale=zh-CN`
  - 6s 渲染无 panic、无字面量 `{count}` 残留
- Final Wave 4 个评审员全部 APPROVE（F1 Plan Compliance / F2 Code Quality / F3 Real Manual QA / F4 Scope Fidelity）

#### 功能
将所有硬编码中文（标签、按钮、消息、错误、tooltip、托盘菜单、Emoji 组名等）抽取为 i18n 键。引入 `rust-i18n` 框架，配置文件 `locales/zh-CN.yml` + `locales/en-US.yml`。新增 `language` 设置（zh-CN / en-US / follow-system）。

#### 价值
- **国际化基础**：当前 100% 中文 = 国际用户完全无法使用
- **社区贡献门槛降低**：英文用户能直接贡献 PR
- **维护性收益**：所有字符串集中管理 → 减少重复 → 减少修改遗漏
- **关键收益 — 避免 Phase 1+ 重复返工**：若在 Phase 1+ 后期做，每个新功能都要在 zh/en 两条线路上重复添加、修改、测试字符串。**Phase 0 集中 1-2 周投入，换来后续所有功能的零翻译成本**
- **对标**：fork 已有（zh/en/tw），tiez 没移植——是**最大**国际化短板

#### 实现规划

**Phase A：i18n 框架选型与集成（1 天）**
- `Cargo.toml` 添加 `rust-i18n = "3"` （YAML 配置，零运行时依赖）
- 初始化：
  ```rust
  // src/i18n.rs
  use rust_i18n::i18n;
  i18n!("locales");
  ```
- `locales/zh-CN.yml` + `locales/en-US.yml` 初始空文件
- 运行时根据 `language` 设置选 locale
- 暴露给所有模块的 `t!()` 宏

**Phase B：字符串提取（3-4 天，最大块）**
- 全量 grep `src/**/*.rs` 提取所有用户可见字符串（基于 `bg_8c3a040f` 调研，估算 30+ 处）
- 分类：UI 标签 / 按钮 / 错误消息 / 状态消息 / tooltip / 设置项名 / 菜单项 / 日志（仅用户可见部分）
- 命名约定：`section.subsection.label`（如 `settings.appearance.theme_mode`）
- 中文为源，英文为翻译；提供 fallback 链 `zh-CN → en-US → 原始 key`

**Phase C：UI 适配（1-2 天）**
- `src/app.rs` 全面替换 `tr!("key")` 宏
- `Settings: language` 字段：zh-CN / en-US / follow-system
- `detect_system_language()` 从 `LANG`/`LANGUAGE` 环境变量读
- emoji 组名、符号组名（emoji_data.rs）也走 i18n
- 关键界面：
  - 设置面板 0-6 全部标题/选项
  - 顶部工具栏图标 tooltip
  - 状态栏消息
  - 错误对话框
  - 拖放提示
  - emoji/符号组名

**Phase D：CLI 与脚本（0.5 天）**
- 添加 `scripts/i18n-check.sh` 扫描缺失键（仿 fork 的 `i18n:check`）
- 在 `cargo test` 中集成：缺翻译则测试失败
- 关键 i18n 键命名空间预留（为 Phase 1+ 准备）：
  - `settings.actions.*`（#1）
  - `settings.snippets.*`（#9）
  - `sync.kde_connect.*`（#4）
  - `cli.*`（#6）

**Phase E：文档更新（0.5 天）**
- README 双语（zh-CN 默认 + en-US 链接）
- 贡献指南：如何添加新翻译
- 翻译覆盖率徽章（README 显示 `zh-CN: 100% / en-US: 85%`）

#### 验收标准
- [x] 切换语言为 `en-US` → 所有 UI 标签、按钮、消息、tooltip 显示英文
- [x] 切换语言为 `zh-CN` → 全部恢复中文
- [x] 缺失翻译键 → 显式标记为 `[MISSING: key.path]` 而非崩溃（实际通过 `log-miss-tr` feature 日志到 stderr）
- [x] 启动日志显示当前 locale 与翻译覆盖率
- [x] `i18n-check.sh` 在 CI 中可运行（输出 `zh-CN: 752 keys, en-US: 752 non-empty, coverage: 100.0%`）
- [x] Phase 1+ 新增功能自动继承 i18n 框架（开发者只需 `t!()` 宏）

#### 风险与权衡
| 风险 | 缓解 |
|---|---|
| 字符串遗漏（特别是新功能） | 强制 `tr!()` 宏 + code review checklist + CI 检查 |
| 复数/性别/上下文 | rust-i18n 支持 ICU MessageFormat，对复杂场景用 `tr!("key", count = n)` |
| Emoji 组名与数据耦合 | emoji_data.rs 的 group name 改为运行时解析 |
| 翻译质量（机器翻译） | 关键字符串人工校对 |
| 文件大小 | 启动时按需加载 locale（rust-i18n 默认行为） |
| Phase 0 推迟可见功能 | 净收益为正：避免后续每个功能重复 i18n 工作的总成本更高 |

#### 依赖
- **前置**：无
- **后置**：所有 Phase 1+ 功能**必须**使用 `tr!()` 宏（建立 code review 强制项）

#### 工作量
**L（7-10 天，1-2 周）**

---

---

### #3 应用黑名单 + 私有模式

#### 功能
- **应用黑名单**：用户在设置中列出 `WM_CLASS` 模式（支持通配），这些应用获得焦点时**不记录**剪贴板
- **私有模式**：一键切换的开关，启用时暂停所有记录。UI/托盘/状态栏有清晰指示器。配全局热键快速切换

#### 价值
- **隐私信任基础**：用户敢在有敏感数据的 app 中复制文本而不会污染历史
- **对标**：CopyQ（窗口忽略）、Maccy（per-app blocklist）、Clipboard Indicator（私有模式）
- **使用场景**：
  - 1Password / KeePassXC / Bitwarden → 黑名单：避免「清空剪贴板后被记录」
  - 终端里复制 API key → 一键 `Ctrl+Alt+P` 私有模式 → 复制 → 关私有
  - 演示/录屏时 → 启用私有模式避免泄露隐私数据

#### 实现规划

**Phase A：窗口识别 API（0.5 天）**
- `src/platform/linux.rs` 新增 `pub fn active_window_class() -> Option<String>` 返回 `_NET_WM_NAME` 或 `WM_CLASS`
- 复用现有 `active_window_title()` 路径

**Phase B：黑名单（1 天）**
- `src/model.rs` 的 `Settings` 新增：
  ```rust
  pub app_exclusion_list: Vec<String>,  // 模式：精确匹配 + 通配符
  ```
- `src/clipboard.rs` 在 capture 入口加：
  ```rust
  if settings.app_exclusion_list.iter().any(|p| glob_match(p, &active_class)) {
      return None;
  }
  ```
- glob 匹配用 `globset` crate（轻量）

**Phase C：私有模式（0.5-1 天）**
- `Settings` 新增 `private_mode: bool`
- `src/clipboard.rs` 入口检查 `private_mode` → 直接 return
- **状态可见性**（关键）：
  - 托盘图标：私有模式时图标加红点 / 颜色变化
  - 状态栏：显示「🔒 私有模式」
  - 主窗口标题栏：显示「（私有模式 - 暂停记录）」副标题
- 全局热键 `private_mode_hotkey`（默认 `Ctrl+Alt+P`）
- UI 切换按钮：托盘菜单 + 状态栏按钮

**Phase D：UI（1 天）**
- 设置面板「隐私」分组（合并现有 sensitive detection）
  - 启用敏感检测 / 敏感类型多选 / 自定义规则
  - **新增**：启用应用黑名单 + 列表编辑器（带「当前活跃窗口」快捷添加按钮）
  - **新增**：启用私有模式（开关 + 全局热键）
- 「当前活跃窗口」按钮：点击自动填入当前焦点窗口的 WM_CLASS

**Phase E：白名单（可选 v1.1）**
- 配合黑名单增加「仅记录这些 app」模式，互斥
- 适合极端隐私场景

#### 验收标准
- [ ] 设置添加 `keepassxc` → 在 KeePassXC 中复制密码 → tiez 历史**无**新增条目
- [ ] `Ctrl+Alt+P` → 状态栏变红「🔒 私有」→ 复制任意内容 → 历史**无**新增 → 再按一次恢复
- [ ] 通配符模式 `*password*` 正确匹配 `org.keepassxc.KeePassXC`
- [ ] 「当前活跃窗口」按钮一键填入真实 WM_CLASS
- [ ] 黑名单生效时托盘图标有视觉变化

#### 风险与权衡
| 风险 | 缓解 |
|---|---|
| WM_CLASS 在 X11 各 DE 不一致 | 文档引导用户用「当前活跃窗口」按钮精确获取 |
| 通配符匹配性能 | 启动时预编译 globset，O(1) 匹配 |
| 用户忘记关闭私有模式 | 30 分钟无活动自动恢复？或仅托盘高亮 |
| 极端隐私用户期待无痕迹 | 提供「完全无历史」hardcore 模式（不写 DB） |

#### 依赖
- **前置**：#8 i18n（隐私状态、托盘提示、设置页文案必须先入 locale）
- **后置**：可与 #2 Primary Selection 协同（primary 也走黑名单）
- **复用**：与 #2 共享窗口识别 API

#### 工作量
**S（2-3 天）**

---

---

### #5 Export/Import + 自动备份

#### 功能
- **手动导出/导入**：JSON 格式，可选择「仅设置 / 仅历史 / 全部」
- **自动备份**：优雅退出时自动写 `backups/clipboard-{ISO timestamp}.json`，保留最近 N 份（默认 10）

#### 价值
- **数据信任基础设施**：用户积累 500+ 钉选/标签条目后，**没有**备份/导出 = 临时缓存心理
- **跨设备迁移**：用户换电脑的核心路径
- **对标**：CopyQ / GPaste 标配

#### 实现规划

**Phase A：数据模型（0.5 天）**
- 新文件 `src/storage_io.rs`
- `ExportBundle` 结构：
  ```rust
  pub struct ExportBundle {
      pub schema_version: u32,    // 当前 1
      pub exported_at: i64,
      pub app_version: String,
      pub settings: HashMap<String, Value>,
      pub tags: Vec<Tag>,
      pub entries: Vec<ClipboardEntry>,
      pub actions: Vec<Action>,   // 与 #1 协同
      pub emoji_favorites: Vec<PathBuf>,
  }
  ```

**Phase B：导出/导入 API（1-2 天）**
- `export_to(path, scope: ExportScope) -> Result<usize>`
- `import_from(path, mode: ImportMode) -> Result<ImportStats>`
  - `ImportMode`: Merge（按 content_hash 去重）/ Replace（清空再导入）
- 事务：导入用单个 SQLite 事务，失败回滚
- 进度：流式写入大文件（> 100 MB 不常见但要支持）

**Phase C：自动备份（1 天）**
- 在 `main.rs` 的 `Drop` 实现 + Ctrl+C handler 中触发
- 路径：`{data_dir}/backups/clipboard-{timestamp}.json`
- 保留策略：扫描目录，超过 N 份时按 mtime 删最旧
- 设置：`auto_backup_enabled: bool`, `backup_retention_count: i32`

**Phase D：UI（0.5-1 天）**
- 设置面板「数据管理」分组（已有）新增：
  - 「导出到文件」按钮 → 文件选择对话框（zenity/kdialog）
  - 「从文件导入」按钮 → 进度条 + 合并/替换选择
  - 自动备份开关 + 保留份数

#### 验收标准
- [ ] 导出 1000 条历史 → JSON 文件 < 10 MB → 重新导入 → 所有条目按 content_hash 去重还原
- [ ] 优雅退出（点关闭按钮）→ 备份目录出现新文件
- [ ] 备份数 > 10 → 自动删最旧
- [ ] 损坏的 JSON 文件导入时给出明确错误，不损坏现有 DB
- [ ] 跨版本导出文件包含 `schema_version`，未来 schema 升级可写迁移代码

#### 风险与权衡
| 风险 | 缓解 |
|---|---|
| 大文件内存爆 | 流式 serde_json |
| 附件文件（如有）未导出 | v1 仅导出 DB 内容；附件路径用相对路径，文档说明手动迁移 |
| 加密数据无法导出 | 与 #7 协同：导出时检测加密状态，提供「先解密再导出」选项 |
| 跨平台路径 | 使用 `dirs` crate 解析 XDG 路径 |

#### 依赖
- **前置**：#8 i18n（导入/导出错误、进度和确认文案必须先入 locale）
- **后置**：#1 Actions 导出复用此机制、#4 KDE Connect 设备列表可导出

#### 工作量
**S（2-3 天）**

---

---

### #1 正则→命令 Actions 系统（Option D）

#### 功能
用户可在设置中定义一组「模式 → 命令」规则。匹配的动作通过**三层**呈现：
1. **工具栏 ⚡ 按钮**（条件渲染）：当且仅当内容匹配时，附加在现有 Pin/Open/Delete 右侧
2. **新建右键菜单**：列出所有匹配动作 + 内置动作（复制/钉选/删除/编辑）
3. **可选自动触发**：每条 action 独立开关，启用后捕获即执行

#### 价值
- **核心差异化**：Klipper / CopyQ / Clipman 的招牌功能，把「剪贴板查看器」升级为「自动化平台」
- **使用场景**：
  - 复制 URL → 一键在 Firefox 打开 / 下载视频 / 复制标题
  - 复制文件路径 → 一键 `xdg-open` / 用 VSCode 打开
  - 复制 JSON → 一键格式化 / 校验
  - 复制颜色代码 `#RRGGBB` → 一键显示预览
- **对标调研结论**：6 款主流剪贴板管理器无单一模式胜出——CopyQ 用 A+C+E+全局快捷键，Klipper 用 E+A，Ditto 用 A+C，最佳是 **Option D（三者兼有）**

#### 实现规划

**Phase A：数据模型与存储（1-2 天）**
- `src/model.rs` 新增 `Action` 结构体：
  ```rust
  pub struct Action {
      pub id: i64,
      pub name: String,            // "用 Firefox 打开"
      pub pattern: String,         // "^https?://"
      pub is_regex: bool,          // true (regex) | false (glob/literal)
      pub command: String,         // "firefox %1"
      pub is_automatic: bool,      // 捕获即自动执行
      pub is_primary: bool,        // 在工具栏 ⚡ 位置显示
      pub icon: Option<String>,    // emoji 字符或图标名
      pub enabled: bool,
      pub order: i32,              // 菜单排序
      pub created_at: i64,
      pub updated_at: i64,
  }
  ```
- `src/storage.rs` 新增表 `actions`，迁移函数 + CRUD API
- 初始种子：内置 2-3 条示例（URL→`xdg-open`，文件路径→`xdg-open`）

**Phase B：正则编译与匹配（1 天）**
- `src/actions/mod.rs` 新模块
- 使用 `regex` crate（已依赖）编译 pattern，启动时缓存编译结果
- 匹配器：`fn match_actions(content: &str) -> Vec<&Action>` 返回所有匹配
- 验证：启动时检测所有用户规则的合法性，无效规则写入 `logs`，UI 标红

**Phase C：执行引擎（1-2 天）**
- 使用 `tokio::process::Command`（不阻塞 UI）
- 占位符替换：
  - `%1`, `%2`...  → 正则捕获组
  - `%clipboard%`  → 完整剪贴板内容
  - `%date%`, `%time%` → ISO 格式
- 安全：argv 模式（不拼 shell），`%` 转义防注入
- 异步执行 + 超时（默认 30 秒可配置）
- 错误处理：失败写入 `app.last_action_error` 设置，UI 显示通知

**Phase D：工具栏扩展（1 天）**
- 修改 `src/app.rs:32-37` 常量：
  - `CARD_ACTION_WIDTH: 92.0 → 130.0`（容纳第 4 个按钮）
- 修改 `src/app.rs:2699-2763`：
  - 保留 Pin/Open/Delete 三按钮
  - 在 `Delete` 后追加 `action_bar_button("⚡", ...)`，**条件渲染**：
    - 仅当 `matching_actions` 非空时显示
    - 若 `is_primary` 仅 1 个 → 直接执行
    - 若 ≥ 2 个 → 弹出 popover 选择
- 颜色用 `accent` 突出（与 Pin/Open 视觉差异）

**Phase E：右键菜单（1.5-2 天）**
- **新建 `src/ui/context_menu.rs`**（约 150 行）：
  ```rust
  pub fn show_entry_context_menu(
      ui: &mut egui::Ui,
      entry: &ClipboardEntry,
      actions: &[Action],
      builtin: BuiltinAction,  // Copy/Pin/Delete/Edit
  ) -> Option<ContextMenuResult>
  ```
- 使用 `egui::Popup::menu()` 实现（egui 0.28 原生支持）
- 菜单结构（以 URL 条目为例）：
  ```
  ⚡ 用 Firefox 打开         ← matched action
  ⚡ 用 Chrome 打开          ← matched action
  ────────────
  📋 复制                    ← 内置
  📌 切换钉选                ← 内置
  🗑 删除                    ← 内置
  ✏️ 编辑内容                ← 内置
  ────────────
  🔗 复制 URL                ← 特殊（仅 URL/文件类）
  ```
- **键位迁移**（关键决策）：
  - 原 `src/app.rs:2802` 的 `response.secondary_clicked()` 触发 `paste_entry(.., true)`（富文本粘贴）
  - 改为：右键 → 弹出 context menu
  - 富文本粘贴迁移到 `Shift + 右键`（`response.secondary_clicked() + modifiers.shift`）
  - 文档 + UI tooltip 双重提示

**Phase F：自动触发（0.5-1 天）**
- 在 `src/clipboard.rs` 捕获入口加：
  ```rust
  if let Some(action) = find_automatic_match(&text) {
      tokio::spawn(action.execute(&text));
      show_toast(&format!("自动执行：{}", action.name), 5_000);  // 5s 撤销窗口
  }
  ```
- 撤销机制：toast 按钮调用 inverse action（删除写入文件、关闭启动进程等）
- 默认关闭，每条 action 独立 toggle

**Phase G：UI 配置面板（1 天）**
- 新增设置面板 7「动作配置」（位置：现有面板 6 之后）
- **模式严格镜像面板 5「标签目录」**（`app.rs:3761-3998`）：
  - 左侧列表（30%）：所有 saved_actions，按 order 排序
  - 右侧详情（70%）：name / pattern / command / is_automatic / is_primary / icon / enabled
  - 实时正则测试：输入测试文本，显示匹配结果
  - 「测试运行」按钮：用当前测试文本执行
- 新增 `Action` 渲染：与 `saved_tag` 列表项视觉一致
- 初始 `i18n` 键：`settings.actions.*`（约 20 个键，与 #8 协同）

**Phase H：i18n 与文档（0.5 天）**
- 所有新增字符串进 i18n 系统（见 #8）
- README 添加「Actions 教程」章节（含 3 个常见模板）

#### 验收标准
- [ ] 添加 URL pattern + `xdg-open %1` → 复制 URL → 工具栏出现 ⚡ 按钮 → 点击 → 浏览器启动
- [ ] 右键条目 → 弹出 context menu 列出「用 Firefox 打开」+ 内置动作 → 选中执行
- [ ] `Shift + 右键` → 触发原富文本粘贴（键位迁移验证）
- [ ] 自动触发的 action 弹出 toast + 提供 5s 撤销窗口
- [ ] pattern 含正则语法错误 → 保存时给出明确错误信息，不崩溃
- [ ] 命令超时（30 秒）被正确中断
- [ ] 导出的 JSON 中 actions 表完整、可在另一台机器导入还原
- [ ] 多个动作匹配时工具栏 ⚡ 点击 → 弹 popover 让用户选择

#### 风险与权衡
| 风险 | 缓解 |
|---|---|
| Regex 注入（DDoS 模式） | 启动时编译 + 复杂度限制 + 最多 100 条 |
| 命令注入（用户输入被解释为 shell） | argv 模式 + 不调用 shell |
| 执行长任务卡 UI | tokio 异步 + 后台线程池 |
| 动作菜单过长 | 按 order 排序 + 折叠到 popover |
| 右键键位迁移遭抗议 | 文档说明 + UI tooltip + 首次启动提示 |
| context menu 与 egui 焦点冲突 | 用 `egui::Popup::menu()` 标准化处理 |

#### 依赖
- **前置**：#8 i18n（Phase 0，所有新增文案必须走 `settings.actions.*`）、#5 Export（让 actions 可迁移）
- **后置**：#6 CLI 可通过命令行手动触发 action、#9 Snippets 可借 actions 执行插入
- **复用**：与 #2 Primary Selection 共享匹配引擎

#### 工作量
**M+（7-9 天）**——比 v0.1 估时 +2 天（context menu 新基础设施 + 键位迁移测试）

---

---

### #2 Primary Selection (X11) 独立跟踪

#### 功能
监听 X11 PRIMARY selection（鼠标中键选区）的变化，与 CLIPBOARD 分离存储。在 UI 中显示条目来源（`剪贴板` / `选区`），可按来源过滤。

#### 价值
- **关键缺失**：X11 用户大量使用鼠标中键粘贴选区文本。tiez 当前完全漏掉这部分数据
- **对标**：Parcellite / Klipper / GPaste / Diodon / Clipman 全部支持——Linux 剪贴板管理器的**基线功能**
- **使用场景**：
  - 在 Firefox 中鼠标选中 URL → 立即在 tiez 中可搜索/钉选
  - 在终端选中命令 → tiez 中可复用
  - 在 PDF 选中段落 → tiez 中归档
- **日常类比**：X11 的两个剪贴板像两个水龙头——CLIPBOARD 是「显式开关」（Ctrl+C/V），PRIMARY 是「感应开关」（鼠标拖蓝/中键）。tiez 当前只听显式那个，错过 50%+ 用水记录

#### 实现规划

**Phase A：XFixes 事件监听（1-2 天）**
- `src/platform/linux.rs` 新增 `PrimarySelectionWatcher` 结构
- 使用 `x11rb` 订阅 `XFixesSelectionNotify` 事件（**事件驱动，零轮询**）
  - **首选 XFixes 原因**：硬件级通知，零 CPU 开销；arboard 轮询在桌面闲置时仍消耗 CPU
  - 回退方案：若 `XFixesQueryExtension` 启动失败，降级为 arboard 轮询
- 启动：在 `start_watcher` 旁启动 `start_primary_watcher`
- 线程模型：每 100ms 检查 ThreadLocal 连接健康度，I/O 失败时自动重连

**Phase B：模型扩展（0.5 天）**
- `src/model.rs` 的 `ClipboardEntry` 加 `source: SelectionSource` 字段
  ```rust
  pub enum SelectionSource { Clipboard, Primary }
  impl SelectionSource {
      pub fn as_str(&self) -> &'static str {
          match self { Self::Clipboard => "clipboard", Self::Primary => "primary" }
      }
  }
  ```
- `captured_text` 工厂方法加 `source` 参数
- DB 迁移 `v12_primary_source`：新增列 `source TEXT NOT NULL DEFAULT 'clipboard'`

**Phase C：捕获与去重（1 天）**
- `src/clipboard.rs` 的轮询/事件循环中区分 source
- **去重要点**：
  - Primary selection 频繁变化（鼠标拖动选区时多次变化）→ 加入 200ms debounce
  - 与 CLIPBOARD dedup 逻辑**独立**（同内容不同 source 应共存）
- 自拷贝检测扩展到 Primary（与 CLIPBOARD 共享 LAST_HASH 表）
- 写回：Primary 条目按 Enter 粘贴时用 `xdotool click 2` 模拟中键

**Phase D：UI（1 天）**
- 条目卡片左侧徽章：`📋 剪贴板` / `🖱️ 选区`（用 emoji 字符，主题色着色）
- 顶部类型过滤器栏新增「来源」过滤 chip：全部 / 仅剪贴板 / 仅选区
- 详情面板元数据表格新增「来源」行
- 设置新增「启用 Primary 跟踪」（默认开）

**Phase E：动作联动（与 #1 协同）**
- Actions 匹配对 PRIMARY 和 CLIPBOARD source 都生效
- 可为 Primary 内容定制专属动作（如「选中后立即调用翻译 API」）

#### 验收标准
- [ ] 在 Firefox 中用鼠标选中一段文字 → tiez 历史新增条目，徽章显示「选区」
- [ ] 在 tiez 中按 Enter 粘贴该条目 → 目标 app 收到 PRIMARY 写入（用 xdotool 模拟中键）
- [ ] 鼠标拖动选区时 tiez 不产生 N 条记录（debounce 生效）
- [ ] 过滤 chip「仅选区」可正确筛选
- [ ] DB schema 升级后旧数据 `source = 'clipboard'`（迁移正确）
- [ ] `XFixesQueryExtension` 失败时静默降级为轮询，UI 提示「Primary 跟踪以兼容模式运行」

#### 风险与权衡
| 风险 | 缓解 |
|---|---|
| Echo 循环（写入→捕获→再写入） | 跟踪 `LAST_PRIMARY_HASH`，自拷贝检测复用现有窗口机制 |
| XFixes 扩展依赖 | 检查 `XFixesQueryExtension` 启动失败时静默降级为轮询 |
| Primary 选区被频繁覆盖（如拖动） | debounce 200ms + 最小长度 4 字符过滤 |
| 与现有 CLIPBOARD 逻辑冲突 | 独立 source 字段，独立 dedup 窗口 |
| Wayland 不支持 | 文档说明仅 X11 完整支持（与现有策略一致） |

#### 依赖
- **前置**：#8 i18n（新增来源徽章、设置项和错误提示必须走 locale）
- **后置**：#1 Actions 可针对 Primary 触发
- **复用**：与 #3 共享「active window 检查」机制

#### 工作量
**M（4-5 天）**

---

---

### #10 fuzzy search

#### 功能
将当前 SQL `LIKE '%query%'` 子串搜索升级为**模糊匹配**（容忍拼写错误、字符顺序）。保持子串搜索作为 fallback。UI 高亮匹配的字符。

#### 价值
- **搜索体验质变**：用户输错仍能匹配（"cllpboard" → "clipboard"）
- **对标标杆**：Maccy 的 fuzzy 搜索是其最被称赞的 UX
- **使用场景**：
  - 大历史（> 1000 条）→ 模糊匹配快速定位
  - 拼写记忆模糊时（特别是中文混合输入）
- **实现成本低**：成熟 crate 可用

#### 实现规划

**Phase A：选型（0.5 天）**
- 候选 crate：
  - `nucleo`（helix 编辑器同款，高性能，GPU 加速可用）
  - `sublime_fuzzy`（Sublime Text 算法）
  - `fuzzy-matcher`（轻量）
- 决定：`nucleo`（最成熟，社区大）

**Phase B：搜索双引擎（1-2 天）**
- `src/search/mod.rs` 新模块
- `SearchEngine` trait：
  ```rust
  pub trait SearchEngine {
      fn search(&self, query: &str, entries: &[ClipboardEntry]) -> Vec<SearchHit>;
  }
  pub struct FuzzyEngine { /* nucleo */ }
  pub struct SubstringEngine { /* 当前 LIKE */ }
  ```
- 默认 fuzzy，子串作为「无结果时回退」或设置切换
- 性能：nucleo 增量匹配，输入时实时返回

**Phase C：UI 高亮（1 天）**
- `src/app.rs` 的搜索结果列表渲染
- `SearchHit` 含 `matched_indices: Vec<usize>`（高亮位置）
- 使用 `RichText` / `Label` 富文本显示匹配字符
- 排序：fuzzy score 高的在前

**Phase D：SQLite FTS5（可选 v1.1）**
- 大历史（> 5000 条）时全量内存 fuzzy 性能下降
- 加 FTS5 虚拟表，先 SQL 预筛 → 再 fuzzy 精排
- 增量同步触发器

**Phase E：CLI 集成（与 #6 协同）**
- `tiez-cli search <query>` 默认用 fuzzy
- `--mode=substring` 切换

#### 验收标准
- [ ] 输错 `cllpboard` → 仍匹配 `clipboard` 相关条目
- [ ] 匹配字符在结果中高亮（黄色背景 / 下划线）
- [ ] 1000 条历史 + 5 字符查询 → 返回延迟 < 50ms
- [ ] 中文搜索：「剪贴板」匹配条目中含「剪贴板」/「剪贴簿」
- [ ] 关闭 fuzzy 设置 → 回退到子串搜索

#### 风险与权衡
| 风险 | 缓解 |
|---|---|
| fuzzy 评分不稳定（结果顺序奇怪） | 调 nucleo 权重 + user 反馈调优 |
| 大数据性能 | FTS5 预筛（v1.1） |
| 中文 fuzzy 效果差 | nucleo 对 Unicode 友好；CJK 按字符匹配即可 |
| 与现有 search_shortcut 冲突 | 无，搜索框不变 |

#### 依赖
- **前置**：#8 i18n（搜索模式、空结果、匹配提示文案必须先入 locale）
- **后置**：#6 CLI 暴露搜索模式

#### 工作量
**S-M（2-4 天）**

---

### #4 KDE Connect 集成

#### 功能
作为 KDE Connect 协议的 **轻量剪贴板子集实现**，与 Android 端 KDE Connect 应用配对后双向同步剪贴板文本。配对通过 mDNS + TLS，UI 显示 QR 码供 Android 扫描。

#### 价值
- **零 Android 开发成本**：复用 50M+ 装机的 KDE Connect Android 应用
- **成熟协议**：10+ 年生产验证，LAN 内安全（mDNS + TLS 自签名证书）
- **X11 完美兼容**：Wayland GSConnect 的 `St.Clipboard` bug 与 X11 无关——**tiez-slim 的天然优势**
- **可演进路径**：先做剪贴板，后续可加 ping-pong、文件分享、通知镜像（不写 Android 端）
- **限制**：剪贴板插件仅同步文本；Android 10+ 后台剪贴板需用户从通知/QS tile 手动点"Send Clipboard"

#### 实现规划

**Phase A：依赖与模块结构（0.5 天）**
- `Cargo.toml` 添加：
  ```toml
  kdeconnect-proto = "0.2"   # MIT, tokio 异步
  mdns-sd = "0.11"            # mDNS 服务发现
  qrcode = "0.14"             # 配对 QR 码
  ```
- 新模块 `src/sync/mod.rs` + `src/sync/kde_connect.rs`

**Phase B：mDNS 发现 + TLS 配对（2-3 天）**
- 实现 KDE Connect 设备发现（multicast `_kdeconnect._tcp.local`）
- 实现配对流程：
  1. tiez 显示 QR（含本机 IP + 临时证书指纹）
  2. Android 扫码，交换自签名证书
  3. 用户在两端确认配对
- 复用 `kdeconnect-proto` 的 `NetworkPacket` / `PairPacket` 类型

**Phase C：剪贴板同步（1-2 天）**
- 实现 `ClipboardPlugin`（kdeconnect-proto 的 Plugin trait）
- 监听 Android 发来的 `ClipboardPacket` → 写入本地 arboard
- 监听本地 arboard 变化 → 发 `ClipboardPacket` 给 Android
- **Echo 抑制**：与现有 self-copy 检测共享 hash 表

**Phase D：UI 与状态（1-2 天）**
- 设置面板「同步」分组
  - KDE Connect 状态：未配对 / 配对中 / 已连接（设备名）
  - 配对按钮 → 弹窗显示 QR 码 + 6 位 PIN
  - 设备列表：已配对设备、最后活跃时间
- 状态栏徽章：已连接时显示「🔗 KDE Connect」chip
- 托盘菜单：「同步状态」子菜单

**Phase E：错误处理与文档（0.5 天）**
- 配对失败：详细错误（证书冲突 / 设备超时 / 协议版本不匹配）
- README 增加「KDE Connect 配对教程」

#### 验收标准
- [ ] Android 装 KDE Connect → 启动 tiez → 配对成功（QR 扫描或 PIN 输入）
- [ ] Android 复制文本 → tiez 历史 2 秒内出现条目（手动从 QS tile 触发发送）
- [ ] tiez 复制文本 → Android 端 KDE Connect 通知显示，点击「写入剪贴板」
- [ ] Echo 抑制：tiez 同步到 Android 后不会再次捕获自己写入的内容
- [ ] 关闭 Android 应用 / 网络断开 → tiez UI 显示「设备离线」，恢复后自动重连

#### 风险与权衡
| 风险 | 缓解 |
|---|---|
| `kdeconnect-proto` v0.2.1 新发布（2026-03），生态未稳 | 锁定版本，遇到 bug 自行 patch + 上游 PR |
| Android 10+ 后台剪贴板限制 | 文档说明，提供「长按通知 → 立即发送」引导 |
| TLS 证书首次配对 UX 复杂 | 提供「PIN 配对 + 信任时长 24h」选项（类似 KDE 桌面） |
| 多设备场景 | 设备列表 + 单向/双向粒度控制（v1.1） |
| 协议版本演进 | 启动时声明支持的 `protocolVersion` 范围 |

#### 依赖
- **前置**：#8 i18n（同步配对/错误文案命名空间 `sync.kde_connect.*`）、#5（配置导出）让 KDE Connect 设置可迁移
- **后置**：#6 CLI 可暴露同步状态、#7 DB 加密需确保同步的剪贴板内容也加密

#### 工作量
**M-L（5-7 天）**

---

---

### #7 数据库加密

#### 功能
对 `is_sensitive = 1` 的条目，`content` 与 `preview` 列在 DB 中以加密形式存储。Linux 使用 `keyring` crate（Secret Service / GNOME Keyring / KWallet）存储主密钥，应用启动时拉取。提供 `secure_storage` Cargo feature 控制是否启用（opt-in）。

#### 价值
- **fork 优势补齐**：fork 已有 AES-256-GCM + keyring 实现，tiez 没移植
- **隐私深度**：DB 文件本身被窃取（如备份泄漏）也无法直接读取敏感条目
- **合规友好**：GDPR / 企业 IT 审计场景
- **对标**：fork 是参考实现；CopyQ、Maccy 也都支持加密

#### 实现规划

**Phase A：加密层抽象（1-2 天）**
- 新文件 `src/encryption.rs`
  ```rust
  pub trait SecureStore {
      fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>>;
      fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>>;
  }
  pub struct KeyringBackend { /* AES-256-GCM */ }
  ```
- AES-256-GCM（`aes-gcm` crate），每次加密随机 nonce
- 主密钥派生：启动时若无 keyring 条目 → 生成 32 字节随机密钥 → 写入 keyring（service: `tiez-slim-linux`，account: `master`）

**Phase B：存储层集成（1-2 天）**
- `src/storage.rs` 改造 `insert_entry` / `get_entry` / `search`：
  - 写入时：`is_sensitive` → 加密 `content`/`preview`
  - 读取时：检测 `enc:linux:` 前缀 → 解密
  - 搜索：先 LIKE 查 content_hash，然后批量解密
- DB schema 加列 `encryption_version: i32` 标记加密算法版本
- 性能：批量解密缓存（LRU 64 条），避免热点条目反复解密

**Phase C：后台加密队列（1 天）**
- 新线程：监听 `is_sensitive` 变更事件
- 加 `sensitive` 标签 → 加密已存内容
- 移除 `sensitive` 标签 → 解密
- 类似 fork 的 `encryption_queue.rs`

**Phase D：启动对齐（0.5 天）**
- 启动时扫描所有 `is_sensitive = 1` 条目
- 加密状态不一致时（半加密）→ 自动重加密/解密
- 进度条显示在 dev 面板

**Phase E：Feature gate 与文档（0.5 天）**
- `Cargo.toml` 加 `[features] secure_storage = ["aes-gcm", "keyring"]`
- 默认 `secure_storage = []` 不启用（避免 keyring 守护进程缺失时启动失败）
- README 增加「敏感数据加密」章节

#### 验收标准
- [ ] `cargo build --features secure_storage` 通过
- [ ] 启用加密后给条目打 `sensitive` 标签 → DB 文件中 `content` 列是密文（`enc:linux:...` 前缀）
- [ ] 删除 keyring 中 `tiez-slim-linux` 条目 → 启动后所有 sensitive 条目**不可读**（UI 显示「加密密钥缺失」）
- [ ] 启动时检测半加密状态 → 自动归一化
- [ ] 加密/解密延迟 < 5ms（单条）/ 100ms（100 条批量）

#### 风险与权衡
| 风险 | 缓解 |
|---|---|
| keyring 守护进程未运行 | feature 默认关闭 + 启动检测 + 明确错误信息 |
| 性能开销 | 批量 LRU 缓存 + 异步后台队列 |
| 主密钥丢失 = 数据永久丢失 | README 强提示用户备份 keyring；提供「导出主密钥」冷存储选项 |
| 跨平台 | 优先 Linux（keyring）；Windows DPAPI / macOS Keychain 留 v1.1 |
| 与 #4 KDE Connect 协同 | 同步前自动解密再同步（v1.1 可加端到端加密同步） |

#### 依赖
- **前置**：#8 i18n（密钥缺失、解密失败、加密状态提示必须先入 locale）
- **后置**：#5 Export 时需考虑加密状态、#4 同步时考虑加密

#### 工作量
**M（5-6 天）**

---

---

### #6 CLI 配套（`tiez-cli`）

#### 功能
在同一 Cargo workspace 新增二进制 `tiez-cli`，通过 Unix domain socket 与运行中的 tiez-slim 主进程通信。提供读/写剪贴板、搜索、粘贴、查询状态等子命令。

#### 价值
- **解锁 tiling WM 生态**：sway/hyprland/i3 用户可绑定 `Super+V` → `tiez-cli list | rofi -dmenu | tiez-cli paste`
- **shell 脚本化**：cron、systemd timer、git hooks
- **对标**：CopyQ / GPaste / Parcellite 都标配
- **零重复逻辑**：所有数据通过 IPC 获取，与 GUI 共享同一份 storage
- **AI agents友好**：能直接完整调用应用的所有信息

#### 实现规划

**Phase A：IPC 服务端（1-2 天）**
- 新文件 `src/ipc.rs`
- Unix domain socket 路径：`$XDG_RUNTIME_DIR/tiez-slim-linux.sock`（推荐，回退 `~/.local/share/tiez-slim-linux/ipc.sock`）
- 协议：JSON Lines（每行一条消息），简单请求/响应
  ```json
  {"cmd": "list", "args": {"limit": 10}}
  {"cmd": "search", "args": {"query": "url"}}
  {"cmd": "paste", "args": {"id": 42}}
  {"cmd": "status"}
  {"cmd": "pin", "args": {"id": 42, "pinned": true}}
  ```
- 启动位置：`main.rs` 在 eframe run 之前启动 IPC 监听线程
- 鉴权：socket 文件权限 `0600`（仅当前用户）；预留 token 机制（v1.1）

**Phase B：CLI 二进制（1-2 天）**
- `Cargo.toml` 新增 `[[bin]] name = "tiez-cli"` target
- 新文件 `src/bin/tiez_cli.rs`
- 用 `clap` 派生参数解析
- 人类可读 / `--json` 机器可读双输出模式
- 退出码：0 成功 / 1 通用错误 / 2 网络错误 / 3 数据不存在

**Phase C：命令实现（1 天）**
| 子命令 | 行为 |
|---|---|
| `list [--limit N] [--type T] [--tag T]` | 打印最近 N 条 |
| `search <query>` | 模糊+子串搜索 |
| `paste <id>` | 将条目写入剪贴板 + 模拟粘贴（如果 GUI 在运行则通知） |
| `pin <id> [--unpin]` | 切换钉选 |
| `tag <id> <tag>...` | 添加标签 |
| `delete <id>` | 删除条目 |
| `status` | 打印 daemon PID + 状态 + 同步状态 |
| `add <content> [--type T]` | 手动新建条目 |

**Phase D：文档与示例（0.5 天）**
- README 添加 CLI 章节
- `docs/sway-integration.md`：sway/hyprland 配置示例
- `docs/rofi-script.sh`：rofi 集成示例

#### 验收标准
- [ ] `tiez-cli list` 打印最近 10 条
- [ ] `tiez-cli search "github"` 列出所有匹配
- [ ] `tiez-cli paste 42` 将条目 42 写入剪贴板（GUI 不在时也能工作）
- [ ] GUI 未运行 → `tiez-cli status` 给出明确错误（不是 panic）
- [ ] sway 绑定 `bindsym $mod+v exec tiez-cli list | rofi -dmenu | tiez-cli paste` 可用

#### 风险与权衡
| 风险 | 缓解 |
|---|---|
| Socket 路径权限（多用户系统） | `0600` + 仅本用户；预留抽象 v1.1 支持 SSH 远程 |
| 主进程未启动时 CLI 行为 | 明确错误 + `--spawn` 选项自动启动主进程 |
| Socket 残留（旧进程崩溃） | 启动时检查并 `unlink` 旧 socket |
| 大量并发 CLI 调用 | tokio 异步 + 简单 mutex 保护 storage |

#### 依赖
- **前置**：#8 i18n（预留 `cli.*` 命名空间）、#5（备份/导出让 CLI 配置可独立）
- **后置**：#4 KDE Connect 状态可通过 CLI 查询、#9 Snippets CLI 命令

#### 工作量
**M（4-5 天）**

---

---

### #9 Snippet 模板（模式镜像 saved_tags）

#### 功能
在 SQLite 中新增独立表 `snippets`（**不复用 saved_tags 代码**），存储用户定义的「可复用文本片段」。支持变量插值 `{{date}}` / `{{time}}` / `{{clipboard}}` / `{{uuid}}`，可绑定全局热键或通过搜索面板触发。Snippet 可分类、可排序、可统计使用次数。

#### 价值
- **文本扩展器集成**：把 tiez 从「剪贴板历史」升级为「轻量文本扩展器」
- **使用场景**：
  - 邮件签名（`Ctrl+Alt+S` → 签名插入）
  - 常用代码块（`forr` → `for (const item of array) { ... }`）
  - 标准回复模板（客户支持场景）
  - UUID/时间戳快速插入
- **对标**：Ditto (Groups) / GPaste (passwords) / CopyQ (Snippets community cmd)

#### 为什么是「模式镜像」而不是「代码复用」

**两者的本质区别**：

| 维度 | 标签 (Tag) | 模板 (Snippet) |
|---|---|---|
| **本体** | 内容的**元数据**（"这是工作内容"） | 内容**本身**（"插入这段代码"） |
| **应用方向** | 给已存在的条目**贴标签** | 创建**新内容**注入剪贴板 |
| **数量关系** | 一个 entry × N tags | 一个 snippet = 独立可重用条目 |
| **触发方式** | 自动（敏感启发式）或手动打标 | 手动搜索 + 触发 |
| **生命周期** | 与 entry 同寿 | 永久（独立存储） |

→ **代码层面是两套独立实现**（不同数据模型 + 不同交互），但**模式层面镜像**（CRUD UI、设置面板布局、i18n 命名空间）。

#### 实现规划

**Phase A：数据模型（0.5-1 天）**
- DB schema 新表 `snippets`（独立表，不与 saved_tags 共享）：
  ```sql
  CREATE TABLE IF NOT EXISTS snippets (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      name TEXT NOT NULL UNIQUE,
      content TEXT NOT NULL,
      category TEXT,
      color TEXT NOT NULL DEFAULT '#4f46e5',
      hotkey TEXT,                -- 可选："Ctrl+Alt+S"
      variables TEXT,             -- JSON: ["date", "time", "uuid"]
      usage_count INTEGER NOT NULL DEFAULT 0,
      last_used_at INTEGER,
      sort_order INTEGER NOT NULL DEFAULT 0,
      created_at INTEGER NOT NULL,
      updated_at INTEGER NOT NULL
  );
  ```
- `src/model.rs` 新增 `Snippet` 结构体（与 `Tag` 结构体平级，**不继承**）
- `src/storage.rs` 新增 CRUD API：`add_snippet` / `list_snippets` / `update_snippet` / `delete_snippet` / `increment_snippet_usage`

**Phase B：变量插值引擎（0.5 天）**
- `src/snippets/expand.rs`：`fn expand(content: &str, context: &SnippetContext) -> Result<String>`
- 支持的变量：
  - `{{date}}` → `2026-06-06`
  - `{{time}}` → `14:30:22`
  - `{{datetime}}` → ISO 8601
  - `{{clipboard}}` → 当前剪贴板内容
  - `{{uuid}}` → 新 UUID v4
  - `{{cursor}}` → 标记光标位置（特殊处理，需 #1 Actions 写回引擎支持）
  - `{{shell:cmd}}` → 执行 shell 命令并插入输出（**默认禁用**，需在设置中显式开启）

**Phase C：UI — 模式镜像 saved_tags 严格对齐（1-2 天）**
- 设置面板 7「模板库」（位置：现有面板 6 之后），**布局严格镜像面板 5「标签目录」**（`app.rs:3761-3998`）：
  - 左侧列表（30%）：所有 snippets，按 sort_order 排序
  - 右侧详情（70%）：name / category / color picker / 变量勾选 / hotkey 录制 / 内容编辑器 / 使用统计
  - 「快速插入到剪贴板」按钮（与标签面板的「加入当前条目标签」对应）
  - 「从模板库移除」按钮（与标签面板的「从目录移除」对应）
  - 列表项渲染：与 `saved_tag_list_item()` 视觉一致
- 新增 `SnippetListItem` 渲染函数
- 拖放排序（与 saved_tags 现有机制保持一致）
- i18n 键命名：`settings.snippets.*`（与 `settings.tags.*` 平行）

**Phase D：搜索面板 + 热键（1 天）**
- 搜索面板新增「Snippets」标签页（与历史并列）
- 全局热键 `snippet_picker_hotkey`：弹出搜索框 → 选 snippet → 写入剪贴板 + 模拟粘贴
- 使用统计：显示 `usage_count` 排序

**Phase E：CLI 集成（与 #6 协同）**
- `tiez-cli snippet list / show <name> / use <name>` 子命令
- 便于 shell 脚本调用

**Phase F：导入/导出（与 #5 协同）**
- Snippet 列表包含在 ExportBundle 中
- 单独的「Snippets 库」导入/导出（与人分享模板）

#### 验收标准
- [ ] 创建 snippet `email-sig` 内容含 `{{date}}` → 触发 → 输出替换为今日日期
- [ ] 绑定 hotkey `Ctrl+Alt+S` → 在任何应用按热键 → 签名粘贴到当前焦点
- [ ] UUID 变量每次调用生成新 UUID
- [ ] {{clipboard}} 变量读取触发时的剪贴板内容
- [ ] `usage_count` 正确递增并按降序排序
- [ ] 设置面板 7 的 UI 布局与面板 5（标签目录）严格一致（视觉对照）
- [ ] Snippet 删除不影响其他 snippet / 不影响 entry_tags（独立表验证）

#### 风险与权衡
| 风险 | 缓解 |
|---|---|
| 变量插值注入 | `{{shell:cmd}}` 默认禁用，需在设置中显式开启 |
| 热键冲突 | 与 #3 私有模式热键 / 主热键统一管理，复用冲突检测 |
| 大量 snippet 性能 | 启动时索引化，< 1000 个无压力 |
| 与历史混淆 | 标签页分离，「Snippets」独立 tab 视觉区分 |
| 与 saved_tags 模式分叉 | 严格的 i18n 命名空间分离 + 独立 widget 组件 |

#### 依赖
- **前置**：#8 i18n（`settings.snippets.*` 命名空间）、#6 CLI（可选，提升体验）
- **后置**：可与 #4 KDE Connect 协同（手机→PC snippet 同步）
- **复用**：UI 组件模式镜像面板 5（`app.rs:3761-3998`），但**不复用代码**——保持两个面板的独立性，便于未来分叉演化

#### 工作量
**M（4-5 天）**

---
---

## 3. 整体工作量与时间表

### 总工作量汇总

| 阶段 | 状态 | 项 | 工作量（人天） | 累计 |
|---|---|---|---|---|
| **Phase 0** | ✅ 已完成 | #8 i18n（zh + en） | 9 | 9 |
| **Phase 1** | ✅ 已完成 | #3 黑名单 + 私有模式 | 3 | 12 |
| | | #5 Export/Import + 备份 | 3 | 15 |
| | | #1 Actions (Option D) | 8 | 23 |
| | | #2 Primary Selection (XFixes) | 5 | 28 |
| | | #10 fuzzy search | 3 | 31 |
| **Phase 2** | ✅ 已完成 | #4 KDE Connect | 6 | 37 |
| | | #7 DB 加密 | 6 | 43 |
| | | #6 CLI 配套 | 5 | 48 |
| **Phase 3** | ✅ 已完成 | #9 Snippets（模式镜像 saved_tags） | 5 | 53 |

**总计**：约 **53 人天** ≈ **10.5 周全职**（计划估算保留作历史参考）  
**已完成**：53 人天（Phase 0-3，10/10 项源码实现）  
**剩余**：0 人天（后续为稳定性、性能、真实设备 QA 与打包发布）

### 建议时间表（单人串行）

```
2026-06 ──── 2026-07 ──── 2026-08 ──── 2026-09 ──── 2026-10 ──── 2026-11
   │            │            │            │            │            │
   │✅Phase 0 ─┤            │            │            │            │
   │ 1.5 周     │            │            │            │            │
   │            ├─ Phase 1 ─┤            │            │            │
   │            │   5 周     │            │            │            │
   │            │            ├─ Phase 2 ─┤            │            │
   │            │            │   4 周     │            │            │
   │            │            │            ├─ Phase 3 ─┤            │
   │            │            │            │  4 周      │            │
```

**实际进度（截至 2026-06-09）**：Phase 0-3 均已进入源码实现；v0.3.2 完成性能、KDE 默认启用、设置折叠迁移与标签布局维护。  
**目标**：后续转入真实设备 KDE Connect QA、打包发布与长期稳定性验证。

### 关键里程碑

| 里程碑 | 状态 | 日期 | 完成项 | 验证方式 |
|---|---|---|---|---|
| **M0**: i18n 上线 | ✅ 已完成 | 2026-06-07（提前 38 天） | #8 | 切换语言 → 全 UI 英文/中文；缺失 key 检查可运行；752 keys 100% 覆盖 |
| **M1**: 信任底座上线 | ✅ 已完成 | 2026-06-08 | #3, #5 | KeePassXC 黑名单不记录；退出自动备份；导出/导入可还原 |
| **M2**: 核心工作流完成 | ✅ 已完成 | 2026-06-08 | #1, #2, #10 | URL→动作执行；PRIMARY 捕获；模糊搜索高亮与延迟达标 |
| **M3**: 扩展能力完成 | ✅ 已完成 | 2026-06-08 | #4, #7, #6 | KDE Connect UI + CLI status；敏感条目加密；CLI 完整子命令 |
| **M4**: 差异化完成 | ✅ 已完成 | 2026-06-09 | #9 | Snippet 模板、变量插值、picker 热键、IPC 子命令 |
| **Release v0.4.0** | 🧪 QA/打包阶段 | TBD | 全部 | 真实设备同步 QA、打包发布、release notes |

---

## 4. 明确不在本路线图

| 项 | 价值 | 跳过原因 |
|---|---|---|
| #11 OCR | 中 | Tesseract 依赖重；可作为可选 feature v1.1 |
| #12 QR/Barcode 生成 | 低 | 小众需求，依赖重 |
| #13 剪贴板统计 | 低 | 锦上添花，工作量大于价值 |
| #14 自动定时清理 | 低 | 现有 30 天保留 + 1000 条限制已够用 |
| #15 多选粘贴 | 低 | 交互复杂，价值有限 |
| #16 拖放 | 低 | egui 支持有限，工作量大 |
| #17 日期分组 | 低 | 仅 UI 改 |
| #18 自定义每应用粘贴热键 | 低 | 复杂度高 |
| #19 6 套主题移植 | 中 | CSS→Rust token 重写工作量大；v1.1 单独评估 |
| #20 AI 助手 | 中-低 | fork 上游验证不足 + 网络依赖；按需评估 |
| #21 WebDAV 云同步 | 中-高 | MQTT 方案（#4 演进）已能覆盖大部分场景；如需可作 v1.1 |
| macOS/iOS 支持 | — | 用户明确无设备 |
| Wayland 支持 | — | 用户明确无测试设备 |

---

## 5. 待用户确认的开放问题

| # | 问题 | 默认建议 | 状态 |
|---|---|---|---|
| ~~Q1~~ | ~~i18n 优先级~~ | ~~是~~ | ✅ **v0.2 决议**：上提 Phase 0 |
| Q2 | **KDE Connect 范围**：v1 仅做剪贴板子集，还是包含 ping/通知/文件分享？ | 仅剪贴板子集（最小可用） | 待确认 |
| Q3 | **DB 加密 opt-in 还是 default**？fork 是 default，破坏首次启动体验 | opt-in（feature gate），给用户选择权 | 待确认 |
| Q4 | **CLI 鉴权**：v1 仅依赖 socket 文件权限（0600），是否需要 token？ | v1 不加，v1.1 再加 | 待确认 |
| Q5 | **Snippet hotkey 优先级**：是否提供「类别+首字母」快速选择？ | v1 仅全局热键弹搜索框 | 待确认 |
| Q6 | **Primary Selection 触发动作**：是否允许为 Primary 内容设置自动动作？ | 是（与 CLIPBOARD 共享 action 库） | 待确认 |
| Q7 | **私有模式自动恢复**：30 分钟无操作是否自动关闭？ | 否，仅视觉高亮 + 手动 | 待确认 |
| Q8 | **fuzzy 与子串并行**：是否同时显示两套结果？ | 否，仅 fuzzy + 设置切换 | 待确认 |
| Q9 | **Export 文件格式**：纯 JSON / SQLite 镜像 / 二者皆可？ | 二者皆可（v1 优先 JSON，SQLite 镜像 v1.1） | 待确认 |
| Q10 | **roadmap 周期**：按季度还是按月发布？ | 按月 minor，按季度 major | 待确认 |
| **Q11** | **右键键位迁移**（v0.2 新增）：右键=菜单，Shift+右键=原富文本粘贴 | 接受（首次启动提示） | ✅ **v0.2 决议**：已确认 |
| **Q12** | **Actions UI 模式**（v0.2 新增）：Option D（工具栏 ⚡ + 右键菜单 + 可选 auto） | 接受 | ✅ **v0.2 决议**：已确认 |
| **Q13** | **Snippets 架构**（v0.2 新增）：模式镜像 saved_tags（独立表 + 独立 UI 模式） | 接受 | ✅ **v0.2 决议**：已确认 |
| **Q14** | **Primary Selection 实现**（v0.2 新增）：XFixes 事件订阅（首选） + 轮询降级 | 接受 | ✅ **v0.2 决议**：已确认 |

---

## 6. 决策日志

| 日期 | 决策 | 原因 |
|---|---|---|
| 2026-06-06 | 路线图 v0.1 创建 | 完成功能差距分析与同步方案调研 |
| 2026-06-06 | Actions 列为 P0 | 投入产出比最高；区分度最大 |
| 2026-06-06 | KDE Connect > MQTT > WebSocket 优先级 | 零 Android 开发成本 > 跨网络 > 全控制 |
| 2026-06-06 | i18n 提前到 Phase 2 | 避免后续功能反复翻译（v0.1 决策） |
| **2026-06-06** | **路线图 v0.2 升级：i18n 进一步上提至 Phase 0** | 用户反馈：i18n 应在所有用户可见功能前完成基础设施 |
| **2026-06-06** | **Actions 改用 Option D（工具栏 ⚡ + 右键菜单 + auto-trigger）** | 调研 6 款竞品无单一模式胜出；用户提出工具栏集成是正确方向 |
| **2026-06-06** | **Snippets 架构：模式镜像 saved_tags（独立表 + 独立 UI）** | 用户反馈：可复用 UI 模式但不应复用代码（语义不同） |
| **2026-06-06** | **Primary Selection 实现：XFixes 事件订阅** | 零 CPU 开销，事件驱动；现有 x11rb 集成简单 |
| **2026-06-06** | **右键键位迁移：右键=菜单，Shift+右键=原富文本粘贴** | 工具栏 ⚡ 按钮 + context menu 需要腾出右键键位 |
| **2026-06-06** | **路线图 v0.3 重排：用户确认先实现 i18n** | 先建立语言基础设施，后续隐私/备份/Actions/Primary/同步/CLI/Snippets 全部按 locale key 接入 |
| **2026-06-07** | **路线图 v0.3.1：Phase 0 #8 i18n 交付完成** | 752 keys（zh-CN 源 + en-US 翻译 100% 覆盖），rust-i18n v4 + `%{var}` 占位符语法，启动时 `set_app_locale()` 修复 follow-system 持久化语义，自定义 `searchable_combo_row` 统一下拉；M0 提前 38 天达成 |
| **2026-06-07** | **i18n 关键设计决策**：默认语言从 zh-CN 改为 follow-system | 用户微调要求 + 启动时 `detect_system_locale()` 正确处理，避免锁定特定 locale |
| **2026-06-07** | **i18n 关键设计决策**：rust-i18n v4 占位符用 `%{var}`（不是 `{var}`） | 由 4.1.0 文档明确；v0.3 实施时误用 `{var}` 致 101 个 / 文件不替换，用户肉眼发现后修复 |
| **2026-06-08** | **v0.3.0：M1-M3 全部完成** | #1 Actions, #2 Primary, #3 黑名单+私有, #4 KDE Connect, #5 备份, #6 CLI, #7 加密, #8 i18n, #10 模糊搜索 |
| **2026-06-09** | **v0.3.2：10/10 功能状态校准 + 性能/布局维护** | #9 Snippets 已实现；KDE Connect 默认编译启用；Primary Selection 单 watcher + echo guard；搜索 debounce；Emoji 收藏页取消每帧扫盘；设置折叠位补齐迁移；卡片标签限量 +N；顶部内容类型过滤固定三行 |
| TBD | 待用户确认 Q2-Q10 | — |

---

## 7. 关联与变更管理

- **本文档变更**：每个 Phase 完成后追加「实际 vs 计划」小节，更新工作量估算
- **Git 关联**：每个 Phase 单独分支（`feature/actions`, `feature/primary-selection` 等），按 feature 合并
- **发布节奏**：
  - 每个 Phase 结束发 minor release（v0.3.x → v0.4.x → v0.5.x → v0.6.x）
  - 大版本（v1.0）保留到全部 10 项完成且稳定运行 1 个月后
- **测试覆盖**：每个功能必须包含 `cargo test` 单元测试 + 手动 QA（参考 `AGENTS.md` 中的 QA 强制要求）

---

> **文档维护者**：Sisyphus (claude)  
> **下次评审**：Phase 0 完成后（约 2026-07-15，验证 i18n 基础、缺失 key 检查与 Phase 1 文案接入规则）
