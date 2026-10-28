// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 rubble-za. See the LICENSE and NOTICE files.

//! Projects tab: previous surveys and their reports.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use eframe::egui;
use wifispanner_core::survey::Project;

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
}

impl ProjectsTab {
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

    fn generate_report(&mut self, path: &Path) {
        let project = std::fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|t| Project::from_json(&t).map_err(|e| e.to_string()));
        let result = project.and_then(|p| report::write_report(path, &p));
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
    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<PathBuf> {
        let mut open = None;
        egui::Panel::left("projects_list").default_size(440.0).show(ui, |ui| {
            ui.heading("Projects");
            ui.horizontal(|ui| {
                if ui.button("Open project file...").clicked()
                    && let Some(p) = rfd::FileDialog::new()
                        .add_filter("WiFi Spanner project", &[store::EXTENSION])
                        .set_directory(store::projects_dir())
                        .pick_file()
                {
                    open = Some(p);
                }
                if ui.button("Refresh").clicked() {
                    self.refresh();
                }
                if ui.button("Show folder").clicked() {
                    let dir = store::projects_dir();
                    let _ = std::fs::create_dir_all(&dir);
                    if let Err(e) = store::open_externally(&dir) {
                        self.status = e;
                    }
                }
            });
            ui.weak(store::projects_dir().display().to_string());
            ui.separator();
            if self.entries.is_empty() {
                ui.label("No saved projects yet. Open a floor plan in the Floor plan survey tab to start one.");
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                for e in &self.entries {
                    let when = e.modified.map_or(String::new(), store::format_time);
                    let label = match &e.error {
                        Some(_) => format!("{}  (unreadable)", e.name),
                        None => format!("{}   {} points   {}", e.name, e.points, when),
                    };
                    let resp = ui.selectable_label(self.selected.as_ref() == Some(&e.path), label);
                    if resp.clicked() {
                        self.selected = Some(e.path.clone());
                    }
                    if resp.double_clicked() && e.error.is_none() {
                        open = Some(e.path.clone());
                    }
                }
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            if !self.status.is_empty() {
                ui.label(&self.status);
                ui.separator();
            }
            let Some(sel) = self.selected.clone() else {
                ui.label("Select a project to see its details and reports.");
                return;
            };
            let Some(s) = self.entries.iter().find(|e| e.path == sel).cloned() else {
                ui.label("Select a project to see its details and reports.");
                return;
            };
            ui.heading(&s.name);
            ui.weak(s.path.display().to_string());
            if let Some(err) = &s.error {
                ui.colored_label(egui::Color32::LIGHT_RED, format!("Could not read this project: {err}"));
                return;
            }
            ui.label(format!("{} survey points, {} access points seen", s.points, s.aps));
            ui.label(if s.scale_set { "Scale set" } else { "Scale not set" });
            if let (Some(a), Some(b)) = (s.first_scan, s.last_scan) {
                ui.label(format!(
                    "Measured {} to {}",
                    store::format_unix(a, "+%Y-%m-%d %H:%M"),
                    store::format_unix(b, "+%Y-%m-%d %H:%M")
                ));
            }
            ui.horizontal(|ui| {
                if ui.button("Open in floor plan tab").clicked() {
                    open = Some(s.path.clone());
                }
                if ui.add_enabled(s.points > 0, egui::Button::new("Generate report")).clicked() {
                    self.generate_report(&s.path);
                }
            });
            ui.separator();
            ui.strong("Reports");
            if self.reports_of.as_ref() != Some(&sel) {
                self.reports = store::reports_for(&sel);
                self.reports_of = Some(sel.clone());
            }
            if self.reports.is_empty() {
                ui.label("No reports yet.");
            }
            for r in &self.reports {
                ui.horizontal(|ui| {
                    let name = r.path.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
                    ui.label(format!("{name}   {}", r.modified.map_or(String::new(), store::format_time)));
                    if ui.button("Open").clicked()
                        && let Err(e) = store::open_externally(&r.path)
                    {
                        self.status = e;
                    }
                });
            }
            if !self.reports.is_empty() && ui.button("Show reports folder").clicked() {
                if let Err(e) = store::open_externally(&store::reports_dir(&sel)) {
                    self.status = e;
                }
            }
        });
        open
    }
}
