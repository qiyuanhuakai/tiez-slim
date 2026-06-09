use crate::app::{APP_ID, ClipboardApp, HotkeyTarget, filter_chip};
use crate::platform;
use crate::ui::settings::hotkey_single_record_row;
use crate::ui::widgets::{macos_collapsible_group, macos_toggle};
use eframe::egui;
use rust_i18n::t;

const PRIVACY_PANEL_INDEX: usize = 7;
const SENSITIVE_KINDS: &[&str] = &["phone", "idcard", "email", "secret", "password"];
const OWN_WINDOW_CLASSES: &[&str] = &[APP_ID, "tiez-slim"];

pub fn draw_privacy_panel(ui: &mut egui::Ui, app: &mut ClipboardApp, _ctx: &egui::Context) {
    let prev = app
        .settings_panel_collapsed
        .get(PRIVACY_PANEL_INDEX)
        .copied()
        .unwrap_or(false);
    let mut expanded = !prev;
    let theme = app.theme.clone();
    macos_collapsible_group(
        ui,
        t!("settings.private_mode.title"),
        &mut expanded,
        &theme,
        |ui| {
            draw_sensitive_detection(ui, app);
            ui.add_space(8.0);
            draw_exclusion_list(ui, app);
            ui.add_space(8.0);
            draw_private_mode(ui, app);
        },
    );
    let collapsed_ref = app.settings_panel_collapsed.get_mut(PRIVACY_PANEL_INDEX);
    if let Some(collapsed) = collapsed_ref
        && expanded == *collapsed
    {
        *collapsed = !expanded;
        app.persist_preferences();
    }
}

fn draw_sensitive_detection(ui: &mut egui::Ui, app: &mut ClipboardApp) {
    if ui
        .horizontal(|ui| {
            ui.label(t!("settings.clipboard.privacy_protection"));
            macos_toggle(ui, &mut app.privacy_protection, &app.theme)
        })
        .inner
        .changed()
    {
        app.persist_preferences();
    }

    if app.privacy_protection {
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(t!("settings.clipboard.privacy_protection_kinds"))
                .size(12.0)
                .color(app.theme.muted),
        );
        ui.add_space(2.0);

        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
            for kind in SENSITIVE_KINDS {
                let mut enabled = app.privacy_protection_kinds.contains(&kind.to_string());
                let label = format!("settings.clipboard.sensitive_kind_{kind}");
                if filter_chip(ui, t!(label), enabled, &app.theme).clicked() {
                    enabled = !enabled;
                    if enabled && !app.privacy_protection_kinds.contains(&kind.to_string()) {
                        app.privacy_protection_kinds.push(kind.to_string());
                    } else if !enabled {
                        app.privacy_protection_kinds.retain(|k| k != kind);
                    }
                    changed = true;
                }
            }
        });

        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(t!("settings.clipboard.custom_rules_label"))
                .size(12.0)
                .color(app.theme.muted),
        );
        let response = ui.add(
            egui::TextEdit::multiline(&mut app.privacy_protection_custom_rules)
                .desired_rows(3)
                .desired_width(ui.available_width())
                .hint_text(t!("settings.clipboard.custom_rules_hint")),
        );
        if response.changed() {
            changed = true;
        }

        if changed {
            app.persist_preferences();
        }
    }
}

fn draw_exclusion_list(ui: &mut egui::Ui, app: &mut ClipboardApp) {
    ui.label(
        egui::RichText::new(t!("settings.exclusion_list.title"))
            .size(13.0)
            .strong(),
    );
    ui.add_space(2.0);
    ui.add_space(4.0);

    let mut to_remove: Option<usize> = None;
    let list_clone = app.app_exclusion_list.clone();
    for (i, pattern) in list_clone.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(pattern).monospace().size(12.0));
            if ui
                .button(
                    egui::RichText::new("\u{00d7}")
                        .size(12.0)
                        .color(app.theme.danger),
                )
                .clicked()
            {
                to_remove = Some(i);
            }
        });
    }
    if let Some(idx) = to_remove {
        app.app_exclusion_list.remove(idx);
        app.persist_preferences();
    }

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let input_width = (ui.available_width() - 120.0).max(80.0);
        let response = ui.add_sized(
            [input_width, 24.0],
            egui::TextEdit::singleline(&mut app.new_exclusion_input)
                .hint_text(t!("settings.exclusion_list.pattern_hint")),
        );
        let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if ui
            .button(t!("settings.exclusion_list.add_button"))
            .clicked()
            || enter
        {
            let pattern = app.new_exclusion_input.trim().to_string();
            if !pattern.is_empty() && !app.app_exclusion_list.contains(&pattern) {
                app.app_exclusion_list.push(pattern);
                app.new_exclusion_input.clear();
                app.persist_preferences();
            }
        }
    });

    ui.add_space(2.0);
    if ui
        .button(t!("settings.exclusion_list.current_window"))
        .clicked()
        && let Some(wm_class) = platform::active_window_class()
        && !is_own_window_class(&wm_class)
        && !app.app_exclusion_list.contains(&wm_class)
    {
        app.app_exclusion_list.push(wm_class);
        app.persist_preferences();
    }
}

fn draw_private_mode(ui: &mut egui::Ui, app: &mut ClipboardApp) {
    ui.label(
        egui::RichText::new(t!("settings.private_mode.enable_hint"))
            .size(11.0)
            .color(app.theme.muted),
    );
    ui.add_space(4.0);

    if ui
        .horizontal(|ui| {
            ui.label(t!("settings.private_mode.enable"));
            macos_toggle(ui, &mut app.private_mode, &app.theme)
        })
        .inner
        .changed()
    {
        app.persist_preferences();
    }

    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(t!("settings.private_mode.hotkey_hint"))
            .size(11.0)
            .color(app.theme.muted),
    );
    ui.add_space(2.0);

    let recording = matches!(app.recording_hotkey, Some(HotkeyTarget::PrivateMode));
    hotkey_single_record_row(
        ui,
        t!("settings.private_mode.hotkey_label"),
        &app.private_mode_hotkey,
        recording,
        || {
            app.recording_hotkey = Some(HotkeyTarget::PrivateMode);
        },
    );

    if app.private_mode {
        ui.add_space(6.0);
        let status_text = t!("settings.private_mode.status_active");
        let galley = ui.painter().layout_no_wrap(
            status_text.to_string(),
            egui::FontId::proportional(12.0),
            egui::Color32::WHITE,
        );
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(galley.size().x + 16.0, 22.0),
            egui::Sense::hover(),
        );
        ui.painter()
            .rect_filled(rect, egui::Rounding::same(11.0), app.theme.danger);
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            status_text,
            egui::FontId::proportional(12.0),
            egui::Color32::WHITE,
        );
    }
}

fn is_own_window_class(wm_class: &str) -> bool {
    let normalized = wm_class.trim().to_ascii_lowercase();
    OWN_WINDOW_CLASSES
        .iter()
        .any(|class| normalized == class.to_ascii_lowercase())
}
