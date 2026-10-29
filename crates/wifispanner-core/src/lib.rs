// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 rubble-za. See the LICENSE and NOTICE files.

//! Project model and RF prediction for WiFi Spanner.
//!
//! Floors with survey points and walls, heatmap interpolation and the wall
//! material table. The prediction engine arrives in a later phase.

pub mod heatmap;
pub mod survey;
pub mod walls;

/// Signal loss in dB when passing through a wall, per band.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Attenuation {
    pub ghz2_4: f32,
    pub ghz5: f32,
    pub ghz6: f32,
}

/// Wall materials with typical default losses. These are starting points
/// that calibration against real survey data is meant to refine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Material {
    Concrete,
    /// Concrete about 30 cm thick.
    ThickConcrete,
    Drywall,
    /// Full-height glass, such as a boardroom wall.
    Glass,
    /// Doors.
    Wood,
    /// Shelving and other metal: usually not full height.
    Metal,
    Window,
}

impl Material {
    pub const ALL: [Material; 7] = [
        Material::Concrete,
        Material::ThickConcrete,
        Material::Drywall,
        Material::Glass,
        Material::Wood,
        Material::Metal,
        Material::Window,
    ];

    pub fn attenuation(self) -> Attenuation {
        let (ghz2_4, ghz5, ghz6) = match self {
            Material::Concrete => (12.0, 15.0, 18.0),
            Material::ThickConcrete => (18.0, 23.0, 27.0),
            Material::Drywall => (3.0, 4.0, 5.0),
            Material::Glass => (3.0, 4.0, 5.0),
            Material::Wood => (3.0, 4.0, 5.0),
            Material::Metal => (20.0, 25.0, 30.0),
            Material::Window => (2.0, 3.0, 4.0),
        };
        Attenuation { ghz2_4, ghz5, ghz6 }
    }

    pub fn name(self) -> &'static str {
        match self {
            Material::Concrete => "Concrete",
            Material::ThickConcrete => "Thick concrete (30 cm)",
            Material::Drywall => "Drywall",
            Material::Glass => "Glass",
            Material::Wood => "Wood (door)",
            Material::Metal => "Metal (shelf)",
            Material::Window => "Window",
        }
    }

    /// Height in metres a new wall of this material starts with. `None` is
    /// the full height of the floor.
    pub fn default_height_m(self) -> Option<f32> {
        match self {
            Material::Metal => Some(2.0),
            _ => None,
        }
    }

    /// How it is drawn: colour and line width in screen points.
    pub fn style(self) -> ([u8; 3], f32) {
        match self {
            Material::Concrete => ([90, 90, 96], 4.0),
            Material::ThickConcrete => ([50, 50, 56], 7.0),
            Material::Drywall => ([150, 150, 150], 3.0),
            Material::Glass => ([60, 170, 200], 3.0),
            Material::Wood => ([160, 100, 50], 3.0),
            Material::Metal => ([200, 90, 40], 3.5),
            Material::Window => ([120, 190, 240], 2.5),
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

    #[test]
    fn thicker_concrete_loses_more_and_only_shelves_default_to_partial_height() {
        assert!(Material::ThickConcrete.attenuation().ghz5 > Material::Concrete.attenuation().ghz5);
        let partial: Vec<_> = Material::ALL.into_iter().filter(|m| m.default_height_m().is_some()).collect();
        assert_eq!(partial, vec![Material::Metal]);
    }
}
