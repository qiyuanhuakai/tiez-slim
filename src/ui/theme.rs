//! macOS 设计语言主题 Token 定义。
//!
//! `MacosTokens` 将 tiez-clipboard 的 macos.css / theme.tokens.css / global.tokens.css
//! 中的关键 CSS 变量映射为编译期 Rust 常量，支持 Light / Dark 双模式。
//! 所有颜色使用 `egui::Color32`，尺寸和间距使用 `f32`。

use egui::Color32;

fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, (a * 255.0) as u8)
}

/// macOS 设计语言主题 Token。
///
/// 包含颜色、圆角、间距、字体、阴影、模糊和动画等完整视觉参数。
/// 通过 `MacosTokens::light()` 和 `MacosTokens::dark()` 获取对应模式的实例。
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MacosTokens {
    // ===== 颜色 (Color32) ===================================================
    /// 窗口主背景色
    pub bg: Color32,
    /// 主前景/文字色
    pub fg: Color32,
    /// 窗口外 body 背景色
    pub body_bg: Color32,
    /// 主题强调色 (macOS blue #0a84ff)
    pub accent: Color32,
    /// 强调色悬停态
    pub accent_hover: Color32,
    /// 柔和强调色 (低饱和度)
    pub accent_soft: Color32,
    /// 主边框色
    pub border: Color32,
    /// 浅色边框
    pub border_light: Color32,
    /// 主阴影色
    pub shadow: Color32,
    /// 次要/弱化文字色
    pub muted: Color32,

    // -- Surface (卡片/历史/设置/数据) --
    /// 卡片背景
    pub card: Color32,
    /// 卡片悬停背景
    pub card_hover: Color32,
    /// 卡片文字色
    pub card_fg: Color32,
    /// 历史列表项背景
    pub history_bg: Color32,
    /// 历史列表项文字
    pub history_fg: Color32,
    /// 历史列表项悬停
    pub history_hover: Color32,
    /// 历史列表项选中背景
    pub history_selected: Color32,
    /// 历史列表项选中边框
    pub history_selected_border: Color32,
    /// 设置分组背景
    pub settings_bg: Color32,
    /// 设置分组边框
    pub settings_border: Color32,
    /// 设置分组头部背景
    pub settings_header_bg: Color32,
    /// 设置分组头部边框
    pub settings_header_border: Color32,
    /// 数据面板背景
    pub data_bg: Color32,
    /// 数据面板边框
    pub data_border: Color32,

    // -- Glass / Header --
    /// 毛玻璃背景 (header 等)
    pub glass_bg: Color32,
    /// 毛玻璃边框
    pub glass_border: Color32,
    /// 毛玻璃阴影
    pub glass_shadow: Color32,
    /// 工具栏背景
    pub toolbar_bg: Color32,
    /// 根窗口背景 (近似 linear-gradient 的中间色)
    pub root_bg: Color32,
    /// 顶栏头部背景
    pub header_bg: Color32,
    /// 顶栏头部分隔线
    pub header_border: Color32,

    // -- Input / Form --
    /// 输入框背景
    pub input_bg: Color32,
    /// 输入框文字色
    pub input_fg: Color32,
    /// 输入框边框
    pub input_border: Color32,
    /// 输入框聚焦边框
    pub input_focus_border: Color32,

    // -- Toggle / Switch --
    /// 开关关闭态轨道色
    pub toggle_track_off: Color32,
    /// 开关开启态轨道色 (= accent)
    pub toggle_track_on: Color32,
    /// 开关滑块色
    pub toggle_thumb: Color32,

    // -- Tag --
    /// 标签背景 (搜索区域)
    pub tag_bg: Color32,
    /// 标签文字
    pub tag_fg: Color32,
    /// 标签边框
    pub tag_border: Color32,
    /// 标签激活态背景
    pub tag_active_bg: Color32,
    /// 标签激活态边框
    pub tag_active_border: Color32,

    // -- Keycap --
    /// 按键帽背景
    pub keycap_bg: Color32,
    /// 按键帽文字
    pub keycap_fg: Color32,
    /// 按键帽边框
    pub keycap_border: Color32,
    /// 按键帽阴影
    pub keycap_shadow: Color32,
    /// 按键帽激活态背景
    pub keycap_active_bg: Color32,
    /// 按键帽激活态边框
    pub keycap_active_border: Color32,

    // -- Range / Slider --
    /// 滑轨背景
    pub range_track: Color32,
    /// 滑轨填充 (= accent)
    pub range_fill: Color32,
    /// 滑块
    pub range_thumb: Color32,

    // -- Modal / Dialog --
    /// 弹窗背景
    pub modal_bg: Color32,
    /// 弹窗遮罩
    pub modal_backdrop: Color32,
    /// 弹窗边框
    pub modal_border: Color32,

    // -- Button --
    /// 按钮悬停背景
    pub btn_hover_bg: Color32,
    /// 按钮激活背景
    pub btn_active_bg: Color32,
    /// 按钮激活文字
    pub btn_active_text: Color32,
    /// 对话框按钮背景
    pub dialog_btn_bg: Color32,
    /// 对话框按钮边框
    pub dialog_btn_border: Color32,
    /// 对话框按钮文字
    pub dialog_btn_text: Color32,
    /// 对话框主按钮背景
    pub dialog_btn_primary_bg: Color32,
    /// 对话框主按钮文字
    pub dialog_btn_primary_text: Color32,
    /// 对话框主按钮边框
    pub dialog_btn_primary_border: Color32,
    /// 对话框按钮悬停背景
    pub dialog_btn_hover_bg: Color32,
    /// 对话框按钮悬停边框
    pub dialog_btn_hover_border: Color32,

    // -- Status --
    /// 成功色 (绿)
    pub success: Color32,
    /// 警告色 (橙)
    pub warning: Color32,
    /// 危险/错误色 (红)
    pub danger: Color32,
    /// 信息色 (蓝)
    pub info: Color32,

    // -- Semantic --
    /// 敏感内容指示色
    pub sensitive: Color32,
    /// 敏感内容背景
    pub sensitive_bg: Color32,
    /// 置顶指示色
    pub pinned: Color32,
    /// 置顶背景
    pub pinned_bg: Color32,

    // -- Scrollbar --
    /// 滚动条滑块
    pub scrollbar_thumb: Color32,
    /// 滚动条滑块悬停
    pub scrollbar_hover: Color32,

    // -- Select / Dropdown --
    /// 下拉菜单背景
    pub select_menu_bg: Color32,
    /// 下拉菜单边框
    pub select_menu_border: Color32,

    // ===== 尺寸 / 圆角 (f32) ================================================
    /// 窗口圆角
    pub radius_window: f32,
    /// 卡片圆角
    pub radius_card: f32,
    /// 输入框圆角
    pub radius_input: f32,
    /// 标签胶囊圆角
    pub radius_tag: f32,
    /// 按键帽圆角
    pub radius_keycap: f32,
    /// 开关圆角
    pub radius_toggle: f32,
    /// 对话框圆角
    pub radius_dialog: f32,

    // ===== 间距 (f32) ========================================================
    /// 最小间距 (4.0)
    pub space_xs: f32,
    /// 小间距 (8.0)
    pub space_sm: f32,
    /// 中间距 (12.0)
    pub space_md: f32,
    /// 大间距 (16.0)
    pub space_lg: f32,
    /// 特大间距 (24.0)
    pub space_xl: f32,
    /// 超大间距 (32.0)
    pub space_2xl: f32,

    // ===== 字体 ===============================================================
    /// 无衬线字体族
    pub font_sans: &'static str,
    /// 展示/标题字体族
    pub font_display: &'static str,
    /// 等宽字体族
    pub font_mono: &'static str,
    /// 极小字号 (11.0)
    pub font_size_xs: f32,
    /// 小字号 (12.0)
    pub font_size_sm: f32,
    /// 中等字号 (13.0)
    pub font_size_md: f32,
    /// 大字号 (15.0)
    pub font_size_lg: f32,
    /// 特大字号 (18.0)
    pub font_size_xl: f32,
    /// Regular 字重 (400)
    pub font_weight_regular: f32,
    /// Medium 字重 (500)
    pub font_weight_medium: f32,
    /// Semibold 字重 (600)
    pub font_weight_semibold: f32,
    /// Bold 字重 (700)
    pub font_weight_bold: f32,

    // ===== 阴影 (Color32 — 阴影主色) ========================================
    /// 卡片阴影色
    pub shadow_card: Color32,
    /// 弹窗阴影色
    pub shadow_modal: Color32,
    /// 顶栏阴影色
    pub shadow_header: Color32,
    /// 按键帽阴影色
    pub shadow_keycap: Color32,

    // ===== 模糊 / 饱和度 (f32) ==============================================
    /// 毛玻璃模糊半径 (18.0)
    pub blur_glass: f32,
    /// 弹窗模糊半径 (10.0)
    pub blur_modal: f32,
    /// 顶栏模糊半径 (16.0)
    pub blur_header: f32,
    /// 毛玻璃饱和度 (1.28)
    pub saturate_glass: f32,

    // ===== 动画 (f32) ========================================================
    /// 快速动画时长 (0.12s)
    pub anim_duration_fast: f32,
    /// 基础动画时长 (0.20s)
    pub anim_duration_base: f32,
    /// 慢速动画时长 (0.32s)
    pub anim_duration_slow: f32,
}

