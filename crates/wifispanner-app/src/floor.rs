// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 rubble-za. See the LICENSE and NOTICE files.

//! The Surveying workspace: import a plan, calibrate its scale, click spots to
//! scan, and view the measurements as a heatmap.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, TextureHandle, TextureOptions, Vec2};
use wifispanner_core::heatmap::{self, Sample};
use wifispanner_core::survey::{Floor, Project, SurveyPoint, Target};
use wifispanner_core::walls::{self, Wall};
use wifispanner_core::Material;
use wifispanner_scan::{Band, ScanResult};

use crate::settings::Settings;
use crate::theme::{self, ButtonKind};
use egui_material_icons::icons::{ICON_ARCHITECTURE, ICON_ARROW_BACK, ICON_HIGHLIGHT_ALT, ICON_DELETE, ICON_DESCRIPTION, ICON_FOLDER_OPEN, ICON_IMAGE, ICON_PLACE, ICON_SAVE, ICON_STRAIGHTEN, ICON_UNDO};
use crate::{Cmd, pdf, report, store};
use crate::live::signal_color;

/// What the tab needs from the app each frame.
pub struct Env<'a> {
    pub tx: &'a Sender<Cmd>,
    pub dwell_secs: u64,
    pub scans_so_far: usize,
    pub settings: &'a Settings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Survey,
    Scale,
    /// Draw walls: click the start, click the end.
    Walls,
    /// Pick walls by clicking or dragging a box, then edit them together.
    Select,
}

/// Textures are capped at this size per side; positions stay in plan pixels.
const MAX_TEXTURE_SIDE: u32 = 4096;
/// Preview size when choosing a PDF page: render side and button size.
const THUMB_SIDE_PX: f32 = 360.0;
const THUMB_CELL: f32 = 150.0;
/// The page picker shows this many pages per row, then scrolls.
const PICKER_COLUMNS: usize = 4;
const PICKER_GAP: f32 = 10.0;
/// Four cells, three gaps and room for the scroll bar.
const PICKER_W: f32 = 4.0 * THUMB_CELL + 3.0 * PICKER_GAP + 34.0;
/// Heatmap grid resolution along the longer side.
const HEAT_CELLS: usize = 256;
/// The two points used to set the scale must be at least this far apart (plan
/// pixels): closer than that (a double-click, say) the result is meaningless.
const MIN_SCALE_PX: f32 = 25.0;
/// A click this close (screen pixels) to a marker selects it instead of scanning.
const PICK_RADIUS: f32 = 10.0;
/// A wall end snaps to another wall's end this close (screen pixels).
const WALL_SNAP_PX: f32 = 12.0;
/// A click this close (screen pixels) to a wall selects it.
const WALL_PICK_PX: f32 = 8.0;
/// A new wall replaces the part of an older one it lies along within this many screen pixels.
const WALL_CARVE_PX: f32 = 8.0;
/// How many undo steps are kept.
const WALL_HISTORY: usize = 100;

/// The scale (metres per plan pixel) implied by `metres` between two points
/// `px_apart` pixels apart on a plan of `w` x `h` pixels, or why it is not
/// believable.
fn check_scale(px_apart: f32, metres: f32, w: u32, h: u32) -> Result<f32, String> {
    if px_apart < MIN_SCALE_PX {
        return Err(format!("The two points are only {px_apart:.0} px apart. Cancel and click two points much further apart."));
    }
    let mpp = metres / px_apart;
    if !heatmap::scale_is_plausible(mpp, w, h) {
        let plan_m = w.max(h) as f32 * mpp;
        return Err(format!("That would make the plan {plan_m:.0} m across, which looks wrong. Check the distance you entered."));
    }
    Ok(mpp)
}

pub struct FloorTab {
    project: Option<Project>,
    path: Option<PathBuf>,
    texture: Option<TextureHandle>,
    offset: Vec2,
    zoom: f32,
    needs_fit: bool,
    tool: Tool,
    scale_clicks: Vec<Pos2>,
    scale_input: String,
    selected: Option<usize>,
    pending: Option<Pos2>,
    show_heat: bool,
    target: Target,
    band: Option<Band>,
    opacity: f32,
    fade_m: f32,
    heat_texture: Option<TextureHandle>,
    heat_dirty: bool,
    status: String,
    /// The last status shown as a snackbar, to spot changes.
    toasted: String,
    /// A project was just loaded, so the app should show the workspace.
    opened: bool,
    /// The user asked to go back to the project gallery.
    back: bool,
    /// Display settings from the settings dialog, copied in each frame.
    view: Settings,
    pdf_picker: Option<PdfPicker>,
    /// Wall drawing: the material and height for the next wall, where the wall
    /// being drawn started, and the wall picked in the list.
    wall_material: Material,
    wall_height: Option<f32>,
    wall_start: Option<Pos2>,
    selected_walls: BTreeSet<usize>,
    /// Earlier states of the walls, for undo.
    wall_history: Vec<Vec<Wall>>,
    last_wall_edit: Option<std::time::Instant>,
    /// Plan position where a selection box was started.
    box_start: Option<Pos2>,
    report_dialog: Option<report::OptionsDialog>,
}

/// What the wall palette asked for.
enum WallEdit {
    Tool(Tool),
    Material(Material),
    Height(Option<f32>),
    Ceiling(f32),
    Undo,
    Delete,
}

/// Thumbnails of a multi-page PDF, shown so the user can pick a page.
struct PdfPicker {
    name: String,
    bytes: Vec<u8>,
    selected: usize,
    thumbs: Vec<Option<TextureHandle>>,
    rx: Receiver<(usize, Vec<u8>)>,
}

impl Default for FloorTab {
    fn default() -> Self {
        FloorTab {
            project: None,
            path: None,
            texture: None,
            offset: Vec2::ZERO,
            zoom: 1.0,
            needs_fit: true,
            tool: Tool::Survey,
            scale_clicks: Vec::new(),
            scale_input: String::new(),
            selected: None,
            pending: None,
            show_heat: false,
            target: Target::Any,
            band: None,
            opacity: 0.65,
            fade_m: 8.0,
            heat_texture: None,
            heat_dirty: true,
            status: "Open a floor plan image to begin.".into(),
            toasted: "Open a floor plan image to begin.".into(),
            opened: false,
            back: false,
            view: Settings::default(),
            pdf_picker: None,
            wall_material: Material::Concrete,
            wall_height: Material::Concrete.default_height_m(),
            wall_start: None,
            selected_walls: BTreeSet::new(),
            wall_history: Vec::new(),
            last_wall_edit: None,
            box_start: None,
            report_dialog: None,
        }
    }
}

