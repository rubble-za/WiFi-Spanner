// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 rubble-za. See the LICENSE and NOTICE files.

//! HTML survey report: one self-contained file with a heatmap per network
//! and band, coverage statistics, and tables of the measurements. Print it
//! to PDF from a browser if needed.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use base64::{Engine, engine::general_purpose::STANDARD as B64};
use wifispanner_core::heatmap::{self, FAIR_DBM, GOOD_DBM, Grid, RAMP_MAX, RAMP_MIN, Sample};
use wifispanner_core::survey::{Floor, Project, SurveyPoint, Target};
use wifispanner_scan::Band;

use crate::store;

const HEAT_CELLS: usize = 256;
const FADE_M: f32 = 8.0;
const HEAT_OPACITY: f32 = 0.65;
const BANDS: [Band; 3] = [Band::Ghz2_4, Band::Ghz5, Band::Ghz6];

/// Build the report for `project` and write it to the project's reports
/// folder, returning the file written.
pub fn write_report(project_path: &Path, project: &Project) -> Result<PathBuf, String> {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let html = build_html(project, &store::format_unix(now, "+%Y-%m-%d %H:%M"))?;
    let dir = store::reports_dir(project_path);
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    let path = dir.join(format!("report-{}.html", store::format_unix(now, "+%Y%m%d-%H%M%S")));
    std::fs::write(&path, html).map_err(|e| format!("could not write {}: {e}", path.display()))?;
    Ok(path)
}

struct View {
    title: String,
    target: Target,
    band: Option<Band>,
}

/// The heatmaps worth showing: overall coverage, then each network that was
/// heard at half or more of the points, each split by band where it has
/// several. Weaker neighbours stay in the access point table only.
fn views(floor: &Floor) -> Vec<View> {
    let n = floor.points.len();
    let bands_of = |t: &Target| -> Vec<Band> {
        BANDS
            .into_iter()
            .filter(|&b| floor.points.iter().any(|p| Floor::signal_at(p, t, Some(b)) > wifispanner_core::survey::MISSING_DBM))
            .collect()
    };
    let mut out = Vec::new();
    let mut push = |label: String, target: Target| {
        let bands = bands_of(&target);
        if bands.is_empty() {
            return;
        }
        out.push(View { title: format!("{label}: all bands"), target: target.clone(), band: None });
        if bands.len() > 1 {
            for b in bands {
                out.push(View { title: format!("{label}: {b}"), target: target.clone(), band: Some(b) });
            }
        }
    };
    push("Strongest signal from any network".into(), Target::Any);
    let need = n.div_ceil(2).max(1);
    for ssid in floor.ssids() {
        let t = Target::Ssid(ssid.clone());
        let heard = floor.points.iter().filter(|p| Floor::signal_at(p, &t, None) > wifispanner_core::survey::MISSING_DBM).count();
        if heard >= need {
            push(format!("Network \"{ssid}\""), t);
        }
    }
    out
}

fn grid_for(floor: &Floor, view: &View) -> Grid {
    let samples: Vec<Sample> = floor
        .points
        .iter()
        .map(|p| Sample { x: p.x, y: p.y, dbm: Floor::signal_at(p, &view.target, view.band) })
        .collect();
    let fade_px = match floor.metres_per_px {
        Some(m) => FADE_M / m,
        None => floor.width_px.max(floor.height_px) as f32 * 0.25,
    };
    heatmap::interpolate(&samples, floor.width_px, floor.height_px, HEAT_CELLS, fade_px)
}

fn heat_png(grid: &Grid) -> Result<Vec<u8>, String> {
    let mut img = image::RgbaImage::new(grid.w as u32, grid.h as u32);
    for (i, px) in img.pixels_mut().enumerate() {
        let [r, g, b] = heatmap::color_for(grid.dbm[i] as f64);
        *px = image::Rgba([r, g, b, (grid.confidence[i] * 255.0) as u8]);
    }
    let mut buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(buf)
}

