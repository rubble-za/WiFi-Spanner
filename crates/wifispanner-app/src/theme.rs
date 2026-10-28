// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 rubble-za. See the LICENSE and NOTICE files.

//! Material Design 3 for egui: colour roles generated from a seed colour,
//! shapes, spacing, type, and a few components egui lacks (tabs). egui has no
//! Material widgets, so this styles egui's own widgets to match the spec.

use std::sync::OnceLock;

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Margin, Stroke, TextStyle};
use material_colors::{color::Rgb, theme::ThemeBuilder};
use serde::{Deserialize, Serialize};

/// The seed colour (the corporate teal). Every colour role derives from it.
pub const SEED: u32 = 0x008080;

/// Font family for the Medium weight, used by buttons, tabs and labels.
const MEDIUM: &str = "roboto-medium";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ThemeMode {
    /// Follow the desktop's light or dark setting.
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeMode {
    pub const ALL: [ThemeMode; 3] = [ThemeMode::System, ThemeMode::Light, ThemeMode::Dark];

    pub fn name(self) -> &'static str {
        match self {
            ThemeMode::System => "Follow system",
            ThemeMode::Light => "Light",
            ThemeMode::Dark => "Dark",
        }
    }

    fn preference(self) -> egui::ThemePreference {
        match self {
            ThemeMode::System => egui::ThemePreference::System,
            ThemeMode::Light => egui::ThemePreference::Light,
            ThemeMode::Dark => egui::ThemePreference::Dark,
        }
    }
}

/// The Material 3 colour roles this app uses, for one theme.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)] // some roles are for components added in later phases
pub struct Roles {
    pub primary: Color32,
    pub on_primary: Color32,
    pub primary_container: Color32,
    pub on_primary_container: Color32,
    pub secondary_container: Color32,
    pub on_secondary_container: Color32,
    pub error: Color32,
    pub error_container: Color32,
    pub on_error_container: Color32,
    pub surface: Color32,
    pub surface_container_lowest: Color32,
    pub surface_container_low: Color32,
    pub surface_container: Color32,
    pub surface_container_high: Color32,
    pub surface_container_highest: Color32,
    pub on_surface: Color32,
    pub on_surface_variant: Color32,
    pub outline: Color32,
    pub outline_variant: Color32,
    pub inverse_surface: Color32,
    pub inverse_on_surface: Color32,
    pub shadow: Color32,
}

fn c(rgb: Rgb) -> Color32 {
    Color32::from_rgb(rgb.red, rgb.green, rgb.blue)
}

fn roles_from(s: &material_colors::scheme::Scheme) -> Roles {
    Roles {
        primary: c(s.primary),
        on_primary: c(s.on_primary),
        primary_container: c(s.primary_container),
        on_primary_container: c(s.on_primary_container),
        secondary_container: c(s.secondary_container),
        on_secondary_container: c(s.on_secondary_container),
        error: c(s.error),
        error_container: c(s.error_container),
        on_error_container: c(s.on_error_container),
        surface: c(s.surface),
        surface_container_lowest: c(s.surface_container_lowest),
        surface_container_low: c(s.surface_container_low),
        surface_container: c(s.surface_container),
        surface_container_high: c(s.surface_container_high),
        surface_container_highest: c(s.surface_container_highest),
        on_surface: c(s.on_surface),
        on_surface_variant: c(s.on_surface_variant),
        outline: c(s.outline),
        outline_variant: c(s.outline_variant),
        inverse_surface: c(s.inverse_surface),
        inverse_on_surface: c(s.inverse_on_surface),
        shadow: c(s.shadow),
    }
}

/// Light and dark roles, generated once from the seed.
fn palettes() -> &'static (Roles, Roles) {
    static PALETTES: OnceLock<(Roles, Roles)> = OnceLock::new();
    PALETTES.get_or_init(|| {
        let theme = ThemeBuilder::with_source(Rgb::from_u32(SEED)).build();
        (roles_from(&theme.schemes.light), roles_from(&theme.schemes.dark))
    })
}

pub fn roles_for(theme: egui::Theme) -> &'static Roles {
    match theme {
        egui::Theme::Light => &palettes().0,
        egui::Theme::Dark => &palettes().1,
    }
}

/// The roles for whichever theme is showing now.
pub fn roles(ctx: &egui::Context) -> &'static Roles {
    roles_for(ctx.theme())
}

