// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 rubble-za. See the LICENSE and NOTICE files.

//! Finding the walls, doors and windows on a floor plan picture.
//!
//! Walls are drawn as solid dark bands. Every dark pixel is measured across (how many dark pixels
//! stand above and below it, or beside it) and the pixels that belong to a band of wall thickness
//! are joined up into rectangles; each rectangle becomes a wall along its middle. Where two walls
//! on one line stop a door's or window's width apart, the gap is looked at: lines along it are a
//! window, a swing arc beside it is a door, and a bare gap is an opening that gets no wall.

use wifispanner_core::walls::Wall;
use wifispanner_core::Material;

/// Walls are at least this thick and at most this thick, in metres.
const WALL_MIN_M: f32 = 0.08;
const WALL_MAX_M: f32 = 0.6;
/// A wall is at least this long, in metres.
const WALL_LONG_M: f32 = 0.3;
/// A gap this wide (metres) between two walls on one line may be a door or a window.
const GAP_MIN_M: f32 = 0.5;
const GAP_MAX_M: f32 = 2.8;
/// Pixels darker than this (0-255) are wall; darker than `LINE` are any drawn line.
const WALL_DARK: u32 = 110;
const LINE_DARK: u32 = 200;

#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub walls: Vec<Wall>,
    pub doors: usize,
    pub windows: usize,
}

/// A picture as two masks: the dark of walls and the dark of any line.
struct Masks {
    w: usize,
    h: usize,
    wall: Vec<bool>,
    line: Vec<bool>,
}

impl Masks {
    fn new(rgb: &[u8], (w, h): (u32, u32)) -> Masks {
        let (w, h) = (w as usize, h as usize);
        let lum = |i: usize| (rgb[i * 3] as u32 * 30 + rgb[i * 3 + 1] as u32 * 59 + rgb[i * 3 + 2] as u32 * 11) / 100;
        Masks { w, h, wall: (0..w * h).map(|i| lum(i) < WALL_DARK).collect(), line: (0..w * h).map(|i| lum(i) < LINE_DARK).collect() }
    }

    /// The same picture on its side: x and y swapped.
    fn transposed(&self) -> Masks {
        let t = |m: &[bool]| (0..self.w * self.h).map(|i| m[(i % self.h) * self.w + i / self.h]).collect();
        Masks { w: self.h, h: self.w, wall: t(&self.wall), line: t(&self.line) }
    }

    fn wall_at(&self, x: isize, y: isize) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h && self.wall[y as usize * self.w + x as usize]
    }
}

/// A horizontal wall: the middle line `y`, from `x0` to `x1`, `thick` across.
#[derive(Debug, Clone, Copy)]
struct Band {
    y: f32,
    x0: f32,
    x1: f32,
    thick: f32,
}