pub fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            c if c.is_control() => o.push(' '),
            c => o.push(c),
        }
    }
    o
}

fn position(floor: &Floor, p: &SurveyPoint) -> String {
    match floor.metres_per_px {
        Some(m) => format!("{:.1} m, {:.1} m", p.x * m, p.y * m),
        None => format!("{:.0} px, {:.0} px", p.x, p.y),
    }
}

fn mime_of(image: &[u8]) -> &'static str {
    if image.starts_with(&[0x89, b'P', b'N', b'G']) { "image/png" } else { "image/jpeg" }
}

fn legend() -> String {
    let stops = (0..=10)
        .map(|i| {
            let [r, g, b] = heatmap::color_for(RAMP_MIN + (RAMP_MAX - RAMP_MIN) * i as f64 / 10.0);
            format!("rgb({r},{g},{b}) {}%", i * 10)
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "<div class=\"legend\"><span>{RAMP_MIN:.0} dBm</span><div class=\"bar\" style=\"background:linear-gradient(90deg,{stops})\"></div><span>{RAMP_MAX:.0} dBm or better</span></div>"
    )
}

pub fn build_html(project: &Project, generated: &str) -> Result<String, String> {
    let floor = &project.floor;
    if floor.points.is_empty() {
        return Err("the project has no survey points yet".into());
    }
    let (w, h) = (floor.width_px, floor.height_px);
    let marker_r = w.max(h) as f32 / 110.0;
    let mut o = String::new();

    let _ = write!(
        o,
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>{} - WiFi survey report</title><style>{CSS}</style></head><body>",
        esc(&floor.name)
    );
    // The plan is stored once and referenced by every figure.
    let _ = write!(
        o,
        "<svg width=\"0\" height=\"0\" style=\"position:absolute\"><defs><image id=\"plan\" width=\"{w}\" height=\"{h}\" href=\"data:{};base64,{}\"/></defs></svg>",
        mime_of(&floor.image),
        B64.encode(&floor.image)
    );

    let first = floor.points.iter().map(|p| p.unix_secs).min().unwrap_or(0);
    let last = floor.points.iter().map(|p| p.unix_secs).max().unwrap_or(0);
    let _ = write!(
        o,
        "<h1>WiFi survey report: {}</h1><p class=\"meta\">Generated {} &middot; {} survey points &middot; measured {} to {} &middot; scale {}</p>",
        esc(&floor.name),
        esc(generated),
        floor.points.len(),
        esc(&store::format_unix(first, "+%Y-%m-%d %H:%M")),
        esc(&store::format_unix(last, "+%H:%M")),
        match floor.metres_per_px {
            Some(m) => format!("{:.1} px/m", 1.0 / m),
            None => "not set (positions in pixels)".to_string(),
        }
    );

    let views = views(floor);
    let mut summary = String::from("<table><tr><th>View</th><th>Surveyed area (approx.)</th><th>Good</th><th>Fair</th><th>Poor</th></tr>");
    let mut figures = String::new();
    for (i, v) in views.iter().enumerate() {
        let grid = grid_for(floor, v);
        let cov = heatmap::coverage(&grid);
        let _ = write!(
            summary,
            "<tr><td><a href=\"#v{i}\">{}</a></td><td>{}</td><td class=\"good\">{:.0}%</td><td class=\"fair\">{:.0}%</td><td class=\"poor\">{:.0}%</td></tr>",
            esc(&v.title),
            match floor.metres_per_px {
                Some(m) => format!("{:.0} m&sup2;", cov.measured_cells as f32 * (grid.step_px * m).powi(2)),
                None => "scale not set".to_string(),
            },
            cov.good_pct,
            cov.fair_pct,
            cov.poor_pct
        );
        let _ = write!(
            figures,
            "<section class=\"fig\" id=\"v{i}\"><h2>{}</h2><svg viewBox=\"0 0 {w} {h}\"><use href=\"#plan\"/><image width=\"{w}\" height=\"{h}\" preserveAspectRatio=\"none\" opacity=\"{HEAT_OPACITY}\" href=\"data:image/png;base64,{}\"/>",
            esc(&v.title),
            B64.encode(heat_png(&grid)?)
        );
        for (n, p) in floor.points.iter().enumerate() {
            let _ = write!(
                figures,
                "<circle cx=\"{}\" cy=\"{}\" r=\"{marker_r}\" fill=\"#1e5ac8\" stroke=\"#fff\" stroke-width=\"{}\"/><text x=\"{}\" y=\"{}\" font-size=\"{}\" text-anchor=\"middle\" fill=\"#fff\" font-family=\"sans-serif\">{}</text>",
                p.x,
                p.y,
                marker_r / 5.0,
                p.x,
                p.y + marker_r * 0.35,
                marker_r * 1.0,
                n + 1
            );
        }
        let _ = write!(figures, "</svg>{}</section>", legend());
    }
    summary.push_str("</table>");
    let _ = write!(
        o,
        "<h2>Coverage summary</h2><p class=\"meta\">Share of the surveyed area by signal: good is {GOOD_DBM:.0} dBm or better, fair is {FAIR_DBM:.0} to {GOOD_DBM:.0}, poor is weaker. Areas far from any survey point are left out.</p>{summary}{figures}"
    );

    // Survey points.
    o.push_str("<h2>Survey points</h2><table><tr><th>#</th><th>Position</th><th>Time</th><th>Dwell</th><th>APs heard</th><th>Strongest</th></tr>");
    for (n, p) in floor.points.iter().enumerate() {
        let best = p.observations.iter().max_by(|a, b| a.signal_dbm.total_cmp(&b.signal_dbm));
        let _ = write!(
            o,
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{} s</td><td>{}</td><td>{}</td></tr>",
            n + 1,
            esc(&position(floor, p)),
            esc(&store::format_unix(p.unix_secs, "+%H:%M:%S")),
            p.dwell_secs,
            p.observations.len(),
            best.map_or("none".into(), |b| format!(
                "{} {:.0} dBm",
                esc(b.ssid.as_deref().unwrap_or("<hidden>")),
                b.signal_dbm
            ))
        );
    }
    o.push_str("</table>");

    // Access points across the whole survey.
    struct Ap<'a> {
        ssid: Option<&'a str>,
        band: Band,
        channel: u32,
        width: u32,
        best: f64,
        seen_at: usize,
    }
    let mut aps: BTreeMap<&str, Ap> = BTreeMap::new();
    for p in &floor.points {
        for ob in &p.observations {
            let ap = aps.entry(&ob.bssid).or_insert(Ap {
                ssid: ob.ssid.as_deref(),
                band: ob.band(),
                channel: ob.channel,
                width: ob.width_mhz,
                best: f64::MIN,
                seen_at: 0,
            });
            ap.best = ap.best.max(ob.signal_dbm);
            ap.seen_at += 1;
        }
    }
    let mut aps: Vec<_> = aps.into_iter().collect();
    aps.sort_by(|a, b| b.1.best.total_cmp(&a.1.best));
    o.push_str("<h2>Access points seen</h2><table><tr><th>SSID</th><th>BSSID</th><th>Band</th><th>Channel</th><th>Width</th><th>Best signal</th><th>Seen at</th></tr>");
    for (bssid, a) in aps {
        let _ = write!(
            o,
            "<tr><td>{}</td><td><code>{}</code></td><td>{}</td><td>{}</td><td>{} MHz</td><td>{:.0} dBm</td><td>{} of {} points</td></tr>",
            esc(a.ssid.unwrap_or("<hidden>")),
            esc(bssid),
            a.band,
            a.channel,
            a.width,
            a.best,
            a.seen_at,
            floor.points.len()
        );
    }
    o.push_str("</table></body></html>");
    Ok(o)
}