impl FloorTab {
    pub fn add_point(&mut self, x: f32, y: f32, dwell_secs: u64, scan: &ScanResult) {
        self.pending = None;
        let Some(p) = self.project.as_mut() else { return };
        let point = SurveyPoint::from_scan(x, y, dwell_secs, scan);
        self.status = format!("Point {} saved: {} APs heard.", p.floor.points.len() + 1, point.observations.len());
        p.floor.points.push(point);
        self.selected = Some(p.floor.points.len() - 1);
        self.heat_dirty = true;
        self.autosave();
    }

    /// True once after a project was loaded.
    pub fn take_opened(&mut self) -> bool {
        std::mem::take(&mut self.opened)
    }

    /// True once after the user pressed the back button.
    pub fn take_back(&mut self) -> bool {
        std::mem::take(&mut self.back)
    }

    /// A new status message since the last call, for a snackbar.
    pub fn take_toast(&mut self) -> Option<String> {
        if self.status == self.toasted {
            return None;
        }
        self.toasted = self.status.clone();
        (!self.status.is_empty()).then(|| self.status.clone())
    }

    pub fn scan_failed(&mut self) {
        self.pending = None;
        self.status = "Scan failed, no point was saved.".into();
    }

    fn autosave(&mut self) {
        if let Some(path) = self.path.clone() {
            self.save_to(&path);
        }
    }

    fn save_to(&mut self, path: &PathBuf) {
        let Some(p) = &self.project else { return };
        match p.to_json().map_err(|e| e.to_string()).and_then(|j| std::fs::write(path, j).map_err(|e| e.to_string())) {
            Ok(()) => self.path = Some(path.clone()),
            Err(e) => self.status = format!("Could not save {}: {e}", path.display()),
        }
    }

