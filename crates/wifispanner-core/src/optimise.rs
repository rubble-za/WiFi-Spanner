// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 rubble-za. See the LICENSE and NOTICE files.

//! Moving the planned access points to where they cover the floor best.
//!
//! The floor is sampled on a coarse grid, and every access point may stand at the centre of any
//! cell of a second, coarser grid. For each candidate spot the signal at every sample is worked
//! out once with the same model as the prediction. The access points are then placed one at a
//! time, each where it adds the most, and moved again in turn until nothing improves.
//!
//! Coverage is scored as the mean over the samples of `min(best signal, target)`: weak spots
//! count for a lot, and signal beyond the target adds nothing, so access points spread out
//! instead of piling up on the strongest spot.

use crate::predict::{scene, signal_from, PlannedAp, Unavailable};
use crate::survey::Floor;
use wifispanner_scan::Band;

/// The signal a spot should get, in dBm; stronger adds nothing to the score.
pub const TARGET_DBM: f32 = -65.0;
/// Signal assumed where no access point reaches at all.
const FLOOR_DBM: f32 = -100.0;
/// Samples and candidate spots along the longer side of the plan.
const SAMPLES_ACROSS: f32 = 40.0;
const SPOTS_ACROSS: f32 = 28.0;
const MAX_PASSES: usize = 8;
/// Access points keep at least this far (metres) from a wall.
const WALL_CLEARANCE_M: f32 = 0.5;