const CSS: &str = "body{font-family:system-ui,sans-serif;max-width:1100px;margin:2rem auto;padding:0 1rem;color:#1c1c1c}\
h1{margin-bottom:.2rem}h2{margin-top:2rem;border-bottom:1px solid #ccc;padding-bottom:.2rem}\
.meta{color:#555;margin-top:.2rem}table{border-collapse:collapse;width:100%;margin:.6rem 0}\
th,td{border:1px solid #ccc;padding:.3rem .5rem;text-align:left;font-size:.9rem}th{background:#f1f1f1}\
td.good{color:#1c7a4a;font-weight:600}td.fair{color:#a87400;font-weight:600}td.poor{color:#b3261e;font-weight:600}\
.fig{break-inside:avoid;margin-top:1.5rem}.fig svg{width:100%;height:auto;border:1px solid #ccc;background:#fff}\
.legend{display:flex;align-items:center;gap:.6rem;font-size:.8rem;margin-top:.4rem}.legend .bar{height:12px;width:240px}\
a{color:#1e5ac8}@media print{body{margin:0;max-width:none}}";

#[cfg(test)]
mod tests {
    use super::*;
    use wifispanner_core::survey::Observation;

    fn point(x: f32, ssid: &str, dbm: f64) -> SurveyPoint {
        SurveyPoint {
            x,
            y: 10.0,
            unix_secs: 1_700_000_000,
            dwell_secs: 5,
            observations: vec![Observation {
                bssid: "aa:bb:cc:00:00:01".into(),
                ssid: Some(ssid.into()),
                freq_mhz: 5180,
                channel: 36,
                width_mhz: 80,
                signal_dbm: dbm,
                samples: 4,
            }],
        }
    }