/// Horizontal walls of the picture (run the same on the transposed picture for vertical ones).
fn horizontal_bands(m: &Masks, min_px: usize, max_px: usize, long_px: usize) -> Vec<Band> {
    // How many dark pixels stand in a column run through each pixel.
    let mut across = vec![0u16; m.w * m.h];
    for x in 0..m.w {
        let mut y = 0;
        while y < m.h {
            if !m.wall[y * m.w + x] {
                y += 1;
                continue;
            }
            let start = y;
            while y < m.h && m.wall[y * m.w + x] {
                y += 1;
            }
            let len = (y - start).min(u16::MAX as usize) as u16;
            for yy in start..y {
                across[yy * m.w + x] = len;
            }
        }
    }
    let candidate = |i: usize| m.wall[i] && (min_px..=max_px).contains(&(across[i] as usize));
    // Join candidate pixels into pieces, and keep the ones shaped like a wall.
    let mut seen = vec![false; m.w * m.h];
    let mut bands = Vec::new();
    let mut stack = Vec::new();
    for start in 0..m.w * m.h {
        if seen[start] || !candidate(start) {
            continue;
        }
        // The piece's pixels per row: a wall is the stack of rows holding most of them, so a
        // door's arc or leaf touching the end of a wall does not spoil it.
        let mut rows: std::collections::BTreeMap<usize, (usize, usize, usize)> = std::collections::BTreeMap::new();
        stack.push(start);
        seen[start] = true;
        while let Some(i) = stack.pop() {
            let (x, y) = (i % m.w, i / m.w);
            let r = rows.entry(y).or_insert((0, usize::MAX, 0));
            *r = (r.0 + 1, r.1.min(x), r.2.max(x));
            for (nx, ny) in [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)] {
                if nx < m.w && ny < m.h && !seen[ny * m.w + nx] && candidate(ny * m.w + nx) {
                    seen[ny * m.w + nx] = true;
                    stack.push(ny * m.w + nx);
                }
            }
        }
        let most = rows.values().map(|r| r.0).max().unwrap_or(0);
        if most < long_px {
            continue;
        }
        // The first run of rows, each at least half as full as the fullest.
        let mut core: Vec<(usize, (usize, usize, usize))> = Vec::new();
        for (&y, &r) in &rows {
            if r.0 * 2 >= most && core.last().is_none_or(|(py, _)| *py + 1 == y) {
                core.push((y, r));
            } else if r.0 * 2 >= most && core.len() < min_px {
                core = vec![(y, r)];
            } else if !core.is_empty() && r.0 * 2 < most && core.len() >= min_px {
                break;
            }
        }
        let thick = core.len();
        if !(min_px..=max_px).contains(&thick) {
            continue;
        }
        let (x0, x1) = (core.iter().map(|(_, r)| r.1).min().unwrap_or(0), core.iter().map(|(_, r)| r.2).max().unwrap_or(0));
        let (bw, area) = (x1 - x0 + 1, core.iter().map(|(_, r)| r.0).sum::<usize>());
        if bw >= long_px && bw >= thick * 2 && area * 100 >= bw * thick * 85 {
            let (y0, y1) = (core[0].0, core[core.len() - 1].0);
            bands.push(Band { y: (y0 + y1) as f32 / 2.0 + 0.5, x0: x0 as f32, x1: x1 as f32 + 1.0, thick: thick as f32 });
        }
    }
    bands
}

/// Extend a band's ends to the middle of any wall it runs into.
fn join_ends(m: &Masks, bands: &mut [Band]) {
    for b in bands {
        let y = b.y as isize;
        let reach = (b.thick / 2.0) as isize;
        if m.wall_at(b.x1 as isize + 1, y) || m.wall_at(b.x1 as isize + 2, y) {
            b.x1 += reach as f32;
        }
        if m.wall_at(b.x0 as isize - 2, y) || m.wall_at(b.x0 as isize - 1, y) {
            b.x0 -= reach as f32;
        }
    }
}

/// What a gap between two walls on one line holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Gap {
    Window,
    Door,
    Opening,
}

/// Look at the gap on the line `y` from `x0` to `x1`, walls `thick` across.
fn classify(m: &Masks, y: f32, x0: f32, x1: f32, thick: f32) -> Gap {
    let (xa, xb) = (x0.ceil() as isize + 2, x1.floor() as isize - 2);
    let len = (xb - xa).max(1) as f32;
    let line = |x: isize, yy: isize| x >= 0 && yy >= 0 && (x as usize) < m.w && (yy as usize) < m.h && m.line[yy as usize * m.w + x as usize];
    // Lines running along the gap, within the wall's thickness.
    let half = (thick / 2.0).ceil() as isize + 2;
    let along = (-half..=half).filter(|dy| ((xa..xb).filter(|&x| line(x, y as isize + dy)).count() as f32) >= len * 0.8).count();
    // Rows of one line can be several pixels thick; separate lines are what count.
    let mut lines = 0;
    let mut prev = false;
    for dy in -half..=half {
        let on = ((xa..xb).filter(|&x| line(x, y as isize + dy)).count() as f32) >= len * 0.8;
        if on && !prev {
            lines += 1;
        }
        prev = on;
    }
    if lines >= 2 && along >= 2 {
        return Gap::Window;
    }
    // A swing arc and leaf: ink in a square as wide as the gap beside the wall, on either side.
    let reach = len as isize;
    for side in [-1isize, 1] {
        let start = y as isize + side * (half + 1);
        let ink = (1..=reach).map(|d| (xa..xb).filter(|&x| line(x, start + side * d)).count()).sum::<usize>();
        if ink as f32 >= len * 1.2 {
            return Gap::Door;
        }
    }
    Gap::Opening
}