/// New positions (plan pixels) for the enabled access points of `floor.aps` that have a radio on
/// `band`, as `(index in floor.aps, [x, y])`. Only those that move are listed; an empty list
/// means the plan is already as good as it gets.
pub fn optimise(floor: &Floor, band: Band, rx_height_m: f32) -> Result<Vec<(usize, [f32; 2])>, Unavailable> {
    let mpp = floor.metres_per_px.filter(|m| m.is_finite() && *m > 0.0).ok_or(Unavailable::NoScale)?;
    let movers: Vec<(usize, &PlannedAp, f32)> = floor
        .aps
        .iter()
        .enumerate()
        .filter(|(_, a)| a.enabled)
        .filter_map(|(i, a)| a.eirp_dbm(band).map(|e| (i, a, e)))
        .collect();
    if movers.is_empty() {
        return Err(Unavailable::NoAps);
    }
    let longest = floor.width_px.max(floor.height_px).max(1) as f32;
    let grid = |across: f32| {
        let step = (longest / across).max(1.0);
        let (w, h) = (((floor.width_px as f32 / step).ceil() as usize).max(1), ((floor.height_px as f32 / step).ceil() as usize).max(1));
        (0..w * h).map(|i| [((i % w) as f32 + 0.5) * step, ((i / w) as f32 + 0.5) * step]).collect::<Vec<[f32; 2]>>()
    };
    let samples = grid(SAMPLES_ACROSS);
    let mut spots = grid(SPOTS_ACROSS);
    // Not on or right beside a wall (unless that leaves nowhere to stand).
    let clear = WALL_CLEARANCE_M / mpp;
    let off_walls: Vec<[f32; 2]> = spots.iter().copied().filter(|p| floor.walls.iter().all(|w| distance_to_segment(*p, w.a, w.b) >= clear)).collect();
    if !off_walls.is_empty() {
        spots = off_walls;
    }
    let walls = scene(floor, mpp, band);
    let at = |ap: &PlannedAp, eirp: f32, xy: [f32; 2], s: [f32; 2]| signal_from(ap, eirp, [xy[0] * mpp, xy[1] * mpp], [s[0] * mpp, s[1] * mpp, rx_height_m], &walls, band);

    // signal[m][spot][sample]
    let signals: Vec<Vec<Vec<f32>>> = std::thread::scope(|scope| {
        let handles: Vec<_> = movers
            .iter()
            .map(|(_, ap, eirp)| {
                let (spots, samples, at) = (&spots, &samples, &at);
                scope.spawn(move || spots.iter().map(|&p| samples.iter().map(|&s| at(ap, *eirp, p, s)).collect()).collect())
            })
            .collect();
        handles.into_iter().map(|h| h.join().expect("optimiser thread")).collect()
    });

    let score = |best: &[f32]| best.iter().map(|b| b.clamp(FLOOR_DBM, TARGET_DBM)).sum::<f32>() / best.len() as f32;
    let best_of = |placed: &[Option<usize>], skip: Option<usize>| -> Vec<f32> {
        let mut best = vec![FLOOR_DBM; samples.len()];
        for (m, spot) in placed.iter().enumerate() {
            if let (Some(spot), true) = (spot, Some(m) != skip) {
                for (b, s) in best.iter_mut().zip(&signals[m][*spot]) {
                    *b = b.max(*s);
                }
            }
        }
        best
    };
    // Where they stand now, for comparison.
    let current: Vec<f32> = {
        let mut best = vec![FLOOR_DBM; samples.len()];
        for (_, ap, eirp) in &movers {
            for (b, s) in best.iter_mut().zip(&samples) {
                *b = b.max(at(ap, *eirp, [ap.x, ap.y], *s));
            }
        }
        best
    };

    // Strongest first, each where it adds the most; then each moved in turn while that helps.
    let mut order: Vec<usize> = (0..movers.len()).collect();
    order.sort_by(|a, b| movers[*b].2.total_cmp(&movers[*a].2));
    let mut placed: Vec<Option<usize>> = vec![None; movers.len()];
    let choose = |placed: &[Option<usize>], m: usize| -> usize {
        let others = best_of(placed, Some(m));
        (0..spots.len())
            .map(|c| (c, score(&others.iter().zip(&signals[m][c]).map(|(o, s)| o.max(*s)).collect::<Vec<_>>())))
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map_or(0, |(c, _)| c)
    };
    for &m in &order {
        placed[m] = Some(choose(&placed, m));
    }
    for _ in 0..MAX_PASSES {
        let mut moved = false;
        for &m in &order {
            let was = placed[m];
            let now = Some(choose(&placed, m));
            let (a, b) = (score(&best_of(&placed, None)), {
                let mut t = placed.clone();
                t[m] = now;
                score(&best_of(&t, None))
            });
            if now != was && b > a + 1e-4 {
                placed[m] = now;
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
    // Keep what is there unless the search found something clearly better.
    if score(&best_of(&placed, None)) <= score(&current) + 0.05 {
        return Ok(Vec::new());
    }
    Ok(movers.iter().zip(&placed).filter_map(|((i, _, _), spot)| spot.map(|s| (*i, spots[s]))).collect())
}

/// Distance from `p` to the segment `a`-`b`.
fn distance_to_segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = dx * dx + dy * dy;
    let t = if len2 < 1e-9 { 0.0 } else { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0) };
    ((p[0] - (a[0] + t * dx)).powi(2) + (p[1] - (a[1] + t * dy)).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::floors::Placement;
    use crate::predict::predict;

    fn floor(aps: Vec<PlannedAp>) -> Floor {
        Floor {
            name: "t".into(),
            image: vec![],
            width_px: 400,
            height_px: 200,
            metres_per_px: Some(0.1),
            points: vec![],
            walls: vec![],
            ceiling_m: 3.0,
            placement: Placement::default(),
            aps,
            survey_aps: vec![],
            areas: vec![],
        }
    }

    fn mean_clamped(f: &Floor) -> f32 {
        let g = predict(f, Band::Ghz5, 1.0).unwrap();
        g.dbm.iter().map(|d| d.clamp(FLOOR_DBM, TARGET_DBM)).sum::<f32>() / g.dbm.len() as f32
    }

    #[test]
    fn access_points_stacked_in_a_corner_spread_out() {
        let mut f = floor(vec![PlannedAp::new("a".into(), 10.0, 10.0), PlannedAp::new("b".into(), 12.0, 12.0)]);
        let before = mean_clamped(&f);
        let moves = optimise(&f, Band::Ghz5, 1.0).unwrap();
        assert_eq!(moves.len(), 2);
        for (i, p) in moves {
            f.aps[i].x = p[0];
            f.aps[i].y = p[1];
        }
        assert!(mean_clamped(&f) > before + 1.0 && mean_clamped(&f) > TARGET_DBM - 0.5, "{} vs {before}", mean_clamped(&f));
        let (a, b) = (&f.aps[0], &f.aps[1]);
        assert!(((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt() > 100.0, "they end up apart");
    }

    #[test]
    fn access_points_keep_off_the_walls() {
        use crate::walls::Wall;
        use crate::Material;
        let mut f = floor(vec![PlannedAp::new("a".into(), 20.0, 20.0)]);
        // A thin glass wall down the middle, where the best spot for one AP would be.
        f.walls.push(Wall { height_m: None, ..Wall::new([200.0, 0.0], [200.0, 200.0], Material::Glass) });
        let moves = optimise(&f, Band::Ghz5, 1.0).unwrap();
        let [x, y] = moves.first().expect("it moves").1;
        assert!(distance_to_segment([x, y], [200.0, 0.0], [200.0, 200.0]) >= WALL_CLEARANCE_M / 0.1, "{x},{y}");
    }

    #[test]
    fn a_good_layout_is_left_alone_and_needs_a_scale() {
        let f = floor(vec![PlannedAp::new("a".into(), 200.0, 100.0)]);
        let moves = optimise(&f, Band::Ghz5, 1.0).unwrap();
        assert!(moves.is_empty() || moves.len() == 1);
        let mut none = floor(vec![]);
        assert_eq!(optimise(&none, Band::Ghz5, 1.0), Err(Unavailable::NoAps));
        none.metres_per_px = None;
        assert_eq!(optimise(&none, Band::Ghz5, 1.0), Err(Unavailable::NoScale));
    }
}