    fn project(points: Vec<SurveyPoint>) -> Project {
        let mut png = Vec::new();
        image::RgbaImage::new(40, 20)
            .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        Project::new(Floor { name: "Test <plan>".into(), image: png, width_px: 40, height_px: 20, metres_per_px: Some(0.5), points })
    }

    #[test]
    fn report_has_a_heatmap_per_view_and_escapes_untrusted_text() {
        let html = build_html(&project(vec![point(5.0, "<script>alert(1)</script>", -50.0), point(30.0, "<script>alert(1)</script>", -72.0)]), "now").unwrap();
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(html.contains("Test &lt;plan&gt;"));
        assert!(html.contains("id=\"v1\""), "any-network view plus the network's own view");
        assert!(html.contains("2.5 m, 5.0 m"), "positions in metres when scaled");
    }

    #[test]
    fn a_network_heard_at_few_points_gets_no_heatmap_of_its_own() {
        let mut far = point(30.0, "Neighbour", -85.0);
        far.observations[0].ssid = Some("Other".into());
        let p = project(vec![point(5.0, "Mine", -50.0), point(6.0, "Mine", -55.0), point(7.0, "Mine", -52.0), far]);
        let titles: Vec<String> = views(&p.floor).into_iter().map(|v| v.title).collect();
        assert!(titles.iter().any(|t| t.contains("\"Mine\"")));
        assert!(!titles.iter().any(|t| t.contains("\"Other\"")));
    }

    #[test]
    fn empty_surveys_are_refused() {
        assert!(build_html(&project(vec![]), "now").is_err());
    }

    #[test]
    fn report_embeds_the_projects_own_floor_plan_bytes() {
        let mut p = project(vec![point(5.0, "Mine", -50.0)]);
        p.floor.image = (0..=255u8).chain(0..=255u8).collect();
        let html = build_html(&p, "now").unwrap();
        assert!(html.contains(&B64.encode(&p.floor.image)));
        assert_eq!(html.matches("id=\"plan\"").count(), 1, "the plan is embedded once and reused");
    }
}