    fn load_plan(&mut self, ctx: &egui::Context, path: &std::path::Path) {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => return self.status = format!("Could not read {}: {e}", path.display()),
        };
        let name = path.file_stem().map_or("Floor".into(), |s| s.to_string_lossy().into_owned());
        if !pdf::is_pdf(&bytes) {
            return self.finish_plan(ctx, bytes, name, "");
        }
        match pdf::page_count(&bytes) {
            Err(e) => self.status = format!("Could not import PDF: {e}"),
            Ok(1) => self.import_pdf_page(ctx, bytes, 0, 1, name),
            Ok(n) => {
                let (tx, rx) = mpsc::channel();
                let (data, repaint) = (bytes.clone(), ctx.clone());
                std::thread::spawn(move || {
                    pdf::thumbnails(data, THUMB_SIDE_PX, |i, png| {
                        let open = tx.send((i, png)).is_ok();
                        repaint.request_repaint();
                        open
                    })
                });
                self.pdf_picker = Some(PdfPicker { name, bytes, selected: 0, thumbs: vec![None; n], rx });
            }
        }
    }

    fn import_pdf_page(&mut self, ctx: &egui::Context, bytes: Vec<u8>, index: usize, count: usize, name: String) {
        match pdf::render_page(bytes, index) {
            Ok(png) => {
                let note = if count > 1 { format!(" Imported page {} of {count}.", index + 1) } else { String::new() };
                self.finish_plan(ctx, png, name, &note);
            }
            Err(e) => self.status = format!("Could not import PDF page: {e}"),
        }
    }

    fn finish_plan(&mut self, ctx: &egui::Context, bytes: Vec<u8>, name: String, note: &str) {
        let dims = image::load_from_memory(&bytes).map(|i| (i.width(), i.height()));
        let (w, h) = match dims {
            Ok(d) => d,
            Err(e) => return self.status = format!("Not a usable image: {e}"),
        };
        let floor = Floor { name, image: bytes, width_px: w, height_px: h, metres_per_px: None, points: Vec::new(), walls: Vec::new(), ceiling_m: wifispanner_core::walls::DEFAULT_CEILING_M };
        let name = floor.name.clone();
        self.set_project(ctx, Project::new(floor), None);
        self.status = format!("Floor plan loaded.{note} Set the scale first: click two points a known distance apart.");
        self.tool = Tool::Scale;
        // Create the project file straight away so every point is autosaved.
        match store::new_project_path(&name) {
            Ok(path) => {
                self.save_to(&path);
                if self.path.is_some() {
                    self.status.push_str(&format!(" Saved as {}.", path.display()));
                }
            }
            Err(e) => self.status.push_str(&format!(" Could not create a project file ({e}); use Save as."))
        }
    }

    fn pdf_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut picker) = self.pdf_picker.take() else { return };
        while let Ok((i, png)) = picker.rx.try_recv() {
            if let Some(slot) = picker.thumbs.get_mut(i) {
                *slot = decode_texture(ctx, &png, &format!("pdf-thumb-{i}"));
            }
        }
        let count = picker.thumbs.len();
        let (mut import, mut cancel) = (false, false);
        theme::dialog(ctx, "Choose a page to import", PICKER_W, |ui| {
            ui.label(format!("{} has {count} pages. Click a page, then Import (or double-click it).", picker.name));
            let loaded = picker.thumbs.iter().filter(|t| t.is_some()).count();
            if loaded < count {
                ui.weak(format!("Rendering previews... {loaded} of {count}"));
            }
            ui.add_space(8.0);
            // Four pages across, then a list that scrolls up and down, however many pages there are.
            let height = (ctx.content_rect().height() - 320.0).clamp(180.0, 520.0);
            egui::ScrollArea::vertical().max_height(height).auto_shrink([false, true]).show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::splat(PICKER_GAP);
                // Room above the first row so the selection ring is not clipped.
                ui.add_space(5.0);
                for first in (0..count).step_by(PICKER_COLUMNS) {
                    ui.horizontal(|ui| {
                        for i in first..(first + PICKER_COLUMNS).min(count) {
                            ui.vertical(|ui| {
                                let selected = picker.selected == i;
                                let size = Vec2::splat(THUMB_CELL);
                                let resp = match &picker.thumbs[i] {
                                    Some(t) => ui.add(egui::Button::image(egui::Image::new(t).max_size(size)).selected(selected).min_size(size)),
                                    None => ui.add_sized(size, egui::Button::new("Loading...").selected(selected)),
                                };
                                if selected {
                                    // A bold ring in the primary colour, so the chosen page is unmistakable.
                                    let primary = theme::roles(ui.ctx()).primary;
                                    ui.painter().rect_stroke(resp.rect.expand(1.5), 20.0, Stroke::new(3.0, primary), egui::StrokeKind::Outside);
                                    ui.label(theme::medium(format!("Page {}", i + 1)).color(primary));
                                } else {
                                    ui.label(format!("Page {}", i + 1));
                                }
                                if resp.clicked() {
                                    picker.selected = i;
                                }
                                if resp.double_clicked() {
                                    picker.selected = i;
                                    import = true;
                                }
                            });
                        }
                    });
                }
            });
            theme::actions(ui, |ui| {
                if theme::button(ui, ButtonKind::Filled, None, &format!("Import page {}", picker.selected + 1)).clicked() {
                    import = true;
                }
                cancel = theme::button(ui, ButtonKind::Text, None, "Cancel").clicked();
            });
        });
        if import {
            let PdfPicker { name, bytes, selected, .. } = picker;
            self.import_pdf_page(ctx, bytes, selected, count, name);
        } else if !cancel {
            self.pdf_picker = Some(picker);
        }
    }

    pub fn open_project(&mut self, ctx: &egui::Context, path: PathBuf) {
        let loaded = std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|t| Project::from_json(&t).map_err(|e| e.to_string()));
        match loaded {
            Ok(p) => {
                self.set_project(ctx, p, Some(path));
                self.status = "Project opened.".into();
            }
            Err(e) => self.status = format!("Could not open project: {e}"),
        }
    }

    fn set_project(&mut self, ctx: &egui::Context, project: Project, path: Option<PathBuf>) {
        self.opened = true;
        self.texture = decode_texture(ctx, &project.floor.image, "floor-plan");
        self.project = Some(project);
        self.path = path;
        self.selected = None;
        self.pending = None;
        self.scale_clicks.clear();
        self.needs_fit = true;
        self.heat_dirty = true;
        self.tool = Tool::Survey;
        self.target = Target::Any;
        self.band = None;
    }

    pub fn show(&mut self, ui: &mut egui::Ui, env: &Env) {
        if self.view != *env.settings {
            self.view = env.settings.clone();
            self.heat_dirty = true;
        }
        let roles = theme::roles(ui.ctx());
        let tools = egui::Frame::new().fill(roles.surface).inner_margin(egui::Margin::symmetric(16, 10));
        let side = egui::Frame::new().fill(roles.surface).inner_margin(16);
        let canvas = egui::Frame::new().fill(roles.surface_container_lowest);
        egui::Panel::top("floor_tools").frame(tools).show(ui, |ui| self.toolbar(ui));
        if !matches!(self.tool, Tool::Walls | Tool::Select) {
            egui::Panel::right("floor_points").default_size(320.0).frame(side).show(ui, |ui| self.side_panel(ui));
        }
        egui::CentralPanel::default().frame(canvas).show(ui, |ui| self.canvas(ui, env));
        self.show_dialogs(ui.ctx());
    }

    /// The plan's scale, for the status bar: how it is set, or why it looks wrong.
    pub fn scale_label(&self, ui: &mut egui::Ui) {
        let Some(f) = self.project.as_ref().map(|p| &p.floor) else { return };
        let error = theme::roles(ui.ctx()).error;
        let (text, colour, hover) = match f.metres_per_px {
            Some(m) if !heatmap::scale_is_plausible(m, f.width_px, f.height_px) => (
                "Scale looks wrong".to_string(),
                Some(error),
                Some(format!(
                    "The scale makes this plan {:.0} m across. Choose Set scale and click two points a known distance apart to redo it.",
                    f.width_px.max(f.height_px) as f32 * m
                )),
            ),
            Some(m) => (format!("Scale: {:.1} px/m", 1.0 / m), None, None),
            None => ("Scale not set".to_string(), Some(Color32::from_rgb(240, 140, 50)), None),
        };
        let mut rich = egui::RichText::new(text);
        if let Some(c) = colour {
            rich = rich.color(c);
        }
        let label = ui.add(egui::Label::new(rich).wrap_mode(egui::TextWrapMode::Extend));
        if let Some(h) = hover {
            label.on_hover_text(h);
        }
    }

    /// The scale and PDF page dialogs, shown whichever view is up.
    pub fn show_dialogs(&mut self, ctx: &egui::Context) {
        self.scale_dialog(ctx);
        self.pdf_dialog(ctx);
        self.report_dialog(ctx);
    }

    /// Ask for a floor plan file (PNG, JPG or PDF) and load it.
    pub fn pick_plan(&mut self, ctx: &egui::Context) {
        if self.confirm_discard()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("Floor plans (PNG, JPG, PDF)", &["png", "jpg", "jpeg", "pdf"])
                .set_title("Open floor plan")
                .pick_file()
        {
            self.load_plan(ctx, &path);
        }
    }

    /// Ask for a saved project file and open it.
    pub fn pick_project(&mut self, ctx: &egui::Context) {
        if self.confirm_discard()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("WiFi Spanner project", &[store::EXTENSION])
                .set_directory(store::projects_dir())
                .pick_file()
        {
            self.open_project(ctx, path);
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        // Fields in the bars are as tall as the buttons for the chosen density.
        ui.spacing_mut().interact_size.y = theme::density(&ctx).button_height();
        ui.horizontal_wrapped(|ui| {
            if theme::button(ui, ButtonKind::Text, Some(ICON_ARROW_BACK), "Projects").clicked() {
                self.back = true;
            }
            ui.separator();
            if theme::button(ui, ButtonKind::Tonal, Some(ICON_IMAGE), "Open floor plan").clicked() {
                self.pick_plan(&ctx);
            }
            if theme::button(ui, ButtonKind::Tonal, Some(ICON_FOLDER_OPEN), "Open project").clicked() {
                self.pick_project(&ctx);
            }
            ui.add_enabled_ui(self.project.is_some(), |ui| {
                if theme::button(ui, ButtonKind::Tonal, Some(ICON_SAVE), "Save").clicked() {
                    match self.path.clone() {
                        Some(p) => self.save_to(&p),
                        None => self.save_as(),
                    }
                }
                if theme::button(ui, ButtonKind::Text, None, "Save as...").clicked() {
                    self.save_as();
                }
                let has_points = self.project.as_ref().is_some_and(|p| !p.floor.points.is_empty());
                let report = ui.add_enabled_ui(has_points, |ui| theme::button(ui, ButtonKind::Filled, Some(ICON_DESCRIPTION), "Generate report")).inner;
                if report.clicked() {
                    self.make_report();
                }
            });
            ui.separator();
            ui.add_enabled_ui(self.project.is_some(), |ui| {
                if theme::chip(ui, self.tool == Tool::Survey, Some(ICON_PLACE), "Survey point").clicked() {
                    self.tool = Tool::Survey;
                }
                if theme::chip(ui, self.tool == Tool::Scale, Some(ICON_STRAIGHTEN), "Set scale").clicked() {
                    self.tool = Tool::Scale;
                    self.scale_clicks.clear();
                }
                if theme::chip(ui, self.tool == Tool::Walls, Some(ICON_ARCHITECTURE), "Draw walls").clicked() {
                    self.tool = Tool::Walls;
                    self.wall_start = None;
                    self.selected_walls.clear();
                }
                if theme::chip(ui, self.tool == Tool::Select, Some(ICON_HIGHLIGHT_ALT), "Select walls").clicked() {
                    self.tool = Tool::Select;
                    self.wall_start = None;
                }
            });
        });

        let Some(floor) = self.project.as_ref().map(|p| &p.floor) else { return };
        ui.horizontal_wrapped(|ui| {
            let mut changed = theme::switch_row(ui, &mut self.show_heat, "Heatmap").changed();
            ui.add_enabled_ui(self.show_heat, |ui| {
                ui.label("Show");
                let label = match &self.target {
                    Target::Any => "Strongest of any AP".to_string(),
                    Target::Ssid(s) => format!("SSID: {s}"),
                    Target::Bssid(b) => format!("AP: {b}"),
                };
                egui::ComboBox::from_id_salt("heat_target").selected_text(label).width(220.0).show_ui(ui, |ui| {
                    changed |= ui.selectable_value(&mut self.target, Target::Any, "Strongest of any AP").changed();
                    for s in floor.ssids() {
                        changed |= ui.selectable_value(&mut self.target, Target::Ssid(s.clone()), format!("SSID: {s}")).changed();
                    }
                    for b in floor.bssids() {
                        changed |= ui.selectable_value(&mut self.target, Target::Bssid(b.clone()), format!("AP: {b}")).changed();
                    }
                });
                egui::ComboBox::from_id_salt("heat_band")
                    .selected_text(self.band.map_or("All bands".to_string(), |b| b.to_string()))
                    .show_ui(ui, |ui| {
                        changed |= ui.selectable_value(&mut self.band, None, "All bands").changed();
                        for b in [Band::Ghz2_4, Band::Ghz5, Band::Ghz6] {
                            changed |= ui.selectable_value(&mut self.band, Some(b), b.to_string()).changed();
                        }
                    });
                ui.spacing_mut().slider_width = 100.0;
                ui.add(egui::Slider::new(&mut self.opacity, 0.1..=1.0).text("Opacity"));
                if floor.metres_per_px.is_some() {
                    changed |= ui.add(egui::Slider::new(&mut self.fade_m, 2.0..=30.0).text("Fade (m)")).changed();
                }
            });
            if changed {
                self.heat_dirty = true;
            }
        });
    }

    /// Remember the walls so that undo can bring them back. Edits that arrive
    /// as a stream (dragging a height) share one step.
    fn remember_walls(&mut self, streaming: bool) {
        let Some(p) = self.project.as_ref() else { return };
        let now = std::time::Instant::now();
        let recent = self.last_wall_edit.is_some_and(|t| now.duration_since(t) < std::time::Duration::from_millis(600));
        self.last_wall_edit = Some(now);
        if streaming && recent {
            return;
        }
        self.wall_history.push(p.floor.walls.clone());
        if self.wall_history.len() > WALL_HISTORY {
            self.wall_history.remove(0);
        }
    }

    fn undo_walls(&mut self) {
        if let (Some(walls), Some(p)) = (self.wall_history.pop(), self.project.as_mut()) {
            p.floor.walls = walls;
            self.selected_walls.clear();
            self.wall_start = None;
            self.autosave();
        }
    }

    fn delete_selected_walls(&mut self) {
        if self.selected_walls.is_empty() {
            return;
        }
        self.remember_walls(false);
        let selected = std::mem::take(&mut self.selected_walls);
        if let Some(p) = self.project.as_mut() {
            let mut i = 0;
            p.floor.walls.retain(|_| {
                i += 1;
                !selected.contains(&(i - 1))
            });
        }
        self.autosave();
    }

    /// The walls the bubble's controls act on: the selection, or the next wall to be drawn.
    fn apply_wall_edit(&mut self, edit: WallEdit) {
        match edit {
            WallEdit::Tool(t) => {
                self.tool = t;
                self.wall_start = None;
                if t == Tool::Walls {
                    self.selected_walls.clear();
                }
            }
            WallEdit::Undo => self.undo_walls(),
            WallEdit::Delete => self.delete_selected_walls(),
            WallEdit::Ceiling(c) => {
                self.remember_walls(true);
                if let Some(p) = self.project.as_mut() {
                    p.floor.ceiling_m = c;
                    for w in &mut p.floor.walls {
                        if let Some(h) = &mut w.height_m {
                            *h = h.min(c);
                        }
                    }
                }
                self.autosave();
            }
            WallEdit::Material(m) => {
                self.wall_material = m;
                self.wall_height = m.default_height_m();
                if self.tool == Tool::Select && !self.selected_walls.is_empty() {
                    self.remember_walls(false);
                    let selected = self.selected_walls.clone();
                    if let Some(p) = self.project.as_mut() {
                        for i in selected {
                            if let Some(w) = p.floor.walls.get_mut(i) {
                                w.material = m;
                                w.height_m = m.default_height_m();
                            }
                        }
                    }
                    self.autosave();
                }
            }
            WallEdit::Height(h) => {
                if self.tool == Tool::Select && !self.selected_walls.is_empty() {
                    self.remember_walls(true);
                    let selected = self.selected_walls.clone();
                    if let Some(p) = self.project.as_mut() {
                        for i in selected {
                            if let Some(w) = p.floor.walls.get_mut(i) {
                                w.height_m = h;
                            }
                        }
                    }
                    self.autosave();
                } else {
                    self.wall_height = h;
                }
            }
        }
    }

    /// The floating palette over the right of the plan: tool, material,
    /// height and the edit actions. Returns what was asked for.
    fn wall_bubble(&self, ctx: &egui::Context, canvas: Rect) -> Vec<WallEdit> {
        let r = theme::roles(ctx);
        let Some(floor) = self.project.as_ref().map(|p| &p.floor) else { return Vec::new() };
        let selecting = self.tool == Tool::Select;
        let picked: Vec<&Wall> = self.selected_walls.iter().filter_map(|&i| floor.walls.get(i)).collect();
        // What the controls show: the selection when selecting, else the next wall.
        let (material, height) = if selecting {
            let first = picked.first();
            let same_material = first.filter(|f| picked.iter().all(|w| w.material == f.material)).map(|f| f.material);
            let same_height = first.filter(|f| picked.iter().all(|w| w.height_m == f.height_m)).map(|f| f.height_m);
            (same_material, same_height)
        } else {
            (Some(self.wall_material), Some(self.wall_height))
        };
        let ceiling = floor.ceiling_m;
        let mut edits = Vec::new();
        egui::Area::new(egui::Id::unique("wall_bubble"))
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::RIGHT_TOP)
            .fixed_pos(canvas.right_top() + Vec2::new(-12.0, 12.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(r.surface_container_high)
                    .stroke(Stroke::new(1.0, r.outline_variant))
                    .corner_radius(20)
                    .inner_margin(14)
                    .shadow(theme::shadow(r.shadow, 16, 0.18))
                    .show(ui, |ui| {
                        ui.set_width(230.0);
                        ui.horizontal(|ui| {
                            if theme::chip(ui, !selecting, Some(ICON_ARCHITECTURE), "Draw").clicked() {
                                edits.push(WallEdit::Tool(Tool::Walls));
                            }
                            if theme::chip(ui, selecting, Some(ICON_HIGHLIGHT_ALT), "Select").clicked() {
                                edits.push(WallEdit::Tool(Tool::Select));
                            }
                        });
                        ui.add_space(6.0);
                        let hint = if selecting {
                            match picked.len() {
                                0 => "Click a wall, or drag a box over several. Shift or Ctrl adds to the selection.".to_string(),
                                1 => "1 wall selected".to_string(),
                                n => format!("{n} walls selected"),
                            }
                        } else {
                            "Click the start of a wall, then its end.".to_string()
                        };
                        ui.label(egui::RichText::new(hint).size(12.5).color(r.on_surface_variant));
                        ui.add_space(6.0);
                        let editable = !selecting || !picked.is_empty();
                        ui.add_enabled_ui(editable, |ui| {
                            for m in Material::ALL {
                                let ([cr, cg, cb], _) = m.style();
                                let on = material == Some(m);
                                ui.horizontal(|ui| {
                                    let (swatch, _) = ui.allocate_exact_size(Vec2::new(18.0, 18.0), Sense::hover());
                                    ui.painter().rect_filled(swatch.shrink2(Vec2::new(0.0, 6.0)), 3.0, Color32::from_rgb(cr, cg, cb));
                                    if ui.add_sized([190.0, 26.0], egui::Button::selectable(on, m.name()).frame_when_inactive(false)).clicked() {
                                        edits.push(WallEdit::Material(m));
                                    }
                                });
                            }
                            ui.add_space(6.0);
                            let mut full = height.is_none_or(|h| h.is_none());
                            let mixed = height.is_none();
                            ui.horizontal(|ui| {
                                if theme::switch_row(ui, &mut full, "Full height").changed() {
                                    edits.push(WallEdit::Height(if full { None } else { Some(ceiling) }));
                                }
                            });
                            let mut metres = height.flatten().unwrap_or(ceiling);
                            ui.horizontal(|ui| {
                                ui.label("Height");
                                let changed = ui
                                    .add_enabled(!full, egui::DragValue::new(&mut metres).range(0.1..=ceiling.max(0.2)).speed(0.05).suffix(" m").fixed_decimals(2))
                                    .changed();
                                if changed {
                                    edits.push(WallEdit::Height(Some(metres.min(ceiling))));
                                }
                                if mixed && selecting {
                                    ui.label(egui::RichText::new("(mixed)").size(12.0).color(r.on_surface_variant));
                                }
                            });
                        });
                        ui.horizontal(|ui| {
                            ui.label("Ceiling");
                            let mut c = ceiling;
                            if ui.add(egui::DragValue::new(&mut c).range(1.0..=30.0).speed(0.05).suffix(" m").fixed_decimals(2)).changed() {
                                edits.push(WallEdit::Ceiling(c));
                            }
                        });
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if ui.add_enabled_ui(!self.wall_history.is_empty(), |ui| theme::button(ui, ButtonKind::Text, Some(ICON_UNDO), "Undo")).inner.clicked() {
                                edits.push(WallEdit::Undo);
                            }
                            if selecting
                                && ui.add_enabled_ui(!picked.is_empty(), |ui| theme::button(ui, ButtonKind::Outlined, Some(ICON_DELETE), "Delete")).inner.clicked()
                            {
                                edits.push(WallEdit::Delete);
                            }
                        });
                    });
            });
        edits
    }

    /// Where a wall point at `plan` lands once snapped.
    fn snapped(&self, plan: Pos2, zoom: f32) -> Pos2 {
        let walls = self.project.as_ref().map_or(&[][..], |p| &p.floor.walls[..]);
        let s = walls::snap([plan.x, plan.y], self.wall_start.map(|p| [p.x, p.y]), walls, WALL_SNAP_PX / zoom);
        Pos2::new(s[0], s[1])
    }

    /// The first click sets the start of a wall, the second its end; the
    /// next click starts another. The new wall replaces whatever stretch of an
    /// older wall it lies along, so a door drawn over a wall opens a gap.
    fn wall_click(&mut self, plan: Pos2, zoom: f32) {
        let at = self.snapped(plan, zoom);
        let Some(start) = self.wall_start else {
            self.wall_start = Some(at);
            return;
        };
        if start.distance(at) < 1.0 {
            return;
        }
        self.remember_walls(false);
        let mut wall = Wall::new([start.x, start.y], [at.x, at.y], self.wall_material);
        wall.height_m = self.wall_height;
        if let Some(p) = self.project.as_mut() {
            let tol = WALL_CARVE_PX / zoom;
            p.floor.walls = p.floor.walls.iter().flat_map(|w| w.carve(&wall, tol)).collect();
            p.floor.walls.push(wall);
        }
        self.wall_start = None;
        self.autosave();
    }

    /// Select by clicking a wall or dragging a box; Shift or Ctrl adds.
    fn select_walls(&mut self, resp: &egui::Response, ui: &egui::Ui, zoom: f32, to_plan: &impl Fn(Pos2) -> Pos2) {
        let additive = ui.input(|i| i.modifiers.shift || i.modifiers.command);
        let Some(floor) = self.project.as_ref().map(|p| &p.floor) else { return };
        if resp.drag_started_by(egui::PointerButton::Primary) {
            self.box_start = ui.input(|i| i.pointer.press_origin()).map(to_plan);
        }
        if resp.drag_stopped_by(egui::PointerButton::Primary)
            && let (Some(a), Some(b)) = (self.box_start.take(), resp.interact_pointer_pos().map(to_plan))
        {
            let (min, max) = ([a.x.min(b.x), a.y.min(b.y)], [a.x.max(b.x), a.y.max(b.y)]);
            let hit = floor.walls.iter().enumerate().filter(|(_, w)| w.intersects_rect(min, max)).map(|(i, _)| i);
            if !additive {
                self.selected_walls.clear();
            }
            self.selected_walls.extend(hit);
        } else if resp.clicked()
            && let Some(p) = resp.interact_pointer_pos().map(to_plan)
        {
            let radius = WALL_PICK_PX / zoom;
            let nearest = floor
                .walls
                .iter()
                .enumerate()
                .map(|(i, w)| (i, w.distance_to([p.x, p.y])))
                .filter(|(_, d)| *d <= radius)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i);
            match (nearest, additive) {
                (Some(i), true) => {
                    if !self.selected_walls.remove(&i) {
                        self.selected_walls.insert(i);
                    }
                }
                (Some(i), false) => self.selected_walls = BTreeSet::from([i]),
                (None, false) => self.selected_walls.clear(),
                (None, true) => {}
            }
        }
    }

    /// Save the project, then open the dialog that chooses what the report contains.
    fn make_report(&mut self) {
        if self.path.is_none() {
            self.save_as();
        }
        let Some(path) = self.path.clone() else { return };
        self.save_to(&path);
        let Some(project) = &self.project else { return };
        self.report_dialog = Some(report::OptionsDialog::new(path, project.clone()));
    }

    fn report_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.report_dialog.take() else { return };
        match dialog.show(ctx) {
            report::DialogResult::Open => self.report_dialog = Some(dialog),
            report::DialogResult::Cancel => {}
            report::DialogResult::Generate(opts) => {
                match report::write_report(dialog.path(), dialog.project(), &report::ReportStyle::from_settings(&self.view), &opts) {
                    Ok(file) => {
                        self.status = format!("Report saved: {}", file.display());
                        // Show the folder with the new report highlighted rather than opening it in a browser.
                        if let Err(e) = store::reveal(&file) {
                            self.status.push_str(&format!(" ({e})"));
                        }
                    }
                    Err(e) => self.status = format!("Could not create report: {e}"),
                }
            }
        }
    }

    fn save_as(&mut self) {
        let name = self.project.as_ref().map_or("survey".into(), |p| p.floor.name.clone());
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("WiFi Spanner project", &[store::EXTENSION])
            .set_directory(store::projects_dir())
            .set_file_name(format!("{name}.wifispanner"))
            .save_file()
        {
            self.save_to(&path);
            if self.path.is_some() {
                self.status = format!("Saved to {}", path.display());
            }
        }
    }

    /// Ask before replacing a project that has measurements and no save file.
    fn confirm_discard(&self) -> bool {
        let unsaved = self.path.is_none() && self.project.as_ref().is_some_and(|p| !p.floor.points.is_empty());
        !unsaved
            || rfd::MessageDialog::new()
                .set_title("Discard survey?")
                .set_description("The current survey has not been saved. Discard its measurements?")
                .set_buttons(rfd::MessageButtons::YesNo)
                .show()
                == rfd::MessageDialogResult::Yes
    }

    fn side_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Survey points");
        let Some(floor) = self.project.as_mut().map(|p| &mut p.floor) else {
            ui.label("No floor plan loaded.");
            return;
        };
        ui.label(&self.status);
        ui.separator();
        let mut delete = None;
        egui::ScrollArea::vertical().id_salt("points").max_height(ui.available_height() * 0.45).show(ui, |ui| {
            for (i, p) in floor.points.iter().enumerate() {
                let best = Floor::signal_at(p, &Target::Any, None);
                let text = format!("#{}   {} APs   best {:.0} dBm", i + 1, p.observations.len(), best);
                if ui.selectable_label(self.selected == Some(i), text).clicked() {
                    self.selected = Some(i);
                }
            }
        });
        if let Some(i) = self.selected.filter(|&i| i < floor.points.len()) {
            ui.separator();
            ui.horizontal(|ui| {
                ui.strong(format!("Point #{}", i + 1));
                if theme::button(ui, ButtonKind::Outlined, Some(ICON_DELETE), "Delete").clicked() {
                    delete = Some(i);
                }
            });
            let p = &floor.points[i];
            ui.label(format!("{} s dwell", p.dwell_secs));
            let mut obs: Vec<_> = p.observations.iter().collect();
            obs.sort_by(|a, b| b.signal_dbm.total_cmp(&a.signal_dbm));
            egui::ScrollArea::vertical().id_salt("obs").show(ui, |ui| {
                egui::Grid::new("obs_grid").striped(true).spacing([10.0, 3.0]).show(ui, |ui| {
                    for h in ["SSID", "Ch", "Band", "dBm", "Seen"] {
                        ui.strong(h);
                    }
                    ui.end_row();
                    for o in obs {
                        ui.label(o.ssid.as_deref().unwrap_or("<hidden>"));
                        ui.label(o.channel.to_string());
                        ui.label(o.band().to_string());
                        ui.colored_label(signal_color(o.signal_dbm, ui.visuals().dark_mode), format!("{:.0}", o.signal_dbm));
                        ui.label(o.samples.to_string());
                        ui.end_row();
                    }
                });
            });
        }
        if let Some(i) = delete {
            floor.points.remove(i);
            self.selected = None;
            self.heat_dirty = true;
            self.autosave();
        }
    }

    fn canvas(&mut self, ui: &mut egui::Ui, env: &Env) {
        let (Some(project), Some(tex)) = (self.project.as_ref(), self.texture.clone()) else {
            let ctx = ui.ctx().clone();
            theme::empty_state(ui, ICON_IMAGE, "No floor plan yet", "Open a floor plan or a saved project to start surveying.", |ui| {
                if theme::button(ui, ButtonKind::Filled, Some(ICON_IMAGE), "Open floor plan").clicked() {
                    self.pick_plan(&ctx);
                }
                ui.add_space(8.0);
                if theme::button(ui, ButtonKind::Tonal, Some(ICON_FOLDER_OPEN), "Open project").clicked() {
                    self.pick_project(&ctx);
                }
            });
            return;
        };
        let (img_w, img_h) = (project.floor.width_px as f32, project.floor.height_px as f32);
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());

        if self.needs_fit && rect.width() > 1.0 {
            self.zoom = (rect.width() / img_w).min(rect.height() / img_h) * 0.95;
            self.offset = (rect.size() - Vec2::new(img_w, img_h) * self.zoom) / 2.0;
            self.needs_fit = false;
        }
        // Selecting by box uses the primary drag, so the plan pans with the
        // middle or right button then.
        let panning = if self.tool == Tool::Select {
            resp.dragged_by(egui::PointerButton::Middle) || resp.dragged_by(egui::PointerButton::Secondary)
        } else {
            resp.dragged()
        };
        if panning {
            self.offset += resp.drag_delta();
        }
        if let Some(cursor) = resp.hover_pos() {
            let factor = ui.input(|i| (i.smooth_scroll_delta.y * 0.0015).exp() * i.zoom_delta());
            if (factor - 1.0).abs() > 1e-4 {
                let anchor = (cursor - rect.min - self.offset) / self.zoom;
                self.zoom = (self.zoom * factor).clamp(0.02, 50.0);
                self.offset = cursor - rect.min - anchor * self.zoom;
            }
        }
        let (offset, zoom) = (self.offset, self.zoom);
        let to_screen = |p: Pos2| rect.min + offset + p.to_vec2() * zoom;
        let to_plan = |s: Pos2| ((s - rect.min - offset) / zoom).to_pos2();

        if matches!(self.tool, Tool::Walls | Tool::Select) {
            let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
            if self.tool == Tool::Walls {
                if resp.secondary_clicked() || escape {
                    self.wall_start = None;
                } else if resp.clicked()
                    && let Some(plan) = resp.interact_pointer_pos().map(to_plan).filter(|p| p.x >= 0.0 && p.y >= 0.0 && p.x <= img_w && p.y <= img_h)
                {
                    self.wall_click(plan, zoom);
                }
            } else {
                self.select_walls(&resp, ui, zoom, &to_plan);
                if escape {
                    self.selected_walls.clear();
                }
                if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::A))
                    && let Some(p) = &self.project
                {
                    self.selected_walls = (0..p.floor.walls.len()).collect();
                }
            }
            if ui.input(|i| i.key_pressed(egui::Key::Delete)) {
                self.delete_selected_walls();
            }
            if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Z)) {
                self.undo_walls();
            }
        } else if resp.clicked()
            && let Some(click) = resp.interact_pointer_pos()
        {
            self.handle_click(click, to_plan(click), &to_screen, env);
        }

        if self.show_heat && self.heat_dirty {
            self.rebuild_heat(ui.ctx());
        }

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
        let plan_rect = Rect::from_min_max(to_screen(Pos2::ZERO), to_screen(Pos2::new(img_w, img_h)));
        let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        if self.view.show_plan {
            painter.image(tex.id(), plan_rect, uv, Color32::WHITE);
        } else {
            painter.rect_filled(plan_rect, 0.0, ui.visuals().faint_bg_color);
        }
        if self.show_heat
            && let Some(heat) = &self.heat_texture
        {
            painter.image(heat.id(), plan_rect, uv, Color32::from_white_alpha((self.opacity * 255.0) as u8));
        }

        let floor = &self.project.as_ref().expect("checked above").floor;
        let ceiling = floor.ceiling_m;
        for (i, w) in floor.walls.iter().enumerate() {
            let ([r, g, b], width) = w.material.style();
            let (a, c) = (to_screen(Pos2::new(w.a[0], w.a[1])), to_screen(Pos2::new(w.b[0], w.b[1])));
            if self.selected_walls.contains(&i) {
                painter.line_segment([a, c], Stroke::new(width + 6.0, Color32::from_rgba_unmultiplied(255, 220, 0, 140)));
            }
            // Partial-height walls are dashed so that they read as "low".
            let colour = Color32::from_rgb(r, g, b);
            if w.effective_height_m(ceiling) < ceiling - 0.01 {
                painter.extend(egui::Shape::dashed_line(&[a, c], Stroke::new(width, colour), 9.0, 5.0));
            } else {
                painter.line_segment([a, c], Stroke::new(width, colour));
            }
        }
        if self.tool == Tool::Walls
            && let (Some(start), Some(hover)) = (self.wall_start, ui.input(|i| i.pointer.hover_pos()).filter(|p| rect.contains(*p)))
        {
            let end = self.snapped(to_plan(hover), zoom);
            let ([r, g, b], width) = self.wall_material.style();
            let (a, c) = (to_screen(start), to_screen(end));
            painter.line_segment([a, c], Stroke::new(width, Color32::from_rgb(r, g, b).gamma_multiply(0.7)));
            painter.circle_stroke(c, 6.0, Stroke::new(2.0, Color32::from_rgb(240, 140, 50)));
            let len_px = start.distance(end);
            let label = match floor.metres_per_px {
                Some(m) => format!("{:.2} m", len_px * m),
                None => format!("{len_px:.0} px"),
            };
            painter.text((a + c.to_vec2()) / 2.0 + Vec2::new(8.0, -8.0), egui::Align2::LEFT_BOTTOM, label, egui::FontId::proportional(14.0), Color32::from_rgb(240, 140, 50));
            ui.ctx().request_repaint();
        }
        if self.tool == Tool::Select
            && let (Some(a), Some(b)) = (self.box_start, ui.input(|i| i.pointer.hover_pos()))
            && resp.dragged_by(egui::PointerButton::Primary)
        {
            let boxed = Rect::from_two_pos(to_screen(a), b);
            let accent = theme::roles(ui.ctx()).primary;
            painter.rect(boxed, 2.0, accent.gamma_multiply(0.15), Stroke::new(1.5, accent), egui::StrokeKind::Inside);
        }
        if self.view.show_path {
            let path: Vec<Pos2> = floor.points.iter().map(|p| to_screen(Pos2::new(p.x, p.y))).collect();
            painter.add(egui::Shape::line(path, Stroke::new(2.0, Color32::from_rgb(30, 90, 200).gamma_multiply(0.7))));
        }
        let radius = self.view.marker_size.radius();
        for (i, p) in floor.points.iter().enumerate() {
            let c = to_screen(Pos2::new(p.x, p.y));
            let selected = self.selected == Some(i);
            painter.circle(c, radius + if selected { 2.0 } else { 0.0 }, Color32::from_rgb(30, 90, 200), Stroke::new(2.0, if selected { Color32::YELLOW } else { Color32::WHITE }));
            if self.view.show_numbers {
                painter.text(c + Vec2::new(radius + 3.0, -radius - 3.0), egui::Align2::LEFT_BOTTOM, (i + 1).to_string(), egui::FontId::proportional(13.0), Color32::BLACK);
            }
        }
        if let Some(p) = self.pending {
            let c = to_screen(p);
            painter.circle_stroke(c, 12.0, Stroke::new(3.0, Color32::from_rgb(240, 140, 50)));
            painter.text(
                rect.center_top() + Vec2::new(0.0, 14.0),
                egui::Align2::CENTER_TOP,
                format!("Scanning for {} s... ({} scans so far). Stand still.", env.dwell_secs, env.scans_so_far),
                egui::FontId::proportional(18.0),
                Color32::from_rgb(240, 140, 50),
            );
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
        }
        for (i, &p) in self.scale_clicks.iter().enumerate() {
            let c = to_screen(p);
            painter.circle(c, 5.0, Color32::from_rgb(220, 50, 50), Stroke::new(1.5, Color32::WHITE));
            if i == 1 {
                painter.line_segment([to_screen(self.scale_clicks[0]), c], Stroke::new(2.0, Color32::from_rgb(220, 50, 50)));
            }
        }
        if self.show_heat {
            draw_legend(&painter, rect, &self.view);
        }
        if matches!(self.tool, Tool::Walls | Tool::Select) {
            for edit in self.wall_bubble(ui.ctx(), rect) {
                self.apply_wall_edit(edit);
            }
        }
    }

    fn handle_click(&mut self, click: Pos2, plan: Pos2, to_screen: &impl Fn(Pos2) -> Pos2, env: &Env) {
        let Some(floor) = self.project.as_ref().map(|p| &p.floor) else { return };
        if plan.x < 0.0 || plan.y < 0.0 || plan.x > floor.width_px as f32 || plan.y > floor.height_px as f32 {
            return;
        }
        match self.tool {
            Tool::Walls | Tool::Select => {}
            Tool::Scale => {
                if self.scale_clicks.len() >= 2 {
                    self.scale_clicks.clear();
                }
                if let Some(first) = self.scale_clicks.first()
                    && first.distance(plan) < MIN_SCALE_PX
                {
                    self.status = "Click a second point further from the first one (at least the other end of a known distance).".into();
                    return;
                }
                self.scale_clicks.push(plan);
                if self.scale_clicks.len() == 2 {
                    self.scale_input.clear();
                }
            }
            Tool::Survey => {
                let near = floor
                    .points
                    .iter()
                    .position(|p| to_screen(Pos2::new(p.x, p.y)).distance(click) <= PICK_RADIUS);
                if near.is_some() {
                    self.selected = near;
                } else if self.pending.is_none() {
                    self.pending = Some(plan);
                    let _ = env.tx.send(Cmd::Survey { x: plan.x, y: plan.y });
                }
            }
        }
    }

    fn scale_dialog(&mut self, ctx: &egui::Context) {
        if self.scale_clicks.len() != 2 {
            return;
        }
        let (a, b) = (self.scale_clicks[0], self.scale_clicks[1]);
        let (w, h) = self.project.as_ref().map_or((1, 1), |p| (p.floor.width_px, p.floor.height_px));
        let mut done = false;
        theme::dialog(ctx, "Set scale", 380.0, |ui| {
            ui.label(format!("The two points are {:.0} px apart. What is that distance in metres?", a.distance(b)));
            ui.add_space(8.0);
            let edit = ui.add(egui::TextEdit::singleline(&mut self.scale_input).hint_text("Distance in metres").desired_width(f32::INFINITY));
            edit.request_focus();
            let metres = self.scale_input.trim().replace(',', ".").parse::<f32>().ok().filter(|m| m.is_finite() && *m > 0.0);
            // Typing nothing yet is not an error; a distance that cannot be right is.
            let check = match metres {
                Some(m) => Some(check_scale(a.distance(b), m, w, h)),
                None if a.distance(b) < MIN_SCALE_PX => Some(check_scale(a.distance(b), 1.0, w, h)),
                None => None,
            };
            if let Some(Err(why)) = &check {
                ui.add_space(6.0);
                ui.colored_label(theme::roles(ui.ctx()).error, why);
            }
            let scale = check.and_then(Result::ok);
            theme::actions(ui, |ui| {
                let set = ui.add_enabled_ui(scale.is_some(), |ui| theme::button(ui, ButtonKind::Filled, None, "Set scale")).inner;
                let ok = set.clicked() || (edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && scale.is_some());
                if ok && let (Some(mpp), Some(p)) = (scale, self.project.as_mut()) {
                    p.floor.metres_per_px = Some(mpp);
                    self.status = "Scale set. Switch to \"Survey point\" and click where you are standing.".into();
                    self.tool = Tool::Survey;
                    self.heat_dirty = true;
                    done = true;
                }
                if theme::button(ui, ButtonKind::Text, None, "Cancel").clicked() {
                    done = true;
                }
            });
        });
        if done {
            self.scale_clicks.clear();
            self.autosave();
        }
    }

    fn rebuild_heat(&mut self, ctx: &egui::Context) {
        self.heat_dirty = false;
        let Some(floor) = self.project.as_ref().map(|p| &p.floor) else { return };
        let samples: Vec<Sample> = floor
            .points
            .iter()
            .map(|p| Sample { x: p.x, y: p.y, dbm: Floor::signal_at_min(p, &self.target, self.band, self.view.detect_dbm) })
            .collect();
        let fade_px = heatmap::fade_px(self.fade_m, floor.metres_per_px, floor.width_px, floor.height_px);
        let grid = heatmap::interpolate(&samples, floor.width_px, floor.height_px, HEAT_CELLS, fade_px);
        let pixels = (0..grid.w * grid.h)
            .map(|i| {
                let v = &self.view;
                let [r, g, b] = heatmap::color_in(v.color_scheme, grid.dbm[i] as f64, v.min_dbm, v.max_dbm);
                let conf = grid.confidence[i];
                if !v.gray_unmeasured {
                    return Color32::from_rgba_unmultiplied(r, g, b, (conf * 255.0) as u8);
                }
                // Colour at `conf` opacity over a half-opaque grey wash.
                let (grey, grey_a) = (120.0, 0.5);
                let a = conf + grey_a * (1.0 - conf);
                let mix = |c: u8| ((c as f32 * conf + grey * grey_a * (1.0 - conf)) / a).round() as u8;
                Color32::from_rgba_unmultiplied(mix(r), mix(g), mix(b), (a * 255.0) as u8)
            })
            .collect();
        let image = egui::ColorImage { size: [grid.w, grid.h], source_size: Vec2::new(grid.w as f32, grid.h as f32), pixels };
        self.heat_texture = Some(ctx.load_texture("heatmap", image, TextureOptions::LINEAR));
    }
}

