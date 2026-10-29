// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 rubble-za. See the LICENSE and NOTICE files.

//! Survey data: a floor plan with the measurements taken on it.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use wifispanner_scan::{Band, ScanResult};

/// Signal assumed for an AP that was not heard at a surveyed spot.
pub const MISSING_DBM: f64 = -100.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    /// File format version, for migrating old projects.
    pub version: u32,
    pub floor: Floor,
}

impl Project {
    pub const VERSION: u32 = 1;

    pub fn new(floor: Floor) -> Self {
        Project { version: Self::VERSION, floor }
    }

    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }

    pub fn from_json(s: &str) -> serde_json::Result<Self> {
        serde_json::from_str(s)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Floor {
    pub name: String,
    /// The floor plan exactly as imported (PNG or JPEG bytes).
    #[serde(with = "base64_bytes")]
    pub image: Vec<u8>,
    pub width_px: u32,
    pub height_px: u32,
    /// Set by calibrating against a known distance.
    pub metres_per_px: Option<f32>,
    pub points: Vec<SurveyPoint>,
    /// Walls drawn on the plan. Older projects have none.
    #[serde(default)]
    pub walls: Vec<crate::walls::Wall>,
    /// Ceiling height in metres, the height of a full-height wall.
    #[serde(default = "default_ceiling")]
    pub ceiling_m: f32,
}

fn default_ceiling() -> f32 {
    crate::walls::DEFAULT_CEILING_M
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurveyPoint {
    /// Position in floor plan pixels.
    pub x: f32,
    pub y: f32,
    pub unix_secs: u64,
    pub dwell_secs: u64,
    /// Noise floor measured by the adapter during the dwell, when its driver
    /// reports one. Older projects and many drivers have none.
    #[serde(default)]
    pub noise_dbm: Option<f64>,
    pub observations: Vec<Observation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub bssid: String,
    pub ssid: Option<String>,
    pub freq_mhz: u32,
    pub channel: u32,
    pub width_mhz: u32,
    pub signal_dbm: f64,
    /// Scans in the dwell that heard this AP.
    pub samples: u32,
}

impl Observation {
    pub fn band(&self) -> Band {
        Band::from_freq(self.freq_mhz)
    }
}

impl SurveyPoint {
    /// Keeps only APs freshly heard during the dwell; entries the kernel
    /// merely had cached from earlier (possibly elsewhere) are dropped.
    pub fn from_scan(x: f32, y: f32, dwell_secs: u64, scan: &ScanResult) -> Self {
        let unix_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let observations = scan
            .networks
            .iter()
            .filter(|n| n.samples > 0)
            .filter_map(|n| {
                Some(Observation {
                    bssid: n.bssid.clone(),
                    ssid: n.ssid.clone(),
                    freq_mhz: n.freq_mhz,
                    channel: n.channel,
                    width_mhz: n.width_mhz,
                    signal_dbm: n.signal_dbm?,
                    samples: n.samples,
                })
            })
            .collect();
        SurveyPoint { x, y, unix_secs, dwell_secs, noise_dbm: scan.noise_dbm, observations }
    }
}

/// Identifies the physical access point behind a BSSID. One AP radio
/// broadcasts several SSIDs from BSSIDs that differ only in the last octet
/// and the locally-administered bit of the first, so those are ignored.
/// Falls back to the BSSID itself when it is not a MAC address.
pub fn ap_key(bssid: &str, band: Band) -> String {
    let parts: Vec<&str> = bssid.split(':').collect();
    if parts.len() != 6 {
        return format!("{bssid}/{band}");
    }
    let Ok(first) = u8::from_str_radix(parts[0], 16) else { return format!("{bssid}/{band}") };
    format!("{:02x}:{}:{}:{}:{}/{band}", first & !0x02, parts[1], parts[2], parts[3], parts[4]).to_lowercase()
}

/// Which APs a heatmap is drawn for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// Best signal from any AP.
    Any,
    /// Best signal from any AP broadcasting this SSID.
    Ssid(String),
    Bssid(String),
}

impl Floor {
    /// Best matching signal at a point, or [`MISSING_DBM`] if none was heard.
    pub fn signal_at(point: &SurveyPoint, target: &Target, band: Option<Band>) -> f64 {
        Self::signal_at_min(point, target, band, MISSING_DBM)
    }

    /// Like [`Floor::signal_at`], but readings weaker than `min_dbm` count as
    /// not heard.
    pub fn signal_at_min(point: &SurveyPoint, target: &Target, band: Option<Band>, min_dbm: f64) -> f64 {
        point
            .observations
            .iter()
            .filter(|o| o.signal_dbm >= min_dbm)
            .filter(|o| band.is_none_or(|b| o.band() == b))
            .filter(|o| match target {
                Target::Any => true,
                Target::Ssid(s) => o.ssid.as_deref() == Some(s.as_str()),
                Target::Bssid(b) => &o.bssid == b,
            })
            .map(|o| o.signal_dbm)
            .fold(MISSING_DBM, f64::max)
    }

    pub fn ssids(&self) -> BTreeSet<String> {
        self.points.iter().flat_map(|p| &p.observations).filter_map(|o| o.ssid.clone()).collect()
    }

    pub fn bssids(&self) -> BTreeSet<String> {
        self.points.iter().flat_map(|p| &p.observations).map(|o| o.bssid.clone()).collect()
    }
}

mod base64_bytes {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(d)?;
        STANDARD.decode(text).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs(bssid: &str, ssid: &str, freq: u32, dbm: f64) -> Observation {
        Observation {
            bssid: bssid.into(),
            ssid: Some(ssid.into()),
            freq_mhz: freq,
            channel: 1,
            width_mhz: 20,
            signal_dbm: dbm,
            samples: 3,
        }
    }

    fn point() -> SurveyPoint {
        SurveyPoint {
            x: 1.0,
            y: 2.0,
            unix_secs: 0,
            dwell_secs: 5,
            noise_dbm: None,
            observations: vec![
                obs("a", "Home", 2437, -60.0),
                obs("b", "Home", 5180, -50.0),
                obs("c", "Other", 5180, -40.0),
            ],
        }
    }

    #[test]
    fn projects_saved_before_walls_still_open() {
        let old = r#"{"version":1,"floor":{"name":"x","image":"","width_px":1,"height_px":1,"metres_per_px":null,"points":[]}}"#;
        let p = Project::from_json(old).unwrap();
        assert!(p.floor.walls.is_empty());
        assert_eq!(p.floor.ceiling_m, crate::walls::DEFAULT_CEILING_M);
    }

    #[test]
    fn bssids_of_one_radio_share_a_key() {
        let key = |b: &str| ap_key(b, Band::Ghz5);
        assert_eq!(key("50:3d:d1:e7:4c:cc"), key("52:3d:d1:e7:4c:ce"), "other SSIDs of the same radio");
        assert_ne!(key("50:3d:d1:e7:4c:cc"), key("50:3d:d1:e7:4d:cc"));
        assert_ne!(ap_key("50:3d:d1:e7:4c:cc", Band::Ghz2_4), key("50:3d:d1:e7:4c:cc"), "bands are separate radios");
        assert_eq!(ap_key("weird", Band::Ghz5), "weird/5 GHz");
    }

    #[test]
    fn signal_filters_by_target_and_band() {
        let p = point();
        assert_eq!(Floor::signal_at(&p, &Target::Any, None), -40.0);
        assert_eq!(Floor::signal_at(&p, &Target::Ssid("Home".into()), None), -50.0);
        assert_eq!(Floor::signal_at(&p, &Target::Ssid("Home".into()), Some(Band::Ghz2_4)), -60.0);
        assert_eq!(Floor::signal_at(&p, &Target::Bssid("c".into()), Some(Band::Ghz2_4)), MISSING_DBM);
    }

    #[test]
    fn weak_readings_below_the_detection_level_are_ignored() {
        let p = point();
        assert_eq!(Floor::signal_at_min(&p, &Target::Ssid("Home".into()), Some(Band::Ghz2_4), -55.0), MISSING_DBM);
        assert_eq!(Floor::signal_at_min(&p, &Target::Ssid("Home".into()), None, -55.0), -50.0);
    }

    #[test]
    fn project_round_trips_through_json() {
        let floor = Floor {
            name: "Ground".into(),
            image: vec![0, 1, 2, 255],
            width_px: 10,
            height_px: 20,
            metres_per_px: Some(0.05),
            points: vec![point()],
            walls: vec![crate::walls::Wall::new([1.0, 2.0], [3.0, 4.0], crate::Material::Metal)],
            ceiling_m: 4.5,
        };
        let back = Project::from_json(&Project::new(floor).to_json().unwrap()).unwrap();
        assert_eq!(back.floor.walls.len(), 1);
        assert_eq!(back.floor.ceiling_m, 4.5);
        assert_eq!(back.floor.image, vec![0, 1, 2, 255]);
        assert_eq!(back.floor.points[0].observations.len(), 3);
        assert_eq!(back.floor.metres_per_px, Some(0.05));
    }
}
