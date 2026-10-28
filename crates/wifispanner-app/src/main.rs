// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 rubble-za. See the LICENSE and NOTICE files.

//! WiFi Spanner desktop app: a live network analyser and a floor plan survey.

mod floor;
mod live;
mod pdf;
mod projects;
mod report;
mod settings;
mod settings_ui;
mod store;
mod update;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::Duration;

use eframe::egui;
use settings::Settings;
use wifispanner_scan::dwell::scan_for_many;
use wifispanner_scan::iw::WifiCard;
use wifispanner_scan::{ScanResult, Scanner, default_scanner, list_cards};

const LOGO_PNG: &[u8] = include_bytes!("../../../assets/logo.png");
const SPLASH_SIZE: [f32; 2] = [560.0, 340.0];
const MAIN_SIZE: [f32; 2] = [1200.0, 760.0];
/// The splash stays up at least this long so it can be read.
const SPLASH_MIN: Duration = Duration::from_millis(1800);
/// The project page, shown in About as the attribution link.
const REPO_URL: &str = "https://github.com/rubble-za/WiFi-Spanner";
const TAGLINE: &str = "Your one-stop WiFi Scanner and Planner";

/// Height of the top bar, about twice egui's default for easier touch use.
const BAR_ROW_HEIGHT: f32 = 48.0;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        // Starts as a small borderless splash; `App::ui` turns it into the
        // main window once the scanner is ready.
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(SPLASH_SIZE)
            .with_decorations(false)
            .with_title("WiFi Spanner")
            .with_app_id("wifispanner")
            .with_icon(window_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "WiFi Spanner",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc.egui_ctx.clone())))),
    )
}

/// Requests to the scanning thread.
enum Cmd {
    /// Scan continuously for the live tab, or stop doing so.
    Live(bool),
    /// Scan with these interfaces from now on.
    SetCards(Vec<String>),
    /// One dwell scan for a survey point at floor plan position (x, y).
    Survey { x: f32, y: f32 },
}

enum Msg {
    Started(String),
    Progress(usize),
    Live(Box<ScanResult>),
    Point { x: f32, y: f32, dwell_secs: u64, result: Box<ScanResult> },
    Failed(String),
}

/// Handle to the scanning thread.
struct Worker {
    tx: Sender<Cmd>,
    dwell_secs: Arc<AtomicU64>,
}

