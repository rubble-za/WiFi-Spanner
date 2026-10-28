// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 rubble-za. See the LICENSE and NOTICE files.

//! Project model and RF prediction for WiFi Spanner.
//!
//! Only the wall material table exists so far; floors, survey points and
//! the prediction engine arrive in later phases.

pub mod heatmap;
pub mod survey;

/// Signal loss in dB when passing through a wall, per band.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Attenuation {
    pub ghz2_4: f32,
    pub ghz5: f32,
    pub ghz6: f32,
}

/// Wall materials with typical default losses. These are starting points
/// that calibration against real survey data is meant to refine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Material {
    Drywall,
    Glass,
    Wood,
    Brick,
    Concrete,
    Metal,
}

impl Material {
    pub const ALL: [Material; 6] = [
        Material::Drywall,
        Material::Glass,
        Material::Wood,
        Material::Brick,
        Material::Concrete,
        Material::Metal,
    ];

    pub fn attenuation(self) -> Attenuation {
        let (ghz2_4, ghz5, ghz6) = match self {
            Material::Drywall => (3.0, 4.0, 5.0),
            Material::Glass => (2.0, 3.0, 4.0),
            Material::Wood => (3.0, 4.0, 5.0),
            Material::Brick => (6.0, 8.0, 10.0),
            Material::Concrete => (12.0, 15.0, 18.0),
            Material::Metal => (20.0, 25.0, 30.0),
        };
        Attenuation { ghz2_4, ghz5, ghz6 }
    }

    pub fn name(self) -> &'static str {
        match self {
            Material::Drywall => "Drywall",
            Material::Glass => "Glass",
            Material::Wood => "Wood",
            Material::Brick => "Brick",
            Material::Concrete => "Concrete",
            Material::Metal => "Metal",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn higher_bands_lose_more() {
        for m in Material::ALL {
            let a = m.attenuation();
            assert!(a.ghz2_4 <= a.ghz5 && a.ghz5 <= a.ghz6, "{}", m.name());
        }
    }
}
