// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 rubble-za. See the LICENSE and NOTICE files.

//! Projects tab: previous surveys and their reports.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use eframe::egui;
use wifispanner_core::survey::Project;

use crate::settings::Settings;
use crate::theme::{self, ButtonKind};
use egui_material_icons::icons::{ICON_DESCRIPTION, ICON_FOLDER, ICON_FOLDER_OPEN, ICON_IMAGE, ICON_OPEN_IN_NEW, ICON_REFRESH};
use crate::{report, store};

#[derive(Clone)]
struct Summary {
    path: PathBuf,
    name: String,
    modified: Option<SystemTime>,
    points: usize,
    aps: usize,
    scale_set: bool,
    first_scan: Option<u64>,
    last_scan: Option<u64>,
    error: Option<String>,
}

impl Summary {
    fn read(path: &Path, modified: Option<SystemTime>) -> Summary {
        let stem = path.file_stem().map_or("project".into(), |s| s.to_string_lossy().into_owned());
        let mut s = Summary {
            path: path.to_path_buf(),
            name: stem,
            modified,
            points: 0,
            aps: 0,
            scale_set: false,
            first_scan: None,
            last_scan: None,
            error: None,
        };
        match std::fs::read_to_string(path).map_err(|e| e.to_string()).and_then(|t| Project::from_json(&t).map_err(|e| e.to_string())) {
            Ok(p) => {
                s.points = p.floor.points.len();
                s.aps = p.floor.bssids().len();
                s.scale_set = p.floor.metres_per_px.is_some();
                s.first_scan = p.floor.points.iter().map(|p| p.unix_secs).min();
                s.last_scan = p.floor.points.iter().map(|p| p.unix_secs).max();
            }
            Err(e) => s.error = Some(e),
        }
        s
    }
}

#[derive(Default)]
pub struct ProjectsTab {
    entries: Vec<Summary>,
    /// Parsed summaries, reused while a file's modified time is unchanged.
    cache: HashMap<PathBuf, (Option<SystemTime>, Summary)>,
    selected: Option<PathBuf>,
    reports: Vec<store::ReportFile>,
    reports_of: Option<PathBuf>,
    status: String,
    /// The last status shown as a snackbar, to spot changes.
    toasted: String,
}

impl ProjectsTab {
    /// A new status message since the last call, for a snackbar.
    pub fn take_toast(&mut self) -> Option<String> {
        if self.status == self.toasted {
            return None;
        }
        self.toasted = self.status.clone();
        (!self.status.is_empty()).then(|| self.status.clone())
    }

    pub fn refresh(&mut self) {
        let mut entries = Vec::new();
        let mut cache = HashMap::new();
        for path in store::project_files() {
            let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
            let summary = match self.cache.remove(&path) {
                Some((m, s)) if m == modified => s,
                _ => Summary::read(&path, modified),
            };
            entries.push(summary.clone());
            cache.insert(path, (modified, summary));
        }
        entries.sort_by(|a, b| b.modified.cmp(&a.modified));
        self.entries = entries;
        self.cache = cache;
        self.reports_of = None;
    }