impl Worker {
    fn spawn(ctx: egui::Context, dwell_secs: Arc<AtomicU64>, cards: Vec<String>) -> (Worker, Receiver<Msg>) {
        let (cmd_tx, cmd_rx) = mpsc::channel::<Cmd>();
        let (tx, rx) = mpsc::channel();
        let dwell = dwell_secs.clone();
        std::thread::spawn(move || {
            let build = |names: &[String]| -> Vec<Box<dyn Scanner>> {
                let scanners: Vec<_> = names
                    .iter()
                    .filter_map(|n| match default_scanner(Some(n.clone())) {
                        Ok(s) => Some(s),
                        Err(e) => {
                            let _ = tx.send(Msg::Failed(format!("{n}: {e:#}")));
                            None
                        }
                    })
                    .collect();
                let name = scanners.iter().map(|s| s.name()).collect::<Vec<_>>().join(" and ");
                let _ = tx.send(Msg::Started(if name.is_empty() { "no Wi-Fi card".into() } else { name }));
                scanners
            };
            let mut scanners = build(&cards);
            let mut live = false;
            loop {
                let cmd = if live && !scanners.is_empty() {
                    match cmd_rx.try_recv() {
                        Ok(c) => Some(c),
                        Err(TryRecvError::Empty) => None,
                        Err(TryRecvError::Disconnected) => return,
                    }
                } else {
                    match cmd_rx.recv() {
                        Ok(c) => Some(c),
                        Err(_) => return,
                    }
                };
                let survey_at = match cmd {
                    Some(Cmd::Live(on)) => {
                        live = on;
                        continue;
                    }
                    Some(Cmd::SetCards(names)) => {
                        scanners = build(&names);
                        continue;
                    }
                    Some(Cmd::Survey { x, y }) => Some((x, y)),
                    None => None,
                };
                if scanners.is_empty() {
                    let _ = tx.send(Msg::Failed("no Wi-Fi card is available for scanning".into()));
                    ctx.request_repaint();
                    live = false;
                    continue;
                }
                let dwell_secs = dwell.load(Ordering::Relaxed);
                let progress = |n: usize| {
                    let _ = tx.send(Msg::Progress(n));
                    ctx.request_repaint();
                };
                let result = scan_for_many(&mut scanners, Duration::from_secs(dwell_secs), &progress);
                let msg = match (result, survey_at) {
                    (Ok(r), Some((x, y))) => Msg::Point { x, y, dwell_secs, result: Box::new(r) },
                    (Ok(r), None) => Msg::Live(Box::new(r)),
                    (Err(e), _) => Msg::Failed(format!("{e:#}")),
                };
                if tx.send(msg).is_err() {
                    return;
                }
                ctx.request_repaint();
            }
        });
        (Worker { tx: cmd_tx, dwell_secs }, rx)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Live,
    Floor,
    Projects,
}

struct App {
    rx: Receiver<Msg>,
    worker: Worker,
    tab: Tab,
    scanner_name: String,
    scans_so_far: usize,
    error: Option<String>,
    latest: Option<ScanResult>,
    floor: floor::FloorTab,
    projects: projects::ProjectsTab,
    settings: Settings,
    cards: Vec<WifiCard>,
    settings_open: bool,
    about_open: bool,
    update_rx: Option<Receiver<(bool, Result<Option<update::UpdateInfo>, String>)>>,
    /// A newer release found, shown in a bar until dismissed.
    update_notice: Option<update::UpdateInfo>,
    /// Result of a manual check: message and an optional release link.
    update_message: Option<(String, Option<String>)>,
    settings_section: settings_ui::Section,
    logo: Option<egui::TextureHandle>,
    /// Set while the splash is showing.
    splash_since: Option<std::time::Instant>,
    scanner_ready: bool,
    /// Width of the tab row, measured each frame to keep it centred.
    tabs_width: f32,
}

impl App {
    fn new(ctx: egui::Context) -> Self {
        let ctx_for_logo = ctx.clone();
        let settings = Settings::load();
        let cards = list_cards().unwrap_or_default();
        let dwell = Arc::new(AtomicU64::new(settings.dwell_secs));
        let (worker, rx) = Worker::spawn(ctx, dwell, settings.active_cards(&cards));
        let _ = worker.tx.send(Cmd::Live(true));
        App {
            rx,
            worker,
            tab: Tab::Live,
            scanner_name: "starting...".into(),
            scans_so_far: 0,
            error: None,
            latest: None,
            floor: floor::FloorTab::default(),
            projects: projects::ProjectsTab::default(),
            settings,
            cards,
            settings_open: false,
            about_open: false,
            update_rx: None,
            update_notice: None,
            update_message: None,
            settings_section: settings_ui::Section::default(),
            logo: logo_texture(&ctx_for_logo),
            splash_since: Some(std::time::Instant::now()),
            scanner_ready: false,
            tabs_width: 0.0,
        }
    }

