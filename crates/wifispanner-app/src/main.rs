// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 rubble-za. See the LICENSE and NOTICE files.

//! WiFi Spanner desktop app: a live network analyser and a floor plan survey.

mod floor;
mod launcher;
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

/// Where an in-place update stands. Nothing is downloaded until the user
/// confirms in the dialog.
enum InstallState {
    Idle,
    Confirm(update::UpdateInfo),
    Working { info: update::UpdateInfo, done: u64 },
    Failed { info: update::UpdateInfo, message: String },
}

enum InstallMsg {
    Progress(u64),
    Done(Result<(), String>),
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
    update_prefs: update::Prefs,
    /// The AppImage this process runs from, if it is one (enables in-place updates).
    appimage: Option<std::path::PathBuf>,
    install: InstallState,
    install_rx: Option<Receiver<InstallMsg>>,
    install_cancel: Arc<std::sync::atomic::AtomicBool>,
    /// Offer to add the desktop launcher (shown once, after the splash).
    launcher_prompt: bool,
    launcher_message: String,
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
            update_prefs: update::Prefs::load(),
            appimage: update::running_appimage(),
            install: InstallState::Idle,
            install_rx: None,
            install_cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            launcher_prompt: false,
            launcher_message: String::new(),
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
                    self.update_notice = Some(info);
                } else if self.update_prefs.should_announce(&info.tag, update::unix_now()) {
                    // The automatic check respects "Dismiss" and "Remind me tomorrow".
                    self.update_notice = Some(info);
                }
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
        enum Choice {
            Update,
            Tomorrow,
            Dismiss,
        }
        let Some(info) = self.update_notice.clone() else { return };
        let can_install = info.asset.is_some() && self.appimage.is_some();
        let mut choice = None;
        egui::Panel::top("update_notice").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("WiFi Spanner {} is available (you have {}).", info.tag, env!("CARGO_PKG_VERSION")));
                ui.hyperlink_to(if can_install { "Release notes" } else { "View the release and download" }, &info.url);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Dismiss").on_hover_text("Skip this version. You will be told about newer ones.").clicked() {
                        choice = Some(Choice::Dismiss);
                    }
                    if ui.button("Remind me tomorrow").on_hover_text("Hide this for a day, then ask again.").clicked() {
                        choice = Some(Choice::Tomorrow);
                    }
                    if can_install && ui.button("Update now").clicked() {
                        choice = Some(Choice::Update);
                    }
                });
            });
        });
        match choice {
            Some(Choice::Update) => self.install = InstallState::Confirm(info),
            Some(Choice::Tomorrow) => {
                self.update_prefs.snooze(update::unix_now());
                self.update_prefs.save();
                self.update_notice = None;
            }
            Some(Choice::Dismiss) => {
                self.update_prefs.skip(&info.tag);
                self.update_prefs.save();
                self.update_notice = None;
            }
            None => {}
        }
    }

    /// Start the download once the user has confirmed.
    fn start_install(&mut self, ctx: &egui::Context, info: update::UpdateInfo) {
        let (Some(asset), Some(target)) = (info.asset.clone(), self.appimage.clone()) else { return };
        let (tx, rx) = mpsc::channel();
        self.install_cancel.store(false, Ordering::Relaxed);
        let (cancel, repaint) = (self.install_cancel.clone(), ctx.clone());
        std::thread::spawn(move || {
            let result = update::install(&asset, &target, &cancel, |n| {
                let _ = tx.send(InstallMsg::Progress(n));
                repaint.request_repaint();
            });
            let _ = tx.send(InstallMsg::Done(result));
            repaint.request_repaint();
        });
        self.install_rx = Some(rx);
        self.install = InstallState::Working { info, done: 0 };
    }

    fn drain_install(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.install_rx else { return };
        let mut finished = None;
        while let Ok(msg) = rx.try_recv() {
            match msg {
                InstallMsg::Progress(n) => {
                    if let InstallState::Working { done, .. } = &mut self.install {
                        *done = n;
                    }
                }
                InstallMsg::Done(result) => finished = Some(result),
            }
        }
        let Some(result) = finished else { return };
        self.install_rx = None;
        let InstallState::Working { info, .. } = std::mem::replace(&mut self.install, InstallState::Idle) else { return };
        let outcome = result.and_then(|()| match &self.appimage {
            Some(target) => update::restart(target),
            None => Err("not running as an AppImage".into()),
        });
        match outcome {
            Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Err(e) if e == "cancelled" => {}
            Err(message) => self.install = InstallState::Failed { info, message },
        }
    }

    fn install_window(&mut self, ctx: &egui::Context) {
        enum Act {
            Install(update::UpdateInfo),
            Cancel,
            Close,
        }
        let mut act = None;
        let appimage = self.appimage.clone();
        let (title, body): (&str, Box<dyn FnOnce(&mut egui::Ui, &mut Option<Act>)>) = match &self.install {
            InstallState::Idle => return,
            InstallState::Confirm(info) => {
                let info = info.clone();
                let size = info.asset.as_ref().map_or(0, |a| a.size) as f64 / 1_048_576.0;
                let path = appimage.map_or(String::new(), |p| p.display().to_string());
                ("Install update?", Box::new(move |ui, act| {
                    ui.label(format!("Install WiFi Spanner {} and restart?", info.tag));
                    ui.add_space(4.0);
                    ui.weak(format!(
                        "This downloads {size:.1} MB from GitHub, checks it against the checksum GitHub publishes for the \
                         file, then replaces {path} and restarts the app. If anything doesn't check out, nothing is \
                         replaced. Your projects and settings are not touched."
                    ));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Install and restart").clicked() {
                            *act = Some(Act::Install(info.clone()));
                        }
                        if ui.button("Cancel").clicked() {
                            *act = Some(Act::Close);
                        }
                    });
                }))
            }
            InstallState::Working { info, done } => {
                let total = info.asset.as_ref().map_or(1, |a| a.size.max(1));
                let (frac, done) = (*done as f32 / total as f32, *done);
                let tag = info.tag.clone();
                ("Updating", Box::new(move |ui, act| {
                    ui.label(format!("Downloading WiFi Spanner {tag}..."));
                    ui.add(egui::ProgressBar::new(frac).text(format!("{:.1} of {:.1} MB", done as f64 / 1_048_576.0, total as f64 / 1_048_576.0)));
                    if ui.button("Cancel").clicked() {
                        *act = Some(Act::Cancel);
                    }
                }))
            }
            InstallState::Failed { info, message } => {
                let (url, message) = (info.url.clone(), message.clone());
                ("Update failed", Box::new(move |ui, act| {
                    ui.colored_label(egui::Color32::LIGHT_RED, format!("The update was not installed: {message}"));
                    ui.label("Your current version is unchanged.");
                    ui.hyperlink_to("Download it from the release page instead", url);
                    if ui.button("Close").clicked() {
                        *act = Some(Act::Close);
                    }
                }))
            }
        };
        egui::Window::new(title).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO).show(ctx, |ui| {
            ui.set_max_width(460.0);
            body(ui, &mut act);
        });
        match act {
            Some(Act::Install(info)) => self.start_install(ctx, info),
            Some(Act::Cancel) => self.install_cancel.store(true, Ordering::Relaxed),
            Some(Act::Close) => self.install = InstallState::Idle,
            None => {}
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

    /// Add or remove the desktop launcher and icon.
    fn set_launcher(&mut self, add: bool) {
        let result = match (&self.appimage, add) {
            (Some(app), true) => launcher::install(app),
            (_, false) => launcher::remove(),
            (None, true) => Err("not running as an AppImage".into()),
        };
        self.launcher_message = match (result, add) {
            (Ok(()), true) => "Added. Restart WiFi Spanner to see the icon in the title bar.".into(),
            (Ok(()), false) => "Removed.".into(),
            (Err(e), _) => format!("Could not change the launcher: {e}"),
        };
    }

    /// The one-time offer to add the launcher. Either answer is remembered.
    fn launcher_prompt_window(&mut self, ctx: &egui::Context) {
        if !self.launcher_prompt {
            return;
        }
        let mut answer = None;
        egui::Window::new("Add WiFi Spanner to your application menu?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_max_width(440.0);
                ui.label(
                    "This adds a launcher and the WiFi Spanner icon to your home folder (a .desktop file under \
                     ~/.local/share/applications and an icon under ~/.local/share/icons), so the app shows its icon in \
                     the taskbar and title bar and can be started from the menu.",
                );
                ui.weak("You can remove it any time in Settings > General.");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Add launcher").clicked() {
                        answer = Some(true);
                    }
                    if ui.button("Not now").clicked() {
                        answer = Some(false);
                    }
                });
            });
        if let Some(add) = answer {
            if add {
                self.set_launcher(true);
            }
            self.launcher_prompt = false;
            self.settings.launcher_prompted = true;
            self.settings.save();
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
                    if let Some(logo) = &self.logo {
                    ui.add(egui::Image::new(logo).fit_to_exact_size(egui::Vec2::splat(BAR_ROW_HEIGHT - 14.0)));
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
        let mut launcher = settings_ui::LauncherView {
            available: self.appimage.is_some(),
            installed: launcher::is_installed(),
            message: self.launcher_message.clone(),
            request: None,
        };
        if settings_ui::show(ctx, &mut open, &mut self.settings_section, &mut self.settings, &self.cards, &mut launcher) {
            self.apply_settings();
        }
        if let Some(add) = launcher.request {
            self.set_launcher(add);
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
        self.drain_install(ui.ctx());

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
                if let Some(app) = &self.appimage {
                    if launcher::is_installed() {
                        launcher::refresh_if_stale(app);
                    } else if !self.settings.launcher_prompted {
                        self.launcher_prompt = true;
                    }
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
        self.install_window(ui.ctx());
        self.launcher_prompt_window(ui.ctx());

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