    fn generate_report(&mut self, path: &Path, style: &report::ReportStyle) {
        let project = std::fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|t| Project::from_json(&t).map_err(|e| e.to_string()));
        let result = project.and_then(|p| report::write_report(path, &p, style));
        match result {
            Ok(file) => {
                self.status = format!("Report saved: {}", file.display());
                if let Err(e) = store::open_externally(&file) {
                    self.status.push_str(&format!(" ({e})"));
                }
            }
            Err(e) => self.status = format!("Could not create report: {e}"),
        }
        self.reports_of = None;
    }

    /// Returns a project to open in the floor plan tab, if one was chosen.
    pub fn show(&mut self, ui: &mut egui::Ui, settings: &Settings) -> Option<PathBuf> {
        let mut open = None;
        let roles = theme::roles(ui.ctx());
        let list_frame = egui::Frame::new().fill(roles.surface).inner_margin(16);
        egui::Panel::left("projects_list").default_size(440.0).frame(list_frame).show(ui, |ui| {
            ui.label(egui::RichText::new("Projects").size(24.0));
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                if theme::button(ui, ButtonKind::Tonal, Some(ICON_FOLDER_OPEN), "Open project file").clicked()
                    && let Some(p) = rfd::FileDialog::new()
                        .add_filter("WiFi Spanner project", &[store::EXTENSION])
                        .set_directory(store::projects_dir())
                        .pick_file()
                {
                    open = Some(p);
                }
                if theme::button(ui, ButtonKind::Text, Some(ICON_REFRESH), "Refresh").clicked() {
                    self.refresh();
                }
                if theme::button(ui, ButtonKind::Text, Some(ICON_FOLDER), "Show folder").clicked() {
                    let dir = store::projects_dir();
                    let _ = std::fs::create_dir_all(&dir);
                    if let Err(e) = store::open_externally(&dir) {
                        self.status = e;
                    }
                }
            });
            ui.weak(store::projects_dir().display().to_string());
            ui.add_space(8.0);
            if self.entries.is_empty() {
                ui.label("No saved projects yet. Open a floor plan in the Floor plan survey tab to start one.");
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                for e in &self.entries {
                    let selected = self.selected.as_ref() == Some(&e.path);
                    let when = e.modified.map_or(String::new(), store::format_time);
                    let meta = match &e.error {
                        Some(_) => "Could not be read".to_string(),
                        None => format!("{} points  ·  {when}", e.points),
                    };
                    let frame = egui::Frame::new()
                        .fill(if selected { roles.secondary_container } else { roles.surface_container_low })
                        .stroke(egui::Stroke::new(1.0, if selected { roles.secondary_container } else { roles.outline_variant }))
                        .corner_radius(12)
                        .inner_margin(14);
                    let card = frame.show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(theme::medium(&e.name).size(16.0));
                        ui.label(egui::RichText::new(meta).size(13.0).color(roles.on_surface_variant));
                    });
                    let resp = ui.interact(card.response.rect, ui.make_persistent_id(("project-card", &e.path)), egui::Sense::click());
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if resp.clicked() {
                        self.selected = Some(e.path.clone());
                    }
                    if resp.double_clicked() && e.error.is_none() {
                        open = Some(e.path.clone());
                    }
                    ui.add_space(6.0);
                }
            });
        });

        egui::CentralPanel::default().frame(theme::page_frame(ui.ctx())).show(ui, |ui| {
            if !self.status.is_empty() {
                ui.colored_label(roles.on_surface_variant, &self.status);
                ui.add_space(8.0);
            }
            let Some(sel) = self.selected.clone() else {
                ui.label("Select a project to see its details and reports.");
                return;
            };
            let Some(s) = self.entries.iter().find(|e| e.path == sel).cloned() else {
                ui.label("Select a project to see its details and reports.");
                return;
            };
            ui.label(egui::RichText::new(&s.name).size(24.0));
            ui.weak(s.path.display().to_string());
            ui.add_space(10.0);
            if let Some(err) = &s.error {
                ui.colored_label(roles.error, format!("Could not read this project: {err}"));
                return;
            }
            theme::card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(format!("{} survey points, {} access points seen", s.points, s.aps));
                ui.label(if s.scale_set { "Scale set" } else { "Scale not set" });
                if let (Some(a), Some(b)) = (s.first_scan, s.last_scan) {
                    ui.label(format!(
                        "Measured {} to {}",
                        store::format_unix(a, "+%Y-%m-%d %H:%M"),
                        store::format_unix(b, "+%Y-%m-%d %H:%M")
                    ));
                }
            });
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                if theme::button(ui, ButtonKind::Filled, Some(ICON_IMAGE), "Open in floor plan tab").clicked() {
                    open = Some(s.path.clone());
                }
                let report = ui.add_enabled_ui(s.points > 0, |ui| theme::button(ui, ButtonKind::Tonal, Some(ICON_DESCRIPTION), "Generate report")).inner;
                if report.clicked() {
                    self.generate_report(&s.path, &report::ReportStyle::from_settings(settings));
                }
            });
            ui.add_space(16.0);
            ui.label(theme::medium("Reports").size(18.0));
            ui.add_space(6.0);
            if self.reports_of.as_ref() != Some(&sel) {
                self.reports = store::reports_for(&sel);
                self.reports_of = Some(sel.clone());
            }
            if self.reports.is_empty() {
                ui.weak("No reports yet.");
            }
            for r in &self.reports {
                theme::card(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let name = r.path.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
                        ui.vertical(|ui| {
                            ui.label(theme::medium(name));
                            ui.weak(r.modified.map_or(String::new(), store::format_time));
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if theme::button(ui, ButtonKind::Tonal, Some(ICON_OPEN_IN_NEW), "Open").clicked()
                                && let Err(e) = store::open_externally(&r.path)
                            {
                                self.status = e;
                            }
                        });
                    });
                });
                ui.add_space(6.0);
            }
            if !self.reports.is_empty() && theme::button(ui, ButtonKind::Text, Some(ICON_FOLDER), "Show reports folder").clicked() {
                if let Err(e) = store::open_externally(&store::reports_dir(&sel)) {
                    self.status = e;
                }
            }
        });
        open
    }
}
