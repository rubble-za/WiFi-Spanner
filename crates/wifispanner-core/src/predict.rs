// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 rubble-za. See the LICENSE and NOTICE files.

//! Predicted coverage from planned access points and the walls drawn on a
//! floor: free-space loss at one metre, a path-loss exponent beyond that, and
//! the loss of each wall the straight line from the AP to the receiver
//! passes through. A wall only counts where the line is lower than the wall,
//! so an AP mounted above shelving reaches over it.
//!
//! These are textbook starting values. Calibrating them against a real
//! survey is a later step, so treat predictions as estimates.

use serde::{Deserialize, Serialize};
use wifispanner_scan::Band;

use crate::heatmap::Grid;
use crate::survey::Floor;

/// An access point placed in the planner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlannedAp {
    pub name: String,
    /// Position in floor plan pixels.
    pub x: f32,
    pub y: f32,
    /// Mounting height above the floor, in metres.
    pub height_m: f32,
    pub tx_dbm: f32,
    pub gain_dbi: f32,
    pub enabled: bool,
}

impl PlannedAp {
    pub const DEFAULT_HEIGHT_M: f32 = 2.7;
    pub const DEFAULT_TX_DBM: f32 = 20.0;
    pub const DEFAULT_GAIN_DBI: f32 = 3.0;

    pub fn new(name: String, x: f32, y: f32) -> Self {
        PlannedAp { name, x, y, height_m: Self::DEFAULT_HEIGHT_M, tx_dbm: Self::DEFAULT_TX_DBM, gain_dbi: Self::DEFAULT_GAIN_DBI, enabled: true }
    }
}

/// Path-loss exponent beyond the first metre; indoors it runs from about 2 to 4.
pub const PATH_LOSS_EXPONENT: f32 = 3.0;
/// Receivers are modelled this high above the floor unless told otherwise (a phone or laptop in use).
pub const DEFAULT_RX_HEIGHT_M: f32 = 1.0;
/// Grid resolution along the longer side of the plan.
pub const GRID_CELLS: usize = 160;

/// A frequency standing for each band.
pub fn band_mhz(band: Band) -> f32 {
    match band {
        Band::Ghz2_4 => 2437.0,
        Band::Ghz5 => 5500.0,
        Band::Ghz6 => 6000.0,
    }
}

/// Free-space loss at one metre.
fn loss_at_1m(band: Band) -> f32 {
    20.0 * band_mhz(band).log10() - 27.56
}

fn wall_loss(material: crate::Material, band: Band) -> f32 {
    let a = material.attenuation();
    match band {
        Band::Ghz2_4 => a.ghz2_4,
        Band::Ghz5 => a.ghz5,
        Band::Ghz6 => a.ghz6,
    }
}

/// A wall in metres, ready for line tests.
struct MetreWall {
    a: [f32; 2],
    b: [f32; 2],
    height: f32,
    loss: f32,
}

fn metre_walls(floor: &Floor, mpp: f32, band: Band) -> Vec<MetreWall> {
    floor
        .walls
        .iter()
        .map(|w| MetreWall {
            a: [w.a[0] * mpp, w.a[1] * mpp],
            b: [w.b[0] * mpp, w.b[1] * mpp],
            height: w.effective_height_m(floor.ceiling_m),
            loss: wall_loss(w.material, band),
        })
        .collect()
}

/// Loss in dB from the walls the line `from` to `to` (x, y, height in metres) passes through.
fn walls_between(walls: &[MetreWall], from: [f32; 3], to: [f32; 3]) -> f32 {
    let (rx, ry) = (to[0] - from[0], to[1] - from[1]);
    let mut loss = 0.0;
    for w in walls {
        let (sx, sy) = (w.b[0] - w.a[0], w.b[1] - w.a[1]);
        let denom = rx * sy - ry * sx;
        if denom.abs() < 1e-9 {
            continue;
        }
        let (qx, qy) = (w.a[0] - from[0], w.a[1] - from[1]);
        let t = (qx * sy - qy * sx) / denom;
        let u = (qx * ry - qy * rx) / denom;
        if !(0.0..=1.0).contains(&t) || !(0.0..=1.0).contains(&u) {
            continue;
        }
        let z = from[2] + (to[2] - from[2]) * t;
        if z <= w.height {
            loss += w.loss;
        }
    }
    loss
}