impl MacosTokens {
    /// Light 模式 Token (白色系背景, 深色文字)。
    ///
    /// 颜色值源自 tiez-clipboard `macos.css` 的 `:root.theme-macos` 变量
    /// 以及 `theme.tokens.css` 的 macOS light 区段。
    pub fn light() -> Self {
        Self {
            // -- Core --
            bg: Color32::from_rgb(245, 246, 248),      // #f5f6f8
            fg: Color32::from_rgb(29, 29, 31),         // #1d1d1f
            body_bg: Color32::from_rgb(236, 239, 243), // #eceff3
            accent: Color32::from_rgb(10, 132, 255),   // #0a84ff
            accent_hover: Color32::from_rgb(51, 149, 255), // #3395ff
            accent_soft: rgba(10, 132, 255, 0.15),
            border: rgba(60, 60, 67, 0.24),
            border_light: rgba(255, 255, 255, 0.64),
            shadow: rgba(15, 18, 26, 0.16),
            muted: Color32::from_rgb(99, 99, 102), // #636366

            // -- Surface --
            card: rgba(255, 255, 255, 0.72),
            card_hover: rgba(255, 255, 255, 0.90),
            card_fg: Color32::from_rgb(29, 29, 31), // #1d1d1f
            history_bg: rgba(255, 255, 255, 0.76),
            history_fg: Color32::from_rgb(29, 29, 31), // #1d1d1f
            history_hover: rgba(255, 255, 255, 0.90),
            history_selected: rgba(10, 132, 255, 0.12),
            history_selected_border: rgba(10, 132, 255, 0.34),
            settings_bg: rgba(255, 255, 255, 0.62),
            settings_border: rgba(60, 60, 67, 0.16),
            settings_header_bg: rgba(255, 255, 255, 0.68),
            settings_header_border: rgba(60, 60, 67, 0.14),
            data_bg: rgba(255, 255, 255, 0.82),
            data_border: rgba(60, 60, 67, 0.20),

            // -- Glass / Header --
            glass_bg: rgba(255, 255, 255, 0.56),
            glass_border: rgba(60, 60, 67, 0.20),
            glass_shadow: rgba(15, 18, 26, 0.18),
            toolbar_bg: Color32::from_rgb(248, 249, 251), // #f8f9fb
            root_bg: rgba(252, 252, 255, 0.85),           // gradient approx
            header_bg: rgba(255, 255, 255, 0.56),
            header_border: rgba(60, 60, 67, 0.20),

            // -- Input --
            input_bg: rgba(255, 255, 255, 0.90),
            input_fg: Color32::from_rgb(29, 29, 31), // #1d1d1f
            input_border: rgba(60, 60, 67, 0.24),
            input_focus_border: rgba(10, 132, 255, 0.60),

            // -- Toggle --
            toggle_track_off: rgba(120, 120, 128, 0.34),
            toggle_track_on: Color32::from_rgb(10, 132, 255), // = accent
            toggle_thumb: Color32::from_rgb(255, 255, 255),   // #ffffff

            // -- Tag --
            tag_bg: rgba(255, 255, 255, 0.35),
            tag_fg: Color32::from_rgb(29, 29, 31), // #1d1d1f
            tag_border: rgba(60, 60, 67, 0.24),
            tag_active_bg: rgba(10, 132, 255, 0.12),
            tag_active_border: rgba(10, 132, 255, 0.32),

            // -- Keycap --
            keycap_bg: rgba(255, 255, 255, 0.88),
            keycap_fg: Color32::from_rgb(29, 29, 31), // #1d1d1f
            keycap_border: rgba(60, 60, 67, 0.26),
            keycap_shadow: rgba(15, 18, 26, 0.12),
            keycap_active_bg: rgba(10, 132, 255, 0.14),
            keycap_active_border: rgba(10, 132, 255, 0.36),

            // -- Range --
            range_track: rgba(255, 255, 255, 0.62),
            range_fill: Color32::from_rgb(10, 132, 255), // = accent
            range_thumb: Color32::from_rgb(255, 255, 255), // #ffffff

            // -- Modal --
            modal_bg: rgba(255, 255, 255, 0.95),
            modal_backdrop: rgba(20, 22, 28, 0.24),
            modal_border: rgba(255, 255, 255, 0.66),

            // -- Button --
            btn_hover_bg: rgba(10, 132, 255, 0.08),
            btn_active_bg: Color32::from_rgb(10, 132, 255), // #0a84ff
            btn_active_text: Color32::from_rgb(255, 255, 255), // #ffffff
            dialog_btn_bg: rgba(255, 255, 255, 0.78),
            dialog_btn_border: rgba(60, 60, 67, 0.22),
            dialog_btn_text: Color32::from_rgb(29, 29, 31), // #1d1d1f
            dialog_btn_primary_bg: Color32::from_rgb(10, 132, 255),
            dialog_btn_primary_text: Color32::from_rgb(255, 255, 255),
            dialog_btn_primary_border: rgba(8, 111, 229, 0.92),
            dialog_btn_hover_bg: rgba(255, 255, 255, 0.95),
            dialog_btn_hover_border: rgba(10, 132, 255, 0.32),

            // -- Status --
            success: Color32::from_rgb(52, 199, 89), // #34c759
            warning: Color32::from_rgb(255, 159, 10), // #ff9f0a
            danger: Color32::from_rgb(255, 69, 58),  // #ff453a
            info: Color32::from_rgb(10, 132, 255),   // = accent

            // -- Semantic --
            sensitive: Color32::from_rgb(255, 159, 10), // = warning
            sensitive_bg: rgba(255, 159, 10, 0.12),
            pinned: Color32::from_rgb(255, 159, 10), // = warning
            pinned_bg: rgba(255, 159, 10, 0.12),

            // -- Scrollbar --
            scrollbar_thumb: rgba(128, 128, 128, 0.15),
            scrollbar_hover: rgba(128, 128, 128, 0.40),

            // -- Select --
            select_menu_bg: rgba(255, 255, 255, 0.98),
            select_menu_border: rgba(60, 60, 67, 0.20),

            // -- Radius --
            radius_window: 12.0,
            radius_card: 14.0,
            radius_input: 10.0,
            radius_tag: 999.0,
            radius_keycap: 8.0,
            radius_toggle: 16.0,
            radius_dialog: 16.0,

            // -- Spacing --
            space_xs: 4.0,
            space_sm: 8.0,
            space_md: 12.0,
            space_lg: 16.0,
            space_xl: 24.0,
            space_2xl: 32.0,

            // -- Font --
            font_sans: "SF Pro Text, PingFang SC, sans-serif",
            font_display: "SF Pro Display, PingFang SC, sans-serif",
            font_mono: "SF Mono, JetBrains Mono, monospace",
            font_size_xs: 11.0,
            font_size_sm: 12.0,
            font_size_md: 13.0,
            font_size_lg: 15.0,
            font_size_xl: 18.0,
            font_weight_regular: 400.0,
            font_weight_medium: 500.0,
            font_weight_semibold: 600.0,
            font_weight_bold: 700.0,

            // -- Shadow --
            shadow_card: rgba(15, 18, 26, 0.08),
            shadow_modal: rgba(0, 0, 0, 0.20),
            shadow_header: rgba(15, 18, 26, 0.06),
            shadow_keycap: rgba(15, 18, 26, 0.12),

            // -- Blur / Saturate --
            blur_glass: 18.0,
            blur_modal: 10.0,
            blur_header: 16.0,
            saturate_glass: 1.28,

            // -- Animation --
            anim_duration_fast: 0.12,
            anim_duration_base: 0.20,
            anim_duration_slow: 0.32,
        }
    }