/// `color` with `layer` laid over it at `opacity`: Material's "state layer"
/// for hover (8%), focus and press (12%).
pub fn state_layer(color: Color32, layer: Color32, opacity: f32) -> Color32 {
    let mix = |a: u8, b: u8| (a as f32 * (1.0 - opacity) + b as f32 * opacity).round() as u8;
    Color32::from_rgb(mix(color.r(), layer.r()), mix(color.g(), layer.g()), mix(color.b(), layer.b()))
}

/// Install the fonts, colours and shapes, and choose the light/dark mode.
/// Call once at start; call `set_mode` when the user changes the mode.
pub fn install(ctx: &egui::Context, mode: ThemeMode) {
    egui_material_icons::initialize(ctx);
    install_fonts(ctx);
    ctx.set_visuals_of(egui::Theme::Light, visuals(roles_for(egui::Theme::Light), false));
    ctx.set_visuals_of(egui::Theme::Dark, visuals(roles_for(egui::Theme::Dark), true));
    ctx.all_styles_mut(shape_style);
    set_mode(ctx, mode);
}

pub fn set_mode(ctx: &egui::Context, mode: ThemeMode) {
    ctx.set_theme(mode.preference());
}

fn install_fonts(ctx: &egui::Context) {
    use damascene_fonts_roboto::{ROBOTO_MEDIUM, ROBOTO_REGULAR};
    use egui::{FontData, FontInsert, InsertFontFamily};
    use egui::epaint::text::FontPriority;
    ctx.add_font(FontInsert::new(
        "roboto",
        FontData::from_static(ROBOTO_REGULAR),
        vec![
            InsertFontFamily { family: FontFamily::Proportional, priority: FontPriority::Highest },
            InsertFontFamily { family: FontFamily::Monospace, priority: FontPriority::Lowest },
        ],
    ));
    ctx.add_font(FontInsert::new(
        MEDIUM,
        FontData::from_static(ROBOTO_MEDIUM),
        vec![InsertFontFamily { family: FontFamily::Name(MEDIUM.into()), priority: FontPriority::Highest }],
    ));
}

