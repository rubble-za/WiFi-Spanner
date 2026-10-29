// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 rubble-za. See the LICENSE and NOTICE files.

//! Walls drawn on a floor plan, and the snapping used while drawing them.

use serde::{Deserialize, Serialize};

use crate::Material;

/// Ceiling height assumed for a floor, in metres.
pub const DEFAULT_CEILING_M: f32 = 3.0;

/// A straight wall between two points in floor plan pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Wall {
    pub a: [f32; 2],
    pub b: [f32; 2],
    pub material: Material,
    /// Height in metres; `None` is the full height of the floor. A wall lower
    /// than an access point lets its signal pass over the top.
    pub height_m: Option<f32>,
}

impl Wall {
    pub fn new(a: [f32; 2], b: [f32; 2], material: Material) -> Self {
        Wall { a, b, material, height_m: material.default_height_m() }
    }

    pub fn length_px(&self) -> f32 {
        (self.a[0] - self.b[0]).hypot(self.a[1] - self.b[1])
    }

    /// The wall's height given the floor's ceiling height.
    pub fn effective_height_m(&self, ceiling_m: f32) -> f32 {
        self.height_m.unwrap_or(ceiling_m).clamp(0.0, ceiling_m.max(0.0))
    }

    /// Distance in pixels from `p` to the nearest point of the wall.
    pub fn distance_to(&self, p: [f32; 2]) -> f32 {
        let (dx, dy) = (self.b[0] - self.a[0], self.b[1] - self.a[1]);
        let len2 = dx * dx + dy * dy;
        let t = if len2 == 0.0 { 0.0 } else { (((p[0] - self.a[0]) * dx + (p[1] - self.a[1]) * dy) / len2).clamp(0.0, 1.0) };
        (p[0] - (self.a[0] + t * dx)).hypot(p[1] - (self.a[1] + t * dy))
    }
}

/// Where a wall being drawn should land.
///
/// The end of a wall snaps to an existing wall end within `radius` pixels.
/// Failing that, when there is a `start`, the direction snaps to the nearest
/// multiple of 15 degrees if within a few degrees of it, which keeps walls
/// square and straight without forcing it.
pub fn snap(p: [f32; 2], start: Option<[f32; 2]>, walls: &[Wall], radius: f32) -> [f32; 2] {
    let nearest = walls
        .iter()
        .flat_map(|w| [w.a, w.b])
        .map(|e| (e, (e[0] - p[0]).hypot(e[1] - p[1])))
        .filter(|(_, d)| *d <= radius)
        .min_by(|x, y| x.1.total_cmp(&y.1));
    if let Some((e, _)) = nearest {
        return e;
    }
    let Some(s) = start else { return p };
    let (dx, dy) = (p[0] - s[0], p[1] - s[1]);
    let len = dx.hypot(dy);
    if len < 1.0 {
        return p;
    }
    let step = 15f32.to_radians();
    let angle = dy.atan2(dx);
    let snapped = (angle / step).round() * step;
    if (angle - snapped).abs() <= 4f32.to_radians() {
        [s[0] + len * snapped.cos(), s[1] + len * snapped.sin()]
    } else {
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wall(a: [f32; 2], b: [f32; 2]) -> Wall {
        Wall::new(a, b, Material::Concrete)
    }

    #[test]
    fn new_walls_take_the_materials_default_height() {
        assert_eq!(Wall::new([0.0; 2], [1.0, 0.0], Material::Metal).height_m, Some(2.0));
        assert_eq!(Wall::new([0.0; 2], [1.0, 0.0], Material::Glass).height_m, None);
    }

    #[test]
    fn height_is_the_ceiling_unless_set_and_never_above_it() {
        let mut w = wall([0.0; 2], [1.0, 0.0]);
        assert_eq!(w.effective_height_m(3.0), 3.0);
        w.height_m = Some(1.5);
        assert_eq!(w.effective_height_m(3.0), 1.5);
        w.height_m = Some(9.0);
        assert_eq!(w.effective_height_m(3.0), 3.0);
    }

    #[test]
    fn distance_is_to_the_segment_not_the_infinite_line() {
        let w = wall([0.0, 0.0], [10.0, 0.0]);
        assert_eq!(w.distance_to([5.0, 3.0]), 3.0);
        assert_eq!(w.distance_to([14.0, 3.0]), 5.0);
        assert_eq!(wall([2.0, 2.0], [2.0, 2.0]).distance_to([5.0, 6.0]), 5.0);
    }

    #[test]
    fn ends_snap_to_existing_wall_ends_within_the_radius() {
        let walls = [wall([100.0, 100.0], [200.0, 100.0])];
        assert_eq!(snap([104.0, 97.0], None, &walls, 8.0), [100.0, 100.0]);
        assert_eq!(snap([120.0, 97.0], None, &walls, 8.0), [120.0, 97.0], "too far from either end");
    }

    #[test]
    fn directions_snap_to_15_degrees_only_when_close() {
        let start = Some([0.0, 0.0]);
        let s = snap([100.0, 3.0], start, &[], 8.0);
        assert!((s[1]).abs() < 1e-3 && (s[0] - 100.0).abs() < 0.2, "nearly horizontal becomes horizontal, got {s:?}");
        let off = [100.0, 12.0];
        assert_eq!(snap(off, start, &[], 8.0), off, "a deliberate angle is left alone");
        let diag = snap([100.0, 98.0], start, &[], 8.0);
        assert!((diag[0] - diag[1]).abs() < 0.5, "45 degrees is a multiple of 15, got {diag:?}");
    }

    #[test]
    fn walls_round_trip_through_json() {
        let w = Wall { a: [1.0, 2.0], b: [3.0, 4.0], material: Material::ThickConcrete, height_m: Some(1.2) };
        let back: Wall = serde_json::from_str(&serde_json::to_string(&w).unwrap()).unwrap();
        assert_eq!(w, back);
    }
}
