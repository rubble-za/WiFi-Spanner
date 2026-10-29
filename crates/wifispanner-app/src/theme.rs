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

/// Text format with the glyphs centred on the line, so an icon and its label
/// (which have different sizes) sit on the same middle line.
fn centred(font: FontId, color: Color32) -> egui::TextFormat {
    egui::TextFormat { valign: egui::Align::Center, ..egui::TextFormat::simple(font, color) }
}

/// Font family for the Medium weight, used by buttons, tabs and labels.
const MEDIUM: &str = "roboto-medium";

/// How much room lists and tables take: row height, text size and spacing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Density {
    Compact,
    Smaller,
    #[default]
    Default,
    Larger,
}

impl Density {
    pub const ALL: [Density; 4] = [Density::Compact, Density::Smaller, Density::Default, Density::Larger];

    pub fn name(self) -> &'static str {
        match self {
            Density::Compact => "Compact",
            Density::Smaller => "Smaller",
            Density::Default => "Default",
            Density::Larger => "Larger",
        }
    }

    /// Minimum height of a table row, list item or field.
    pub fn row_height(self) -> f32 {
        match self {
            Density::Compact => 22.0,
            Density::Smaller => 30.0,
            Density::Default => 40.0,
            Density::Larger => 52.0,
        }
    }

    /// Height of buttons, chips and fields in the toolbars.
    pub fn button_height(self) -> f32 {
        match self {
            Density::Compact => 28.0,
            Density::Smaller => 34.0,
            Density::Default => 40.0,
            Density::Larger => 48.0,
        }
    }

    /// Height of the top bar's row (tabs and menu button).
    pub fn bar_height(self) -> f32 {
        match self {
            Density::Compact => 36.0,
            Density::Smaller => 42.0,
            Density::Default => 48.0,
            Density::Larger => 56.0,
        }
    }

    /// Vertical padding inside buttons, drop-downs and value fields, so a field's
    /// own height shrinks and grows with the density.
    pub fn padding_y(self) -> f32 {
        match self {
            Density::Compact => 3.0,
            Density::Smaller => 6.0,
            Density::Default => 9.0,
            Density::Larger => 11.0,
        }
    }

    /// Scale of the project cards in the gallery.
    pub fn card_scale(self) -> f32 {
        match self {
            Density::Compact => 0.7,
            Density::Smaller => 0.85,
            Density::Default => 1.0,
            Density::Larger => 1.2,
        }
    }

    /// Multiplier for body text sizes.
    pub fn font_scale(self) -> f32 {
        match self {
            Density::Compact => 0.84,
            Density::Smaller => 0.93,
            Density::Default => 1.0,
            Density::Larger => 1.15,
        }
    }

    /// Vertical gap between items.
    pub fn item_gap(self) -> f32 {
        match self {
            Density::Compact => 2.0,
            Density::Smaller => 5.0,
            Density::Default => 8.0,
            Density::Larger => 11.0,
        }
    }
}

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
pub fn install(ctx: &egui::Context, mode: ThemeMode, density: Density) {
    egui_material_icons::initialize(ctx);
    install_fonts(ctx);
    ctx.set_visuals_of(egui::Theme::Light, visuals(roles_for(egui::Theme::Light), false));
    ctx.set_visuals_of(egui::Theme::Dark, visuals(roles_for(egui::Theme::Dark), true));
    set_density(ctx, density);
    set_mode(ctx, mode);
}

pub fn set_mode(ctx: &egui::Context, mode: ThemeMode) {
    ctx.set_theme(mode.preference());
}

/// Apply a density to both themes' type sizes and spacing.
pub fn set_density(ctx: &egui::Context, density: Density) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("density"), density));
    ctx.all_styles_mut(|style| shape_style(style, density));
}