/// Find walls, doors and windows in an RGB picture drawn at `metres_per_px`.
pub fn scan(rgb: &[u8], size: (u32, u32), metres_per_px: f32) -> Found {
    let px = |m: f32| m / metres_per_px;
    let (min_px, max_px, long_px) = ((px(WALL_MIN_M).round() as usize).max(3), px(WALL_MAX_M).round() as usize, px(WALL_LONG_M).round() as usize);
    let masks = Masks::new(rgb, size);
    let mut walls = Vec::new();
    let (mut doors, mut windows) = (0, 0);
    for vertical in [false, true] {
        let turned;
        let m = if vertical {
            turned = masks.transposed();
            &turned
        } else {
            &masks
        };
        let mut bands = horizontal_bands(m, min_px, max_px, long_px);
        join_ends(m, &mut bands);
        // Back to picture coordinates.
        let point = |x: f32, y: f32| if vertical { [y, x] } else { [x, y] };
        for b in &bands {
            walls.push(Wall::new(point(b.x0, b.y), point(b.x1, b.y), Material::Concrete));
        }
        // Gaps between walls on one line.
        let mut order: Vec<usize> = (0..bands.len()).collect();
        order.sort_by(|&a, &b| bands[a].y.total_cmp(&bands[b].y).then(bands[a].x0.total_cmp(&bands[b].x0)));
        for (i, &a) in order.iter().enumerate() {
            let left = bands[a];
            let next = order[i + 1..]
                .iter()
                .map(|&j| bands[j])
                .filter(|r| (r.y - left.y).abs() <= left.thick.max(r.thick) / 2.0 && r.x0 >= left.x1)
                .min_by(|p, q| p.x0.total_cmp(&q.x0));
            let Some(right) = next else { continue };
            let gap_m = (right.x0 - left.x1) * metres_per_px;
            if !(GAP_MIN_M..=GAP_MAX_M).contains(&gap_m) {
                continue;
            }
            let y = (left.y + right.y) / 2.0;
            let thick = left.thick.max(right.thick);
            match classify(m, y, left.x1, right.x0, thick) {
                Gap::Window => {
                    windows += 1;
                    walls.push(Wall::new(point(left.x1, y), point(right.x0, y), Material::Window));
                }
                Gap::Door => {
                    doors += 1;
                    walls.push(Wall::new(point(left.x1, y), point(right.x0, y), Material::Wood));
                }
                Gap::Opening => {}
            }
        }
    }
    Found { walls, doors, windows }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A white picture with a dark rectangle ring of walls and gaps cut into it.
    fn picture() -> (Vec<u8>, (u32, u32)) {
        let (w, h) = (600usize, 400usize);
        let mut img = vec![255u8; w * h * 3];
        let mut fill = |x0: usize, y0: usize, x1: usize, y1: usize, v: u8| {
            for y in y0..y1 {
                for x in x0..x1 {
                    img[(y * w + x) * 3..][..3].fill(v);
                }
            }
        };
        // 66 px per metre: walls 13 px thick. Outer ring from (100,100) to (500,300).
        fill(100, 100, 500, 113, 40); // top
        fill(100, 287, 500, 300, 40); // bottom
        fill(100, 100, 113, 300, 40); // left
        fill(487, 100, 500, 300, 40); // right
        // A door gap in the top wall: x 250..320 with a swing arc drawn below.
        fill(250, 100, 320, 113, 255);
        for i in 0..70 {
            let a = i as f32 / 70.0 * std::f32::consts::FRAC_PI_2;
            let (x, y) = (250.0 + 70.0 * a.cos(), 113.0 + 70.0 * a.sin());
            fill(x as usize, y as usize, x as usize + 2, y as usize + 2, 60);
        }
        fill(250, 113, 252, 183, 60);
        // A window gap in the bottom wall: x 350..440 with three lines across it.
        fill(350, 287, 440, 300, 255);
        for y in [289, 293, 297] {
            fill(350, y, 440, y + 1, 70);
        }
        (img, (w as u32, h as u32))
    }

    #[test]
    fn a_ring_of_walls_with_a_door_and_a_window_is_found() {
        let (img, size) = picture();
        let found = scan(&img, size, 1.0 / 66.0);
        assert_eq!(found.doors, 1, "{found:?}");
        assert_eq!(found.windows, 1, "{found:?}");
        let concrete = found.walls.iter().filter(|w| w.material == Material::Concrete).count();
        assert!(concrete >= 6, "{concrete} concrete walls");
        let door = found.walls.iter().find(|w| w.material == Material::Wood).unwrap();
        assert!((door.a[0] - 250.0).abs() < 4.0 && (door.b[0] - 320.0).abs() < 4.0, "{door:?}");
        assert!((door.a[1] - 106.5).abs() < 3.0, "{door:?}");
    }

    #[test]
    fn a_bare_gap_gets_no_wall_and_thin_lines_are_not_walls() {
        let (mut img, (w, h)) = picture();
        // Clear the door's arc, leaving an opening; draw a thin dimension line.
        for y in 113..200 {
            for x in 240..340 {
                img[(y * w as usize + x) * 3..][..3].fill(255);
            }
        }
        for x in 150..450 {
            img[(200 * w as usize + x) * 3..][..3].fill(90);
        }
        let found = scan(&img, (w, h), 1.0 / 66.0);
        assert_eq!(found.doors, 0);
        assert!(found.walls.iter().all(|w| (w.a[1] - 200.0).abs() > 5.0 || (w.b[1] - 200.0).abs() > 5.0), "the thin line is not a wall");
    }

    /// See what is found on a plan: `WS_PLAN=plan.png WS_MPP=0.0151 cargo test --release scan_plan -- --ignored --nocapture`.
    /// Writes `walls.png` beside it with the walls (red), doors (green) and windows (blue) drawn on.
    #[test]
    #[ignore]
    fn scan_plan() {
        let path = std::env::var("WS_PLAN").expect("WS_PLAN");
        let mpp: f32 = std::env::var("WS_MPP").ok().and_then(|v| v.parse().ok()).unwrap_or(0.015);
        let mut img = image::open(&path).unwrap().into_rgb8();
        let t = std::time::Instant::now();
        let found = scan(img.as_raw(), img.dimensions(), mpp);
        println!("{} walls, {} doors, {} windows in {:?}", found.walls.len(), found.doors, found.windows, t.elapsed());
        if let Ok(v) = std::env::var("WS_NEAR") {
            let n: Vec<f32> = v.split(',').filter_map(|s| s.parse().ok()).collect();
            for w in &found.walls {
                if (w.a[1] - n[1]).abs() < 30.0 && (w.b[0] > n[0] - 200.0) && w.a[0] < n[0] + 400.0 {
                    println!("{w:?}");
                }
            }
        }
        for w in &found.walls {
            let colour = match w.material {
                Material::Wood => image::Rgb([0, 170, 0]),
                Material::Window => image::Rgb([0, 90, 255]),
                _ => image::Rgb([230, 30, 30]),
            };
            let n = w.length_px().max(1.0) as usize;
            for i in 0..=n {
                let t = i as f32 / n as f32;
                let (x, y) = (w.a[0] + (w.b[0] - w.a[0]) * t, w.a[1] + (w.b[1] - w.a[1]) * t);
                for d in -1i32..=1 {
                    let (px, py) = ((x as i32 + d).max(0) as u32, (y as i32 + d).max(0) as u32);
                    if px < img.width() && py < img.height() {
                        img.put_pixel(px, y.max(0.0) as u32 % img.height(), colour);
                        img.put_pixel(x.max(0.0) as u32 % img.width(), py, colour);
                    }
                }
            }
        }
        img.save(std::path::Path::new(&path).with_file_name("walls.png")).unwrap();
    }
}
