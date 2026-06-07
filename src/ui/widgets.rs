//! macOS 风格自定义 widget 助手函数。
//!
//! 提供 toggle、range slider、collapsible group 共 3 个 helper。
//!
//! 颜色从 `MacosTokens` 读取以支持 Light/Dark 切换。

use egui::{self, Pos2, Rect, Rounding, Stroke, Vec2};

use super::MacosTokens;

const TOGGLE_W: f32 = 38.0;
const TOGGLE_H: f32 = 22.0;
const THUMB_R: f32 = 9.0;
const TRACK_R: f32 = 11.0;
const THUMB_PAD: f32 = 2.0;
const SLIDER_TRACK_H: f32 = 8.0;
const SLIDER_THUMB_R: f32 = 9.0;
const SETTINGS_R: f32 = 14.0;

/// iOS 风格 38×22 toggle 开关。
///
/// 绘制圆角轨道 + 圆形 thumb，点击切换布尔值。
pub fn macos_toggle(ui: &mut egui::Ui, value: &mut bool, theme: &MacosTokens) -> egui::Response {
    let desired_size = Vec2::new(TOGGLE_W, TOGGLE_H);
    let (rect, mut response) = ui.allocate_exact_size(desired_size, egui::Sense::click());

    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }

    if ui.is_rect_visible(rect) {
        let track_color = if *value {
            theme.toggle_track_on
        } else {
            theme.toggle_track_off
        };
        let painter = ui.painter();

        painter.rect_filled(rect, Rounding::same(TRACK_R), track_color);

        let thumb_x = if *value {
            rect.right() - THUMB_PAD - THUMB_R
        } else {
            rect.left() + THUMB_PAD + THUMB_R
        };
        let thumb_center = Pos2::new(thumb_x, rect.center().y);
        painter.circle_filled(thumb_center, THUMB_R, theme.toggle_thumb);

        response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    }

    response
}

/// macOS 风格 range slider (渐变轨道, 圆形 thumb)。
///
/// 轨道左侧填充强调色，右侧为半透明白。
pub fn macos_range_slider(
    ui: &mut egui::Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    theme: &MacosTokens,
) -> egui::Response {
    let desired_size = Vec2::new(ui.available_width(), SLIDER_TRACK_H + 12.0);
    let (rect, mut response) = ui.allocate_exact_size(desired_size, egui::Sense::click_and_drag());

    if (response.dragged() || response.clicked())
        && let Some(mouse_pos) = ui.input(|i| i.pointer.interact_pos())
    {
        let fraction = ((mouse_pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        *value = range.start() + fraction * (range.end() - range.start());
        response.mark_changed();
    }

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let track_rect =
            Rect::from_center_size(rect.center(), Vec2::new(rect.width(), SLIDER_TRACK_H));
        let track_rounding = Rounding::same(SLIDER_TRACK_H / 2.0);

        painter.rect_filled(track_rect, track_rounding, theme.range_track);

        let fraction = if *range.end() > *range.start() {
            ((*value - *range.start()) / (*range.end() - *range.start())).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let fill_width = track_rect.width() * fraction;
        if fill_width > 0.5 {
            let fill_rect =
                Rect::from_min_size(track_rect.min, Vec2::new(fill_width, track_rect.height()));
            let fill_rounding = if fraction >= 0.999 {
                track_rounding
            } else {
                Rounding {
                    nw: SLIDER_TRACK_H / 2.0,
                    sw: SLIDER_TRACK_H / 2.0,
                    ne: 0.0,
                    se: 0.0,
                }
            };
            painter.rect_filled(fill_rect, fill_rounding, theme.range_fill);
        }

        let thumb_x = track_rect.left() + fill_width;
        let thumb_center = Pos2::new(thumb_x, track_rect.center().y);
        painter.circle_filled(thumb_center, SLIDER_THUMB_R, theme.range_thumb);

        response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    }

    response
}

/// 折叠式 settings group (14px 圆角, 头部 + 可折叠 body)。
///
/// - `title`: 组标题
/// - `expanded`: 当前展开状态（外部管理）
/// - `add_body`: 展开时绘制 body 内容的闭包
pub fn macos_collapsible_group(
    ui: &mut egui::Ui,
    title: impl AsRef<str>,
    expanded: &mut bool,
    theme: &MacosTokens,
    add_body: impl FnOnce(&mut egui::Ui),
) {
    let title = title.as_ref();
    let available_width = ui.available_width();

    egui::Frame::none()
        .fill(theme.settings_bg)
        .stroke(Stroke::new(1.0, theme.settings_border))
        .rounding(Rounding::same(SETTINGS_R))
        .show(ui, |ui| {
            ui.set_width(available_width);

            let header_font = egui::FontId::new(13.0, egui::FontFamily::Proportional);
            let header_resp = egui::Frame::none()
                .fill(theme.settings_header_bg)
                .stroke(Stroke::new(1.0, theme.settings_header_border))
                .rounding(if *expanded {
                    Rounding {
                        nw: SETTINGS_R,
                        ne: SETTINGS_R,
                        sw: 0.0,
                        se: 0.0,
                    }
                } else {
                    Rounding::same(12.0)
                })
                .inner_margin(egui::Margin::symmetric(14.0, 10.0))
                .show(ui, |ui| {
                    ui.set_width(available_width - 28.0);
                    ui.horizontal(|ui| {
                        let arrow = if *expanded { "▼" } else { "▶" };
                        ui.label(
                            egui::RichText::new(arrow)
                                .font(egui::FontId::new(10.0, egui::FontFamily::Proportional))
                                .color(theme.muted),
                        );
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new(title).font(header_font).color(theme.fg));
                    });
                })
                .response;

            if header_resp.interact(egui::Sense::click()).clicked() {
                *expanded = !*expanded;
            }

            if *expanded {
                let sep_y = ui.cursor().top();
                let sep_rect = Rect::from_min_size(
                    Pos2::new(ui.cursor().left() + 14.0, sep_y),
                    Vec2::new(available_width - 42.0, 1.0),
                );
                ui.painter()
                    .rect_filled(sep_rect, Rounding::ZERO, theme.settings_header_border);
                ui.add_space(1.0);

                egui::Frame::none()
                    .inner_margin(egui::Margin {
                        left: 14.0,
                        right: 14.0,
                        top: 8.0,
                        bottom: 12.0,
                    })
                    .show(ui, |ui| {
                        ui.set_width(available_width - 28.0);
                        add_body(ui);
                    });
            }
        });
}