/// The density in use.
pub fn density(ctx: &egui::Context) -> Density {
    ctx.data(|d| d.get_temp(egui::Id::unique("density"))).unwrap_or_default()
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
fn shape_style(style: &mut egui::Style, density: Density) {
    let medium = FontFamily::Name(MEDIUM.into());
    style.text_styles = [
        (TextStyle::Heading, FontId::new(24.0, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(15.0 * density.font_scale(), FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(15.0, medium)),
        (TextStyle::Small, FontId::new(12.0 * density.font_scale().max(0.9), FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(14.0 * density.font_scale(), FontFamily::Monospace)),
    ]
    .into();
    let sp = &mut style.spacing;
    sp.item_spacing = egui::vec2(10.0, density.item_gap());
    sp.button_padding = egui::vec2(20.0, density.padding_y());
    sp.interact_size = egui::vec2(48.0, density.row_height());
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
    v.selection.stroke = Stroke::new(2.0, r.primary); // also the keyboard focus ring
    v.text_cursor.stroke = Stroke::new(2.0, r.primary);
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Circle;
    v.indent_has_left_vline = false;
    v.striped = true;

    // Pills, like the buttons: egui rounds no further than half a control's height.
    let radius = CornerRadius::same(20);
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

pub fn shadow(color: Color32, blur: u8, alpha: f32) -> egui::Shadow {
    egui::Shadow {
        offset: [0, 4],
        blur,
        spread: 0,
        color: Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), (alpha * 255.0) as u8),
    }
}

/// Medium-weight text, for headers and labels that need emphasis.
pub fn medium(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).family(FontFamily::Name(MEDIUM.into()))
}

/// Material button kinds: filled for the main action, tonal for secondary
/// ones, outlined for lower emphasis, text for the least (cancel, dismiss).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Filled,
    Tonal,
    Outlined,
    Text,
}

/// How a pill (a button or a chip) looks in each state.
struct PillStyle {
    rest: Color32,
    hover: Color32,
    press: Color32,
    fg: Color32,
    stroke: Stroke,
}

/// Vertical nudges, in points, that put the middle of the icon glyph and of
/// the label's capital letters on the pill's centre line. Icon fonts and text
/// fonts place their glyphs differently inside a line, so these were measured
/// from screenshots rather than left to the layout.
const ICON_NUDGE: f32 = -1.2;
const TEXT_NUDGE: f32 = 0.8;

/// Draw a pill: a rounded background with an optional leading icon and a
/// label, both centred. Greys out when the `Ui` is disabled.
fn pill(ui: &mut egui::Ui, style: PillStyle, icon: Option<egui_material_icons::MaterialIcon>, label: &str) -> egui::Response {
    let r = roles(ui.ctx());
    let height = density(ui.ctx()).button_height();
    let font = TextStyle::Button.resolve(ui.style());
    let text = ui.painter().layout_no_wrap(label.to_string(), font, style.fg);
    let (pad, gap, icon_size) = ((height / 2.0).clamp(14.0, 20.0), 8.0, 20.0);
    let icon_galley = icon.map(|i| ui.painter().layout_no_wrap(i.codepoint.to_string(), FontId::new(icon_size, i.font_family()), style.fg));
    let icon_w = icon_galley.as_ref().map_or(0.0, |g| g.size().x + gap);
    let content_w = icon_w + text.size().x;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2((content_w + 2.0 * pad).max(64.0), height), egui::Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
    let radius = (height / 2.0).round() as u8;
    let fill = if resp.is_pointer_button_down_on() {
        style.press
    } else if resp.hovered() {
        style.hover
    } else {
        style.rest
    };
    let painter = ui.painter();
    painter.rect(rect, radius, fill, style.stroke, egui::StrokeKind::Inside);
    if resp.has_focus() {
        painter.rect_stroke(rect.expand(2.0), radius + 2, Stroke::new(2.0, r.primary), egui::StrokeKind::Outside);
    }
    let mut x = rect.center().x - content_w / 2.0;
    if let Some(g) = icon_galley {
        let w = g.size().x;
        painter.galley(egui::pos2(x, rect.center().y - g.size().y / 2.0 + ICON_NUDGE), g, style.fg);
        x += w + gap;
    }
    painter.galley(egui::pos2(x, rect.center().y - text.size().y / 2.0 + TEXT_NUDGE), text, style.fg);
    resp
}

/// A pill-shaped Material button, optionally with a leading icon. Greys out
/// when the `Ui` is disabled (wrap in `add_enabled_ui`).
pub fn button(ui: &mut egui::Ui, kind: ButtonKind, icon: Option<egui_material_icons::MaterialIcon>, label: &str) -> egui::Response {
    let r = roles(ui.ctx());
    let (fill, fg, outline) = match kind {
        ButtonKind::Filled => (r.primary, r.on_primary, None),
        ButtonKind::Tonal => (r.secondary_container, r.on_secondary_container, None),
        ButtonKind::Outlined => (Color32::TRANSPARENT, r.primary, Some(r.outline)),
        ButtonKind::Text => (Color32::TRANSPARENT, r.primary, None),
    };
    let layer = |opacity: f32| match kind {
        ButtonKind::Filled | ButtonKind::Tonal => state_layer(fill, fg, opacity),
        ButtonKind::Outlined | ButtonKind::Text => r.primary.gamma_multiply(opacity),
    };
    let stroke = outline.map_or(Stroke::NONE, |c| Stroke::new(1.0, c));
    pill(ui, PillStyle { rest: fill, hover: layer(0.08), press: layer(0.12), fg, stroke }, icon, label)
}

/// A Material chip, shaped and sized like the buttons: outlined at rest,
/// filled with a check mark when selected. Use a row of them for a choice
/// among a few options.
pub fn chip(ui: &mut egui::Ui, selected: bool, icon: Option<egui_material_icons::MaterialIcon>, label: &str) -> egui::Response {
    let r = roles(ui.ctx());
    let (fill, fg, stroke) = if selected {
        (r.secondary_container, r.on_secondary_container, Stroke::NONE)
    } else {
        (Color32::TRANSPARENT, r.on_surface_variant, Stroke::new(1.0, r.outline))
    };
    let layer = |opacity: f32| if selected { state_layer(fill, fg, opacity) } else { r.on_surface.gamma_multiply(opacity) };
    let icon = if selected { Some(egui_material_icons::icons::ICON_CHECK) } else { icon };
    pill(ui, PillStyle { rest: fill, hover: layer(0.08), press: layer(0.12), fg, stroke }, icon, label)
}

/// A menu entry with a check mark column that is invisible when unchecked, so
/// every row has the identical layout whatever is selected.
pub fn check_label(ui: &egui::Ui, checked: bool, label: &str) -> egui::text::LayoutJob {
    let color = ui.visuals().text_color();
    let check = if checked { color } else { Color32::TRANSPARENT };
    let icon = egui_material_icons::icons::ICON_CHECK;
    let mut job = egui::text::LayoutJob::default();
    job.append(icon.codepoint, 0.0, centred(FontId::new(20.0, icon.font_family()), check));
    job.append(label, 12.0, centred(TextStyle::Body.resolve(ui.style()), color));
    job
}

/// Text with an optional leading Material icon, for menu entries. With no icon
/// the label is indented by the icon's width so a menu's labels line up.
pub fn icon_label(ui: &egui::Ui, icon: Option<egui_material_icons::MaterialIcon>, label: &str) -> egui::text::LayoutJob {
    let color = ui.visuals().text_color();
    let body = TextStyle::Body.resolve(ui.style());
    let mut job = egui::text::LayoutJob::default();
    match icon {
        Some(icon) => {
            job.append(icon.codepoint, 0.0, centred(FontId::new(20.0, icon.font_family()), color));
            job.append(label, 12.0, centred(body, color));
        }
        None => job.append(label, 32.0, centred(body, color)),
    }
    job
}

/// A Material snackbar: a short message near the bottom of the window.
/// Returns true when the user clicks it away.
pub fn snackbar(ctx: &egui::Context, text: &str) -> bool {
    let r = roles(ctx);
    egui::Area::new(egui::Id::unique("snackbar"))
        .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -48.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let frame = egui::Frame::new()
                .fill(r.inverse_surface)
                .corner_radius(8)
                .inner_margin(egui::Margin::symmetric(20, 14))
                .shadow(shadow(r.shadow, 14, 0.25));
            let shown = frame.show(ui, |ui| {
                ui.set_max_width(520.0);
                ui.label(egui::RichText::new(text).color(r.inverse_on_surface));
            });
            ui.interact(shown.response.rect, egui::Id::unique("snackbar-dismiss"), egui::Sense::click()).clicked()
        })
        .inner
}