    fn drain(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Started(name) => {
                    self.scanner_name = name;
                    self.scanner_ready = true;
                }
                Msg::Progress(n) => self.scans_so_far = n,
                Msg::Live(r) => {
                    self.scans_so_far = 0;
                    self.latest = Some(*r);
                    self.error = None;
                }
                Msg::Point { x, y, dwell_secs, result } => {
                    self.scans_so_far = 0;
                    self.error = None;
                    self.floor.add_point(x, y, dwell_secs, &result);
                }
                Msg::Failed(e) => {
                    self.scanner_ready = true;
                    self.scans_so_far = 0;
                    self.floor.scan_failed();
                    self.error = Some(e);
                }
            }
        }
    }

    fn main_menu(&mut self, ui: &mut egui::Ui, action: &mut Option<MenuAction>) {
        ui.set_min_width(230.0);
        ui.spacing_mut().interact_size.y = 34.0;
        ui.spacing_mut().button_padding = egui::vec2(12.0, 6.0);
        ui.menu_button("File", |ui| {
            if ui.button("Open floor plan...").clicked() {
                *action = Some(MenuAction::OpenPlan);
            }
            if ui.button("Open project...").clicked() {
                *action = Some(MenuAction::OpenProject);
            }
            ui.separator();
            if ui.button("Show projects folder").clicked() {
                *action = Some(MenuAction::ShowProjectsFolder);
            }
        });
        if ui.button("Settings...").clicked() {
            self.settings_open = true;
        }
        ui.menu_button("Adapters", |ui| {
            let before = self.settings.card.clone();
            if ui.selectable_label(before.is_none(), "Auto (connected card)").clicked() {
                self.settings.card = None;
            }
            for c in &self.cards {
                if ui.selectable_label(before.as_ref() == Some(&c.name), c.label()).clicked() {
                    self.settings.card = Some(c.name.clone());
                }
            }
            if self.cards.is_empty() {
                ui.label("No Wi-Fi cards found");
            }
            ui.separator();
            let refresh = ui.button("Refresh cards").clicked();
            if refresh {
                self.cards = list_cards().unwrap_or_default();
            }
            if refresh || before != self.settings.card {
                self.apply_settings();
            }
        });
        ui.separator();
        if ui.button("Check for updates").clicked() {
            *action = Some(MenuAction::CheckUpdates);
        }
        if ui.button("About...").clicked() {
            self.about_open = true;
        }
        ui.separator();
        if ui.button("Quit").clicked() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// Check GitHub on a background thread. A manual check reports every
    /// outcome; the automatic one only speaks up when there is a newer release.
    fn start_update_check(&mut self, ctx: &egui::Context, manual: bool) {
        if self.update_rx.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send((manual, update::check(env!("CARGO_PKG_VERSION"))));
            repaint.request_repaint();
        });
        self.update_rx = Some(rx);
    }

    fn drain_update(&mut self) {
        let Some(rx) = &self.update_rx else { return };
        let Ok((manual, result)) = rx.try_recv() else { return };
        self.update_rx = None;
        match result {
            Ok(Some(info)) => {
                if manual {
                    let text = format!("WiFi Spanner {} is available. You have {}.", info.tag, env!("CARGO_PKG_VERSION"));
                    self.update_message = Some((text, Some(info.url.clone())));
                }
                self.update_notice = Some(info);
            }
            Ok(None) if manual => {
                self.update_message = Some((format!("You have the latest version ({}).", env!("CARGO_PKG_VERSION")), None));
            }
            Err(e) if manual => {
                self.update_message = Some((format!("Could not check for updates: {e}"), None));
            }
            _ => {}
        }
    }

    fn update_notice_bar(&mut self, ui: &mut egui::Ui) {
        let Some(info) = self.update_notice.clone() else { return };
        let mut dismiss = false;
        egui::Panel::top("update_notice").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "WiFi Spanner {} is available (you have {}).",
                    info.tag,
                    env!("CARGO_PKG_VERSION")
                ));
                ui.hyperlink_to("View the release and download", &info.url);
                if ui.button("Dismiss").clicked() {
                    dismiss = true;
                }
            });
        });
        if dismiss {
            self.update_notice = None;
        }
    }

    fn update_message_window(&mut self, ctx: &egui::Context) {
        let Some((text, url)) = self.update_message.clone() else { return };
        let mut close = false;
        egui::Window::new("Check for updates")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_max_width(420.0);
                ui.label(&text);
                if let Some(url) = &url {
                    ui.hyperlink_to("View the release and download", url);
                }
                if ui.button("OK").clicked() {
                    close = true;
                }
            });
        if close {
            self.update_message = None;
        }
    }

    fn about_window(&mut self, ctx: &egui::Context) {
        let mut open = self.about_open;
        egui::Window::new("About WiFi Spanner")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    if let Some(logo) = &self.logo {
                        ui.add(egui::Image::new(logo).fit_to_exact_size(egui::Vec2::splat(96.0)));
                    }
                    ui.label(egui::RichText::new("WiFi Spanner").heading());
                    ui.label(TAGLINE);
                    ui.weak(format!("Version {}", env!("CARGO_PKG_VERSION")));
                    ui.add_space(6.0);
                    ui.label("Copyright (C) 2026 rubble-za");
                    ui.hyperlink_to(REPO_URL, REPO_URL);
                    ui.add_space(6.0);
                    ui.weak(
                        "Free software under the GNU General Public License, version 3 or later, with attribution \
                         terms (see NOTICE). This program comes with ABSOLUTELY NO WARRANTY.",
                    );
                    ui.hyperlink_to("GNU General Public License", "https://www.gnu.org/licenses/gpl-3.0.html");
                });
            });
        self.about_open = open;
    }

    /// Push the current settings to the scanner thread and save them.
    fn apply_settings(&mut self) {
        self.worker.dwell_secs.store(self.settings.dwell_secs, Ordering::Relaxed);
        let _ = self.worker.tx.send(Cmd::SetCards(self.settings.active_cards(&self.cards)));
        self.settings.save();
    }

    fn settings_dialog(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let mut open = true;
        if settings_ui::show(ctx, &mut open, &mut self.settings_section, &mut self.settings, &self.cards) {
            self.apply_settings();
        }
        self.settings_open = open;
    }

    fn set_tab(&mut self, tab: Tab) {
        if self.tab != tab {
            self.tab = tab;
            if tab == Tab::Projects {
                self.projects.refresh();
            }
            let _ = self.worker.tx.send(Cmd::Live(tab == Tab::Live));
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.drain();
        self.drain_update();

        if let Some(since) = self.splash_since {
            if self.scanner_ready && since.elapsed() >= SPLASH_MIN {
                self.splash_since = None;
                let ctx = ui.ctx();
                ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(MAIN_SIZE[0], MAIN_SIZE[1])));
                if let Some(center) = egui::ViewportCommand::center_on_screen(ctx) {
                    ctx.send_viewport_cmd(center);
                }
                if self.settings.check_updates {
                    self.start_update_check(ctx, false);
                }
            } else {
                draw_splash(ui, self.logo.as_ref());
                ui.ctx().request_repaint_after(Duration::from_millis(100));
                return;
            }
        }

        let mut action: Option<MenuAction> = None;
        egui::Panel::top("status").show(ui, |ui| {
            ui.spacing_mut().interact_size.y = BAR_ROW_HEIGHT;
            ui.spacing_mut().button_padding = egui::vec2(14.0, 10.0);
            ui.spacing_mut().item_spacing.x = 12.0;
            ui.horizontal(|ui| {
                let row = ui.available_rect_before_wrap();
                ui.label(egui::RichText::new("WiFi Spanner").heading());
                // Tabs are centred on the bar; their width comes from the last
                // frame, as it is only known after they are laid out.
                let left = (row.center().x - self.tabs_width / 2.0).max(ui.cursor().min.x + 12.0);
                let rect = egui::Rect::from_min_size(
                    egui::pos2(left, row.top()),
                    egui::vec2(self.tabs_width + 8.0, BAR_ROW_HEIGHT),
                );
                let layout = egui::Layout::left_to_right(egui::Align::Center);
                let used = ui
                    .scope_builder(egui::UiBuilder::new().max_rect(rect).layout(layout), |ui| {
                        let mut tab = self.tab;
                        for (t, name) in [(Tab::Live, "Live analysis"), (Tab::Floor, "Floor plan survey"), (Tab::Projects, "Projects")] {
                            ui.selectable_value(&mut tab, t, egui::RichText::new(name).size(16.0));
                        }
                        self.set_tab(tab);
                        ui.min_rect().width()
                    })
                    .inner;
                if (used - self.tabs_width).abs() > 0.5 {
                    self.tabs_width = used;
                    ui.ctx().request_repaint();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let button = hamburger_button(ui);
                    egui::Popup::menu(&button).show(|ui| self.main_menu(ui, &mut action));
                });
            });
        });
        match action {
            Some(MenuAction::OpenPlan) => {
                self.set_tab(Tab::Floor);
                self.floor.pick_plan(ui.ctx());
            }
            Some(MenuAction::OpenProject) => {
                self.set_tab(Tab::Floor);
                self.floor.pick_project(ui.ctx());
            }
            Some(MenuAction::ShowProjectsFolder) => {
                let dir = store::projects_dir();
                let _ = std::fs::create_dir_all(&dir);
                if let Err(e) = store::open_externally(&dir) {
                    self.error = Some(e);
                }
            }
            Some(MenuAction::CheckUpdates) => self.start_update_check(ui.ctx(), true),
            None => {}
        }
        self.update_notice_bar(ui);
        self.settings_dialog(ui.ctx());
        self.about_window(ui.ctx());
        self.update_message_window(ui.ctx());

        // Slim status bar at egui's default height, so it only fits its text.
        egui::Panel::bottom("scan_status").show(ui, |ui| {
            ui.horizontal(|ui| {
                if self.scans_so_far > 0 {
                    ui.label(format!("scanning... {} done", self.scans_so_far));
                    ui.separator();
                }
                ui.label(&self.scanner_name);
                if self.tab == Tab::Live
                    && let Some(r) = &self.latest
                {
                    ui.separator();
                    ui.label(format!(
                        "{} networks ({} fresh), last dwell {:.1} s",
                        r.networks.len(),
                        r.fresh_count,
                        r.scan_ms as f64 / 1000.0
                    ));
                    if let Some(n) = r.noise_dbm {
                        ui.separator();
                        ui.label(format!("noise {n:.0} dBm"));
                    }
                }
                if let Some(e) = &self.error {
                    ui.separator();
                    ui.colored_label(egui::Color32::LIGHT_RED, e);
                }
            });
        });

        match self.tab {
            Tab::Live => {
                egui::CentralPanel::default().show(ui, |ui| live::show(ui, self.latest.as_ref()));
            }
            Tab::Floor => {
                let env = floor::Env {
                    tx: &self.worker.tx,
                    dwell_secs: self.worker.dwell_secs.load(Ordering::Relaxed),
                    scans_so_far: self.scans_so_far,
                    settings: &self.settings,
                };
                self.floor.show(ui, &env);
            }
            Tab::Projects => {
                if let Some(path) = self.projects.show(ui) {
                    self.floor.open_project(ui.ctx(), path);
                    self.set_tab(Tab::Floor);
                }
            }
        }
    }
}

