// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 rubble-za. See the LICENSE and NOTICE files.

//! The tabbed settings dialog.

use eframe::egui;
use wifispanner_core::heatmap::{self, ColorScheme};
use wifispanner_scan::iw::WifiCard;

use crate::settings::{DWELLS, MarkerSize, SIGNAL_CEIL, SIGNAL_FLOOR, Settings};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Section {
    #[default]
    Scanner,
    Survey,
    Heatmap,
    General,
}

impl Section {
    const ALL: [(Section, &'static str); 4] = [
        (Section::Scanner, "Scanner"),
        (Section::Survey, "Survey"),
        (Section::Heatmap, "Heatmap"),
        (Section::General, "General"),
    ];
}

/// What the General section needs to know about the desktop launcher, and
/// where it reports the user's request back (true = add, false = remove).
pub struct LauncherView {
    pub available: bool,
    pub installed: bool,
    pub message: String,
    pub request: Option<bool>,
}

/// Show the dialog while `open`. Returns true if any setting changed.
pub fn show(
    ctx: &egui::Context,
    open: &mut bool,
    section: &mut Section,
    settings: &mut Settings,
    cards: &[WifiCard],
    launcher: &mut LauncherView,
) -> bool {
    let before = settings.clone();
    let mut close = false;
    crate::theme::dialog(ctx, "Settings", 680.0, |ui| {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(160.0);
                for (s, name) in Section::ALL {
                    if crate::theme::nav_item(ui, *section == s, name).clicked() {
                        *section = s;
                    }
                }
            });
            ui.add_space(16.0);
            ui.vertical(|ui| {
                ui.set_width(480.0);
                egui::ScrollArea::vertical().max_height(420.0).auto_shrink([false, false]).show(ui, |ui| match *section {
                    Section::Scanner => scanner(ui, settings, cards),
                    Section::Survey => survey(ui, settings),
                    Section::Heatmap => heatmap_section(ui, settings),
                    Section::General => general(ui, settings, launcher),
                });
            });
        });
        crate::theme::actions(ui, |ui| {
            if crate::theme::button(ui, crate::theme::ButtonKind::Filled, None, "Done").clicked() {
                close = true;
            }
        });
    });
    if close {
        *open = false;
    }
    *settings != before
}

fn general(ui: &mut egui::Ui, s: &mut Settings, launcher: &mut LauncherView) {
    ui.label(crate::theme::medium("Appearance"));
    ui.horizontal(|ui| {
        for mode in crate::theme::ThemeMode::ALL {
            ui.selectable_value(&mut s.theme_mode, mode, mode.name());
        }
    });
    ui.add_space(14.0);
    crate::theme::switch_row(ui, &mut s.check_updates, "Check for updates when the app starts");
    ui.weak(
        "Asks GitHub whether a newer release exists, which shares your IP address with GitHub. You can also check \
         any time from the menu: Check for updates.",
    );
    ui.add_space(14.0);
    ui.label(crate::theme::medium("Application menu and icon"));
    if !launcher.available {
        ui.weak("Available when running the AppImage.");
    } else if launcher.installed {
        ui.label("WiFi Spanner is in your application menu.");
        if crate::theme::button(ui, crate::theme::ButtonKind::Outlined, Some(egui_material_icons::icons::ICON_DELETE), "Remove launcher").clicked() {
            launcher.request = Some(false);
        }
    } else {
        ui.weak(
            "Adds a launcher and the WiFi Spanner icon to your home folder, so the app shows its icon in the taskbar \
             and title bar and can be started from the menu.",
        );
        if crate::theme::button(ui, crate::theme::ButtonKind::Tonal, Some(egui_material_icons::icons::ICON_ADD), "Add launcher").clicked() {
            launcher.request = Some(true);
        }
    }
    if !launcher.message.is_empty() {
        ui.weak(&launcher.message);
    }
}

fn scanner(ui: &mut egui::Ui, s: &mut Settings, cards: &[WifiCard]) {
    ui.label(crate::theme::medium("Scan time"));
    egui::ComboBox::from_id_salt("dwell").selected_text(format!("{} s", s.dwell_secs)).show_ui(ui, |ui| {
        for d in DWELLS {
            ui.selectable_value(&mut s.dwell_secs, d, format!("{d} s"));
        }
    });
    ui.weak(
        "Scanning repeats for this long and the readings are averaged. Longer is steadier and gives 6 GHz networks \
         more chances to be heard, so use 5 or 10 s for surveys.",
    );
    ui.add_space(12.0);
    ui.label(crate::theme::medium("Wi-Fi radios"));
    ui.weak("Pick the card from the menu: Adapters.");
    let multi = Settings::radio_count(cards) > 1;
    ui.add_enabled_ui(multi, |ui| {
        crate::theme::switch_row(ui, &mut s.use_all, "Use all").on_disabled_hover_text("Needs a second Wi-Fi adapter");
    });
    ui.weak(if multi {
        "Scans with every Wi-Fi radio at once, which needs driver support for each. Readings from different cards \
         are never averaged together: for an AP heard by several, the better-supported reading is kept."
    } else {
        "Only one Wi-Fi radio found. Plug in a second adapter and press Refresh cards in the Adapters menu."
    });
}