/// A centred empty state: a large icon, a headline, a sentence, and actions.
pub fn empty_state(
    ui: &mut egui::Ui,
    icon: egui_material_icons::MaterialIcon,
    headline: &str,
    body: &str,
    actions: impl FnOnce(&mut egui::Ui),
) {
    let r = roles(ui.ctx());
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() * 0.25).max(24.0));
        ui.label(icon.rich_text().size(64.0).color(r.outline));
        ui.add_space(8.0);
        ui.label(egui::RichText::new(headline).size(22.0).color(r.on_surface));
        ui.add_space(4.0);
        ui.label(egui::RichText::new(body).color(r.on_surface_variant));
        ui.add_space(16.0);
        actions(ui);
    });
}

/// A Material switch (52 x 32 track, thumb that grows when on).
pub fn switch(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    let r = roles(ui.ctx());
    let (rect, mut resp) = ui.allocate_exact_size(egui::vec2(52.0, 32.0), egui::Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_responsive(resp.id, *on);
    let fade = if ui.is_enabled() { 1.0 } else { 0.38 };
    let lerp = |a: Color32, b: Color32| a.lerp_to_gamma(b, t).gamma_multiply(fade);
    let painter = ui.painter();
    painter.rect_filled(rect, 16.0, lerp(r.surface_container_highest, r.primary));
    if t < 1.0 {
        painter.rect_stroke(rect, 16.0, Stroke::new(2.0 * (1.0 - t), r.outline.gamma_multiply(fade)), egui::StrokeKind::Inside);
    }
    let pressed = resp.is_pointer_button_down_on();
    let radius = egui::lerp(8.0..=12.0, t) + if pressed { 2.0 } else { 0.0 };
    let x = egui::lerp((rect.left() + 16.0)..=(rect.right() - 16.0), t);
    let centre = egui::pos2(x, rect.center().y);
    if resp.hovered() || pressed {
        painter.circle_filled(centre, 20.0, lerp(r.on_surface, r.primary).gamma_multiply(if pressed { 0.12 } else { 0.08 }));
    }
    painter.circle_filled(centre, radius, lerp(r.outline, r.on_primary));
    resp
}

/// A switch with its label beside it; clicking the label toggles too.
pub fn switch_row(ui: &mut egui::Ui, on: &mut bool, label: &str) -> egui::Response {
    ui.horizontal(|ui| {
        let mut resp = switch(ui, on);
        let text = ui.add(egui::Label::new(label).sense(egui::Sense::click()));
        if text.clicked() {
            *on = !*on;
            resp.mark_changed();
        }
        resp
    })
    .inner
}

/// An outlined Material card.
pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> egui::InnerResponse<R> {
    let r = roles(ui.ctx());
    egui::Frame::new()
        .fill(r.surface_container_low)
        .stroke(Stroke::new(1.0, r.outline_variant))
        .corner_radius(12)
        .inner_margin(16)
        .show(ui, add)
}

/// The frame for a page's content: Material's 16 dp margins.
pub fn page_frame(ctx: &egui::Context) -> egui::Frame {
    egui::Frame::new().fill(roles(ctx).surface).inner_margin(16)
}

/// A Material dialog: centred, 28 dp corners, a headline, then the content.
/// Returns what the content returned while it is open.
pub fn dialog<R>(ctx: &egui::Context, title: &str, width: f32, add: impl FnOnce(&mut egui::Ui) -> R) -> egui::InnerResponse<Option<R>> {
    let r = roles(ctx);
    // Never wider than the window it sits in.
    let width = width.min((ctx.content_rect().width() - 80.0).max(280.0));
    let frame = egui::Frame::new()
        .fill(r.surface_container_high)
        .corner_radius(28)
        .inner_margin(24)
        .shadow(shadow(r.shadow, 24, 0.20));
    egui::Window::new(title)
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .frame(frame)
        .show(ctx, |ui| {
            ui.set_width(width);
            ui.label(egui::RichText::new(title).size(24.0));
            ui.add_space(12.0);
            add(ui)
        })
        .expect("a dialog is always open when shown")
}

/// A right-aligned row for a dialog's buttons. Buttons are laid out right to
/// left, so add the main action first and the dismissive one last.
pub fn actions(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(16.0);
    // A fixed-height row: a right-to-left layout would otherwise take all the
    // height left in the window and leave a gap above the buttons.
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 40.0),
        egui::Layout::right_to_left(egui::Align::Center),
        add,
    );
}