enum MenuAction {
    OpenPlan,
    OpenProject,
    ShowProjectsFolder,
    CheckUpdates,
}

/// Three bars, drawn with the painter so it does not depend on a font glyph.
fn hamburger_button(ui: &mut egui::Ui) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::Vec2::splat(BAR_ROW_HEIGHT - 6.0), egui::Sense::click());
    let v = ui.style().interact(&resp);
    let painter = ui.painter();
    painter.rect_filled(rect, 8.0, v.bg_fill);
    let (c, half, gap) = (rect.center(), rect.width() * 0.26, rect.height() * 0.17);
    for dy in [-gap, 0.0, gap] {
        painter.line_segment(
            [egui::pos2(c.x - half, c.y + dy), egui::pos2(c.x + half, c.y + dy)],
            egui::Stroke::new(3.0, v.fg_stroke.color),
        );
    }
    resp.on_hover_text("Menu")
}

fn logo_texture(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let img = image::load_from_memory(LOGO_PNG).ok()?.resize(256, 256, image::imageops::FilterType::Lanczos3).to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Some(ctx.load_texture("logo", egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()), egui::TextureOptions::LINEAR))
}

fn window_icon() -> egui::IconData {
    match image::load_from_memory(LOGO_PNG) {
        Ok(img) => {
            let img = img.resize(256, 256, image::imageops::FilterType::Lanczos3).to_rgba8();
            egui::IconData { width: img.width(), height: img.height(), rgba: img.into_raw() }
        }
        Err(_) => egui::IconData::default(),
    }
}