/// Signal in dBm at `rx` (x, y, height in metres) from one AP.
fn signal_from(ap: &PlannedAp, ap_xy_m: [f32; 2], rx: [f32; 3], walls: &[MetreWall], band: Band) -> f32 {
    let from = [ap_xy_m[0], ap_xy_m[1], ap.height_m];
    let d = ((rx[0] - from[0]).powi(2) + (rx[1] - from[1]).powi(2) + (rx[2] - from[2]).powi(2)).sqrt().max(1.0);
    ap.tx_dbm + ap.gain_dbi - loss_at_1m(band) - 10.0 * PATH_LOSS_EXPONENT * d.log10() - walls_between(walls, from, rx)
}

/// Why a prediction cannot be made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unavailable {
    NoScale,
    NoAps,
}

impl std::fmt::Display for Unavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Unavailable::NoScale => "Set the plan's scale first, so distances are real.",
            Unavailable::NoAps => "Place an access point to see predicted coverage.",
        })
    }
}

/// The strongest signal from any enabled AP at each cell of a grid over the
/// floor, for receivers `rx_height_m` above the floor.
pub fn predict(floor: &Floor, band: Band, rx_height_m: f32) -> Result<Grid, Unavailable> {
    let mpp = floor.metres_per_px.filter(|m| m.is_finite() && *m > 0.0).ok_or(Unavailable::NoScale)?;
    let aps: Vec<&PlannedAp> = floor.aps.iter().filter(|a| a.enabled).collect();
    if aps.is_empty() {
        return Err(Unavailable::NoAps);
    }
    let longest = floor.width_px.max(floor.height_px).max(1) as f32;
    let step = (longest / GRID_CELLS as f32).max(1.0);
    let w = ((floor.width_px as f32 / step).ceil() as usize).max(1);
    let h = ((floor.height_px as f32 / step).ceil() as usize).max(1);
    let walls = metre_walls(floor, mpp, band);
    let ap_m: Vec<[f32; 2]> = aps.iter().map(|a| [a.x * mpp, a.y * mpp]).collect();
    let mut dbm = vec![0.0f32; w * h];
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(h);
    let rows_per = h.div_ceil(threads);
    std::thread::scope(|scope| {
        for (chunk_index, rows) in dbm.chunks_mut(rows_per * w).enumerate() {
            let (aps, ap_m, walls) = (&aps, &ap_m, &walls);
            scope.spawn(move || {
                for (i, cell) in rows.iter_mut().enumerate() {
                    let index = chunk_index * rows_per * w + i;
                    let (gx, gy) = (index % w, index / w);
                    let rx = [(gx as f32 + 0.5) * step * mpp, (gy as f32 + 0.5) * step * mpp, rx_height_m];
                    *cell = aps.iter().zip(ap_m).map(|(ap, xy)| signal_from(ap, *xy, rx, walls, band)).fold(f32::MIN, f32::max);
                }
            });
        }
    });
    Ok(Grid { w, h, dbm, confidence: vec![1.0; w * h], step_px: step })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::floors::Placement;
    use crate::walls::Wall;
    use crate::Material;

    fn floor(aps: Vec<PlannedAp>, walls: Vec<Wall>) -> Floor {
        Floor {
            name: "t".into(),
            image: vec![],
            width_px: 200,
            height_px: 100,
            metres_per_px: Some(0.1),
            points: vec![],
            walls,
            ceiling_m: 3.0,
            placement: Placement::default(),
            aps,
        }
    }

    fn ap(x: f32, y: f32, height: f32) -> PlannedAp {
        PlannedAp { height_m: height, ..PlannedAp::new("ap".into(), x, y) }
    }

    fn at(g: &Grid, mpp: f32, x_m: f32, y_m: f32) -> f32 {
        let (gx, gy) = ((x_m / mpp / g.step_px) as usize, (y_m / mpp / g.step_px) as usize);
        g.dbm[gy * g.w + gx]
    }

    #[test]
    fn free_space_loss_at_one_metre_matches_the_textbook() {
        assert!((loss_at_1m(Band::Ghz2_4) - 40.2).abs() < 0.2);
        assert!((loss_at_1m(Band::Ghz5) - 47.3).abs() < 0.2);
        assert!(loss_at_1m(Band::Ghz6) > loss_at_1m(Band::Ghz5));
    }

    #[test]
    fn signal_falls_with_distance_and_is_strongest_under_the_ap() {
        let f = floor(vec![ap(100.0, 50.0, 2.7)], vec![]);
        let g = predict(&f, Band::Ghz5, 1.0).unwrap();
        let (near, mid, far) = (at(&g, 0.1, 10.0, 5.0), at(&g, 0.1, 15.0, 5.0), at(&g, 0.1, 19.5, 5.0));
        assert!(near > mid && mid > far, "{near} {mid} {far}");
        // 20 dBm + 3 dBi - 47.3 dB at 1 m, about 1.7 m away in 3D.
        assert!(near < 20.0 + 3.0 - 47.3 + 1.0 && near > -40.0, "{near}");
    }

    #[test]
    fn a_wall_between_costs_its_attenuation() {
        let clear = floor(vec![ap(20.0, 50.0, 2.7)], vec![]);
        let wall = Wall { height_m: None, ..Wall::new([100.0, 0.0], [100.0, 100.0], Material::Concrete) };
        let walled = floor(vec![ap(20.0, 50.0, 2.7)], vec![wall]);
        let (a, b) = (predict(&clear, Band::Ghz5, 1.0).unwrap(), predict(&walled, Band::Ghz5, 1.0).unwrap());
        let drop = at(&a, 0.1, 15.0, 5.0) - at(&b, 0.1, 15.0, 5.0);
        assert!((drop - Material::Concrete.attenuation().ghz5).abs() < 0.01, "{drop}");
        assert!((at(&a, 0.1, 5.0, 5.0) - at(&b, 0.1, 5.0, 5.0)).abs() < 1e-3, "same side of the wall is unaffected");
    }

    #[test]
    fn an_ap_mounted_above_a_low_wall_reaches_over_it() {
        let shelf = Wall { height_m: Some(2.0), ..Wall::new([100.0, 0.0], [100.0, 100.0], Material::Metal) };
        let low_ap = floor(vec![ap(20.0, 50.0, 1.5)], vec![shelf]);
        let high_ap = floor(vec![ap(20.0, 50.0, 6.0)], vec![shelf]);
        let (l, h) = (predict(&low_ap, Band::Ghz5, 1.0).unwrap(), predict(&high_ap, Band::Ghz5, 1.0).unwrap());
        let behind = (at(&l, 0.1, 15.0, 5.0), at(&h, 0.1, 15.0, 5.0));
        // The low AP's line stays under the 2 m shelf; the high AP's line crosses it above 2 m.
        assert!(behind.0 < -60.0, "blocked: {}", behind.0);
        assert!(behind.1 > behind.0 + 20.0, "over the top: {} vs {}", behind.1, behind.0);
    }

    #[test]
    fn the_strongest_enabled_ap_wins_and_disabled_ones_are_ignored() {
        let mut off = ap(20.0, 50.0, 2.7);
        off.enabled = false;
        let only_far = floor(vec![off, ap(180.0, 50.0, 2.7)], vec![]);
        let both = floor(vec![ap(20.0, 50.0, 2.7), ap(180.0, 50.0, 2.7)], vec![]);
        let (a, b) = (predict(&only_far, Band::Ghz5, 1.0).unwrap(), predict(&both, Band::Ghz5, 1.0).unwrap());
        assert!(at(&b, 0.1, 2.0, 5.0) > at(&a, 0.1, 2.0, 5.0) + 10.0);
        assert!((at(&a, 0.1, 18.0, 5.0) - at(&b, 0.1, 18.0, 5.0)).abs() < 1e-3);
    }

    #[test]
    fn it_says_why_it_cannot_predict() {
        let mut f = floor(vec![ap(20.0, 50.0, 2.7)], vec![]);
        f.metres_per_px = None;
        assert_eq!(predict(&f, Band::Ghz5, 1.0).unwrap_err(), Unavailable::NoScale);
        assert_eq!(predict(&floor(vec![], vec![]), Band::Ghz5, 1.0).unwrap_err(), Unavailable::NoAps);
    }
}