/// Type scale and spacing, shared by both themes.
fn shape_style(style: &mut egui::Style) {
    let medium = FontFamily::Name(MEDIUM.into());
    style.text_styles = [
        (TextStyle::Heading, FontId::new(24.0, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(15.0, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(15.0, medium)),
        (TextStyle::Small, FontId::new(12.0, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(14.0, FontFamily::Monospace)),
    ]
    .into();
    let sp = &mut style.spacing;
    sp.item_spacing = egui::vec2(10.0, 8.0);
    sp.button_padding = egui::vec2(20.0, 9.0);
    sp.interact_size = egui::vec2(48.0, 40.0);
    sp.window_margin = Margin::same(24);
    sp.menu_margin = Margin::symmetric(8, 8);
    sp.slider_width = 200.0;
}

/// Material 3 colours and shapes mapped onto egui's widget states.
fn visuals(r: &Roles, dark: bool) -> egui::Visuals {
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = r.surface;
    v.window_fill = r.surface_container_high;
    v.extreme_bg_color = r.surface_container_lowest;
    v.faint_bg_color = r.surface_container_low;
    v.code_bg_color = r.surface_container_highest;
    v.hyperlink_color = r.primary;
    v.error_fg_color = r.error;
    v.window_stroke = Stroke::NONE;
    v.window_corner_radius = CornerRadius::same(28);
    v.menu_corner_radius = CornerRadius::same(12);
    v.window_shadow = shadow(r.shadow, 24, 0.20);
    v.popup_shadow = shadow(r.shadow, 14, 0.22);
    v.selection.bg_fill = r.primary_container;
    v.selection.stroke = Stroke::new(1.0, r.primary);
    v.text_cursor.stroke = Stroke::new(2.0, r.primary);
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Circle;
    v.indent_has_left_vline = false;
    v.striped = true;

    let radius = CornerRadius::same(12);
    let w = &mut v.widgets;
    // Labels and frames.
    w.noninteractive.bg_fill = r.surface;
    w.noninteractive.weak_bg_fill = r.surface;
    w.noninteractive.bg_stroke = Stroke::new(1.0, r.outline_variant);
    w.noninteractive.fg_stroke = Stroke::new(1.0, r.on_surface);
    w.noninteractive.corner_radius = radius;
    // Buttons at rest are "tonal": secondary container with matching text.
    w.inactive.bg_fill = r.surface_container_highest;
    w.inactive.weak_bg_fill = r.secondary_container;
    w.inactive.bg_stroke = Stroke::NONE;
    w.inactive.fg_stroke = Stroke::new(1.0, r.on_secondary_container);
    w.inactive.corner_radius = radius;
    // Hover and press add Material's 8% and 12% state layers.
    w.hovered.bg_fill = state_layer(r.surface_container_highest, r.on_surface, 0.08);
    w.hovered.weak_bg_fill = state_layer(r.secondary_container, r.on_secondary_container, 0.08);
    w.hovered.bg_stroke = Stroke::new(1.0, r.outline);
    w.hovered.fg_stroke = Stroke::new(1.0, r.on_secondary_container);
    w.hovered.corner_radius = radius;
    w.active.bg_fill = state_layer(r.surface_container_highest, r.on_surface, 0.12);
    w.active.weak_bg_fill = state_layer(r.secondary_container, r.on_secondary_container, 0.12);
    w.active.bg_stroke = Stroke::new(1.0, r.primary);
    w.active.fg_stroke = Stroke::new(1.0, r.on_secondary_container);
    w.active.corner_radius = radius;
    w.open = w.active;
    v
}

fn shadow(color: Color32, blur: u8, alpha: f32) -> egui::Shadow {
    egui::Shadow {
        offset: [0, 4],
        blur,
        spread: 0,
        color: Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), (alpha * 255.0) as u8),
    }
}

/// A Material "primary tab": medium-weight label, a rounded indicator under
/// the active tab, and state layers on hover and press.
pub fn tab(ui: &mut egui::Ui, selected: bool, label: &str, height: f32) -> egui::Response {
    let r = roles(ui.ctx());
    let font = FontId::new(15.0, FontFamily::Name(MEDIUM.into()));
    let color = if selected { r.primary } else { r.on_surface_variant };
    let galley = ui.painter().layout_no_wrap(label.to_string(), font, color);
    let size = egui::vec2(galley.size().x + 40.0, height);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let painter = ui.painter();
    if resp.hovered() || resp.is_pointer_button_down_on() {
        let opacity = if resp.is_pointer_button_down_on() { 0.12 } else { 0.08 };
        painter.rect_filled(rect.shrink2(egui::vec2(4.0, 4.0)), 12.0, r.on_surface.gamma_multiply(opacity));
    }
    painter.galley(egui::pos2(rect.center().x - galley.size().x / 2.0, rect.center().y - galley.size().y / 2.0), galley, color);
    if selected {
        let bar = egui::Rect::from_min_max(egui::pos2(rect.left() + 12.0, rect.bottom() - 3.0), egui::pos2(rect.right() - 12.0, rect.bottom()));
        painter.rect_filled(bar, CornerRadius { nw: 3, ne: 3, sw: 0, se: 0 }, r.primary);
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_seed_gives_a_teal_primary_in_both_themes() {
        let (light, dark) = (roles_for(egui::Theme::Light), roles_for(egui::Theme::Dark));
        // Teal: green and blue clearly above red.
        for p in [light.primary, dark.primary] {
            assert!(p.g() > p.r() + 20 && p.b() > p.r() + 20, "{p:?} is not teal");
        }
        assert!(light.surface.r() > 200 && dark.surface.r() < 60, "light is light, dark is dark");
        assert_ne!(light.primary, dark.primary);
    }

    #[test]
    fn text_has_enough_contrast_on_its_surface() {
        fn luminance(c: Color32) -> f32 {
            let f = |v: u8| {
                let v = v as f32 / 255.0;
                if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
            };
            0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
        }
        fn ratio(a: Color32, b: Color32) -> f32 {
            let (la, lb) = (luminance(a), luminance(b));
            (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
        }
        for theme in [egui::Theme::Light, egui::Theme::Dark] {
            let r = roles_for(theme);
            assert!(ratio(r.on_surface, r.surface) >= 7.0, "{theme:?} body text");
            assert!(ratio(r.on_secondary_container, r.secondary_container) >= 4.5, "{theme:?} tonal buttons");
            assert!(ratio(r.on_primary, r.primary) >= 4.5, "{theme:?} filled buttons");
            assert!(ratio(r.primary, r.surface) >= 3.0, "{theme:?} tab indicator against the surface");
        }
    }

    #[test]
    fn state_layers_move_toward_the_layer_colour() {
        let base = Color32::from_rgb(200, 200, 200);
        assert_eq!(state_layer(base, Color32::BLACK, 0.0), base);
        assert_eq!(state_layer(base, Color32::BLACK, 1.0), Color32::BLACK);
        assert!(state_layer(base, Color32::BLACK, 0.08).r() < 200);
    }
}