fn draw_splash(ui: &mut egui::Ui, logo: Option<&egui::TextureHandle>) {
    egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
        let rect = ui.max_rect();
        let painter = ui.painter();
        let mut mesh = egui::Mesh::default();
        for (pos, color) in [
            (rect.left_top(), egui::Color32::from_rgb(226, 160, 100)),
            (rect.right_top(), egui::Color32::from_rgb(238, 88, 112)),
            (rect.right_bottom(), egui::Color32::from_rgb(40, 150, 212)),
            (rect.left_bottom(), egui::Color32::from_rgb(122, 72, 200)),
        ] {
            mesh.colored_vertex(pos, color);
        }
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        painter.add(egui::Shape::mesh(mesh));

        let c = rect.center();
        if let Some(logo) = logo {
            let r = egui::Rect::from_center_size(c - egui::vec2(0.0, 62.0), egui::Vec2::splat(112.0));
            painter.image(logo.id(), r, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
        }
        let shadow = egui::Color32::from_black_alpha(90);
        for (text, y, size) in [("WiFi Spanner", 50.0, 36.0), (TAGLINE, 92.0, 17.0)] {
            let font = egui::FontId::proportional(size);
            painter.text(c + egui::vec2(1.5, y + 1.5), egui::Align2::CENTER_CENTER, text, font.clone(), shadow);
            painter.text(c + egui::vec2(0.0, y), egui::Align2::CENTER_CENTER, text, font, egui::Color32::WHITE);
        }
        painter.text(
            rect.center_bottom() - egui::vec2(0.0, 18.0),
            egui::Align2::CENTER_CENTER,
            "Loading...",
            egui::FontId::proportional(12.0),
            egui::Color32::from_white_alpha(190),
        );
    });
}