fn survey(ui: &mut egui::Ui, s: &mut Settings) {
    crate::theme::switch_row(ui, &mut s.show_plan, "Show floor plan");
    crate::theme::switch_row(ui, &mut s.show_path, "Show path between points");
    crate::theme::switch_row(ui, &mut s.gray_unmeasured, "Gray out unmeasured area");
    crate::theme::switch_row(ui, &mut s.show_numbers, "Show point numbers");
    ui.add_space(8.0);
    ui.label(crate::theme::medium("Point marker size"));
    egui::ComboBox::from_id_salt("marker").selected_text(s.marker_size.name()).show_ui(ui, |ui| {
        for m in MarkerSize::ALL {
            ui.selectable_value(&mut s.marker_size, m, m.name());
        }
    });
    ui.add_space(12.0);
    if crate::theme::button(ui, crate::theme::ButtonKind::Outlined, Some(egui_material_icons::icons::ICON_REFRESH), "Reset survey settings").clicked() {
        s.reset_survey();
    }
}

fn heatmap_section(ui: &mut egui::Ui, s: &mut Settings) {
    ui.label(crate::theme::medium("Heatmap colors"));
    ui.horizontal(|ui| {
        for scheme in ColorScheme::ALL {
            if swatch(ui, scheme, s.color_scheme == scheme, s.min_dbm, s.max_dbm).clicked() {
                s.color_scheme = scheme;
            }
        }
    });
    ui.add_space(8.0);
    ui.label(crate::theme::medium("Signal range"));
    ui.weak("Signal at or below the minimum shows as the weakest colour; at or above the maximum, the strongest.");
    signal_slider(ui, "Min signal level, dBm", &mut s.min_dbm, SIGNAL_FLOOR, s.max_dbm - 5.0);
    signal_slider(ui, "Max signal level, dBm", &mut s.max_dbm, s.min_dbm + 5.0, SIGNAL_CEIL);
    ui.add_space(8.0);
    ui.label(crate::theme::medium("Detection sensitivity"));
    ui.weak("Readings weaker than this are treated as not heard.");
    signal_slider(ui, "Weakest signal counted, dBm", &mut s.detect_dbm, SIGNAL_FLOOR, -40.0);
    ui.add_space(12.0);
    if crate::theme::button(ui, crate::theme::ButtonKind::Outlined, Some(egui_material_icons::icons::ICON_REFRESH), "Reset heatmap settings").clicked() {
        s.reset_heatmap();
    }
}

fn signal_slider(ui: &mut egui::Ui, label: &str, value: &mut f64, lo: f64, hi: f64) {
    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(value, lo..=hi).integer().text(label));
    });
}

/// A selectable colour-ramp preview.
fn swatch(ui: &mut egui::Ui, scheme: ColorScheme, selected: bool, min_dbm: f64, max_dbm: f64) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(110.0, 40.0), egui::Sense::click());
    let painter = ui.painter();
    let stroke = if selected {
        egui::Stroke::new(2.5, ui.visuals().selection.stroke.color)
    } else {
        egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color)
    };
    painter.rect_filled(rect, 4.0, ui.visuals().widgets.inactive.bg_fill);
    let bar = rect.shrink2(egui::vec2(10.0, 12.0));
    let steps = 40;
    for i in 0..steps {
        let t = i as f64 / (steps - 1) as f64;
        let [r, g, b] = heatmap::color_in(scheme, min_dbm + t * (max_dbm - min_dbm), min_dbm, max_dbm);
        let x0 = bar.left() + bar.width() * i as f32 / steps as f32;
        painter.rect_filled(
            egui::Rect::from_min_max(egui::pos2(x0, bar.top()), egui::pos2(x0 + bar.width() / steps as f32 + 1.0, bar.bottom())),
            0.0,
            egui::Color32::from_rgb(r, g, b),
        );
    }
    painter.rect_stroke(rect, 4.0, stroke, egui::StrokeKind::Inside);
    resp.on_hover_text(scheme.name())
}