    /// Dark 模式 Token (深色背景 #141518, 浅色文字 #f5f5f7)。
    ///
    /// 颜色值源自 tiez-clipboard `theme.tokens.css` 的
    /// `body.dark-mode.theme-macos` 区段。
    pub fn dark() -> Self {
        Self {
            // -- Core --
            bg: Color32::from_rgb(20, 21, 24),       // #141518
            fg: Color32::from_rgb(245, 245, 247),    // #f5f5f7
            body_bg: Color32::from_rgb(20, 21, 24),  // #141518
            accent: Color32::from_rgb(90, 200, 250), // #5ac8fa (lighter for dark bg)
            accent_hover: Color32::from_rgb(127, 207, 255), // #7fcfff
            accent_soft: rgba(10, 132, 255, 0.20),
            border: rgba(255, 255, 255, 0.14),
            border_light: rgba(255, 255, 255, 0.08),
            shadow: rgba(0, 0, 0, 0.50),
            muted: Color32::from_rgb(168, 171, 180), // #a8abb4

            // -- Surface --
            card: rgba(56, 58, 67, 0.66),
            card_hover: rgba(64, 66, 76, 0.78),
            card_fg: Color32::from_rgb(245, 245, 247), // #f5f5f7
            history_bg: rgba(56, 58, 67, 0.66),
            history_fg: Color32::from_rgb(245, 245, 247), // #f5f5f7
            history_hover: rgba(62, 64, 74, 0.78),
            history_selected: rgba(10, 132, 255, 0.20),
            history_selected_border: rgba(112, 191, 255, 0.44),
            settings_bg: rgba(48, 50, 58, 0.60),
            settings_border: rgba(255, 255, 255, 0.12),
            settings_header_bg: rgba(62, 64, 74, 0.58),
            settings_header_border: rgba(255, 255, 255, 0.12),
            data_bg: rgba(255, 255, 255, 0.05),
            data_border: rgba(255, 255, 255, 0.10),

            // -- Glass / Header --
            glass_bg: rgba(44, 46, 54, 0.56),
            glass_border: rgba(255, 255, 255, 0.12),
            glass_shadow: rgba(0, 0, 0, 0.38),
            toolbar_bg: Color32::from_rgb(31, 32, 36), // #1f2024
            root_bg: rgba(35, 37, 43, 0.77),           // gradient approx
            header_bg: rgba(44, 46, 54, 0.56),
            header_border: rgba(255, 255, 255, 0.12),

            // -- Input --
            input_bg: rgba(58, 60, 68, 0.70),
            input_fg: Color32::from_rgb(245, 245, 247), // #f5f5f7
            input_border: rgba(255, 255, 255, 0.16),
            input_focus_border: rgba(10, 132, 255, 0.60),

            // -- Toggle --
            toggle_track_off: rgba(120, 120, 128, 0.46),
            toggle_track_on: Color32::from_rgb(90, 200, 250), // = accent
            toggle_thumb: rgba(244, 245, 247, 0.96),

            // -- Tag --
            tag_bg: rgba(44, 46, 54, 0.54),
            tag_fg: Color32::from_rgb(245, 245, 247), // #f5f5f7
            tag_border: rgba(255, 255, 255, 0.18),
            tag_active_bg: rgba(10, 132, 255, 0.20),
            tag_active_border: rgba(112, 191, 255, 0.44),

            // -- Keycap --
            keycap_bg: rgba(74, 76, 86, 0.84),
            keycap_fg: Color32::from_rgb(245, 245, 247), // #f5f5f7
            keycap_border: rgba(255, 255, 255, 0.20),
            keycap_shadow: rgba(0, 0, 0, 0.28),
            keycap_active_bg: rgba(10, 132, 255, 0.28),
            keycap_active_border: rgba(112, 191, 255, 0.58),

            // -- Range --
            range_track: rgba(255, 255, 255, 0.14),
            range_fill: Color32::from_rgb(90, 200, 250), // = accent
            range_thumb: rgba(244, 245, 247, 0.96),

            // -- Modal --
            modal_bg: rgba(30, 30, 30, 0.80),
            modal_backdrop: rgba(0, 0, 0, 0.56),
            modal_border: rgba(255, 255, 255, 0.10),

            // -- Button --
            btn_hover_bg: rgba(86, 89, 99, 0.90),
            btn_active_bg: rgba(10, 132, 255, 0.28),
            btn_active_text: Color32::from_rgb(169, 215, 255), // #a9d7ff
            dialog_btn_bg: rgba(74, 76, 86, 0.82),
            dialog_btn_border: rgba(255, 255, 255, 0.18),
            dialog_btn_text: Color32::from_rgb(245, 245, 247), // #f5f5f7
            dialog_btn_primary_bg: rgba(10, 132, 255, 0.85),
            dialog_btn_primary_text: Color32::from_rgb(255, 255, 255),
            dialog_btn_primary_border: rgba(112, 191, 255, 0.62),
            dialog_btn_hover_bg: rgba(86, 89, 99, 0.90),
            dialog_btn_hover_border: rgba(112, 191, 255, 0.50),

            // -- Status --
            success: Color32::from_rgb(48, 209, 88), // #30d158
            warning: Color32::from_rgb(255, 214, 10), // #ffd60a
            danger: Color32::from_rgb(255, 69, 58),  // #ff453a
            info: Color32::from_rgb(90, 200, 250),   // = accent

            // -- Semantic --
            sensitive: Color32::from_rgb(255, 214, 10), // = warning
            sensitive_bg: rgba(255, 214, 10, 0.15),
            pinned: Color32::from_rgb(255, 214, 10), // = warning
            pinned_bg: rgba(255, 214, 10, 0.15),

            // -- Scrollbar --
            scrollbar_thumb: rgba(255, 255, 255, 0.20),
            scrollbar_hover: rgba(255, 255, 255, 0.40),

            // -- Select --
            select_menu_bg: rgba(62, 64, 74, 0.98),
            select_menu_border: rgba(255, 255, 255, 0.14),

            // -- Radius (同 Light) --
            radius_window: 12.0,
            radius_card: 14.0,
            radius_input: 10.0,
            radius_tag: 999.0,
            radius_keycap: 8.0,
            radius_toggle: 16.0,
            radius_dialog: 16.0,

            // -- Spacing (同 Light) --
            space_xs: 4.0,
            space_sm: 8.0,
            space_md: 12.0,
            space_lg: 16.0,
            space_xl: 24.0,
            space_2xl: 32.0,

            // -- Font (同 Light) --
            font_sans: "SF Pro Text, PingFang SC, sans-serif",
            font_display: "SF Pro Display, PingFang SC, sans-serif",
            font_mono: "SF Mono, JetBrains Mono, monospace",
            font_size_xs: 11.0,
            font_size_sm: 12.0,
            font_size_md: 13.0,
            font_size_lg: 15.0,
            font_size_xl: 18.0,
            font_weight_regular: 400.0,
            font_weight_medium: 500.0,
            font_weight_semibold: 600.0,
            font_weight_bold: 700.0,

            // -- Shadow --
            shadow_card: rgba(0, 0, 0, 0.24),
            shadow_modal: rgba(0, 0, 0, 0.45),
            shadow_header: rgba(0, 0, 0, 0.20),
            shadow_keycap: rgba(0, 0, 0, 0.28),

            // -- Blur / Saturate (同 Light) --
            blur_glass: 18.0,
            blur_modal: 10.0,
            blur_header: 16.0,
            saturate_glass: 1.28,

            // -- Animation (同 Light) --
            anim_duration_fast: 0.12,
            anim_duration_base: 0.20,
            anim_duration_slow: 0.32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_and_dark_accent_differ() {
        let light = MacosTokens::light();
        let dark = MacosTokens::dark();
        assert_ne!(
            light.accent, dark.accent,
            "Light accent (#0a84ff) should differ from Dark accent (#5ac8fa)"
        );
    }

    #[test]
    fn light_and_dark_bg_differ() {
        let light = MacosTokens::light();
        let dark = MacosTokens::dark();
        assert_ne!(
            light.bg, dark.bg,
            "Light bg (#f5f6f8) should differ from Dark bg (#141518)"
        );
    }
}