fn decode_texture(ctx: &egui::Context, bytes: &[u8], name: &str) -> Option<TextureHandle> {
    let mut img = image::load_from_memory(bytes).ok()?;
    if img.width().max(img.height()) > MAX_TEXTURE_SIDE {
        img = img.resize(MAX_TEXTURE_SIDE, MAX_TEXTURE_SIDE, image::imageops::FilterType::Triangle);
    }
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Some(ctx.load_texture(name, egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()), TextureOptions::LINEAR))
}

fn draw_legend(painter: &egui::Painter, rect: Rect, view: &Settings) {
    let (w, h, steps) = (220.0, 14.0, 44);
    let origin = rect.left_bottom() + Vec2::new(14.0, -38.0);
    for i in 0..steps {
        let t = i as f64 / (steps - 1) as f64;
        let [r, g, b] = heatmap::color_in(view.color_scheme, view.min_dbm + t * (view.max_dbm - view.min_dbm), view.min_dbm, view.max_dbm);
        let x = origin.x + w * i as f32 / steps as f32;
        painter.rect_filled(Rect::from_min_size(Pos2::new(x, origin.y), Vec2::new(w / steps as f32 + 1.0, h)), 0.0, Color32::from_rgb(r, g, b));
    }
    let font = egui::FontId::proportional(12.0);
    painter.text(origin + Vec2::new(0.0, h + 2.0), egui::Align2::LEFT_TOP, format!("{:.0} dBm", view.min_dbm), font.clone(), Color32::WHITE);
    painter.text(origin + Vec2::new(w, h + 2.0), egui::Align2::RIGHT_TOP, format!("{:.0}+", view.max_dbm), font, Color32::WHITE);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_checks_reject_close_points_and_absurd_plan_sizes() {
        assert!(check_scale(1.0, 13.0, 2828, 4000).unwrap_err().contains("only 1 px apart"));
        let mpp = check_scale(190.0, 10.0, 2828, 4000).unwrap();
        assert!((mpp - 0.0526).abs() < 0.001);
        assert!(check_scale(100.0, 100_000.0, 2828, 4000).unwrap_err().contains("looks wrong"));
        assert!(check_scale(100.0, 0.001, 2828, 4000).unwrap_err().contains("looks wrong"));
    }
}