/// A navigation item (as in a navigation drawer): a pill that fills with the
/// secondary container when selected.
pub fn nav_item(ui: &mut egui::Ui, selected: bool, label: &str) -> egui::Response {
    let r = roles(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 48.0), egui::Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, 24.0, r.secondary_container);
    } else if resp.hovered() || resp.is_pointer_button_down_on() {
        let opacity = if resp.is_pointer_button_down_on() { 0.12 } else { 0.08 };
        painter.rect_filled(rect, 24.0, r.on_surface.gamma_multiply(opacity));
    }
    let color = if selected { r.on_secondary_container } else { r.on_surface_variant };
    painter.text(
        egui::pos2(rect.left() + 20.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::new(15.0, FontFamily::Name(MEDIUM.into())),
        color,
    );
    resp
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
            assert!(ratio(r.primary, r.surface_container_high) >= 4.5, "{theme:?} text buttons in dialogs");
            assert!(ratio(r.primary, r.secondary_container) >= 3.0, "{theme:?} text buttons in the update banner");
            assert!(ratio(r.on_surface_variant, r.surface) >= 4.5, "{theme:?} table headers and hints");
            assert!(ratio(r.on_surface_variant, r.surface_container_high) >= 4.5, "{theme:?} hints in dialogs");
            assert!(ratio(r.error, r.surface_container_high) >= 4.5, "{theme:?} error text in dialogs");
            assert!(ratio(r.error, r.surface) >= 4.5, "{theme:?} error text on pages");
            assert!(ratio(r.inverse_on_surface, r.inverse_surface) >= 7.0, "{theme:?} snackbar text");
            assert!(ratio(r.on_surface_variant, r.surface_container) >= 4.5, "{theme:?} status bar text");
        }
    }

    #[test]
    fn state_layers_move_toward_the_layer_colour() {
        let base = Color32::from_rgb(200, 200, 200);
        assert_eq!(state_layer(base, Color32::BLACK, 0.0), base);
        assert_eq!(state_layer(base, Color32::BLACK, 1.0), Color32::BLACK);
        assert!(state_layer(base, Color32::BLACK, 0.08).r() < 200);
    }

    #[test]
    fn density_levels_grow_in_row_height_text_and_gaps() {
        let all = Density::ALL;
        for pair in all.windows(2) {
            assert!(pair[0].row_height() < pair[1].row_height());
            assert!(pair[0].font_scale() < pair[1].font_scale());
            assert!(pair[0].item_gap() < pair[1].item_gap());
            assert!(pair[0].button_height() < pair[1].button_height());
            assert!(pair[0].bar_height() < pair[1].bar_height());
        }
        assert_eq!((Density::Default.row_height(), Density::Default.font_scale()), (40.0, 1.0), "default is unchanged");
        assert_eq!((Density::Default.button_height(), Density::Default.bar_height()), (40.0, 48.0), "default bars are unchanged");
        // Buttons are never taller than the bar they sit in.
        for d in Density::ALL {
            assert!(d.button_height() < d.bar_height());
        }
    }
}
