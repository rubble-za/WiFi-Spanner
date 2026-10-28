// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 rubble-za. See the LICENSE and NOTICE files.

//! Real-time analysis tab: the collated network table.

use eframe::egui;
use wifispanner_scan::ScanResult;

pub fn show(ui: &mut egui::Ui, latest: Option<&ScanResult>) {
    let Some(r) = latest else {
        ui.label("Waiting for first scan...");
        return;
    };
    let mut nets: Vec<_> = r.networks.iter().collect();
    nets.sort_by(|a, b| b.signal_dbm.partial_cmp(&a.signal_dbm).unwrap_or(std::cmp::Ordering::Equal));

    egui::ScrollArea::both().show(ui, |ui| {
        egui::Grid::new("networks").striped(true).num_columns(10).spacing([16.0, 4.0]).show(ui, |ui| {
            for h in ["SSID", "BSSID", "Ch", "Band", "Width", "Standard", "Security", "Signal", "Seen", "Util"] {
                ui.strong(h);
            }
            ui.end_row();
            for n in nets {
                let ssid = n.ssid.as_deref().unwrap_or("<hidden>");
                if n.associated {
                    ui.strong(format!("{ssid} (connected)"));
                } else {
                    ui.label(ssid);
                }
                ui.monospace(&n.bssid);
                ui.label(n.channel.to_string());
                ui.label(n.band.to_string());
                ui.label(format!("{} MHz", n.width_mhz));
                ui.label(n.standard.to_string());
                ui.label(n.security.to_string());
                match n.signal_dbm {
                    Some(s) => {
                        ui.colored_label(signal_color(s), format!("{s:.0} dBm"));
                    }
                    None => {
                        ui.label("-");
                    }
                }
                ui.label(if n.samples == 0 { "stale".to_string() } else { n.samples.to_string() });
                ui.label(n.channel_util_pct.map_or("-".into(), |u| format!("{u:.0}%")));
                ui.end_row();
            }
        });
    });
}

pub fn signal_color(dbm: f64) -> egui::Color32 {
    match dbm {
        d if d >= -60.0 => egui::Color32::from_rgb(80, 200, 120),
        d if d >= -70.0 => egui::Color32::from_rgb(230, 200, 60),
        d if d >= -80.0 => egui::Color32::from_rgb(240, 140, 50),
        _ => egui::Color32::from_rgb(230, 80, 80),
    }
}
