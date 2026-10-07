// SPDX-License-Identifier: MIT OR Apache-2.0

//! Neutral shot JSON: the input schema and its validation.
//!
//! Axes and units follow glTF: metres, right-handed, +Y up. Unknown fields
//! are ignored so the same file can carry data for other tools.

use std::collections::HashMap;
use std::f64::consts::PI;

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::math::{self, Quat, Vec3};

/// Input schema version understood by this crate.
pub const SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_FOCAL_LENGTH_MM: f64 = 35.0;
/// Full-frame stills gate, width x height.
pub const DEFAULT_SENSOR_MM: [f64; 2] = [36.0, 24.0];
pub const DEFAULT_NEAR_M: f64 = 0.1;
pub const DEFAULT_PROP_COLOR: &str = "#9E9E9E";
const DEFAULT_RGB: [u8; 3] = [0x9E, 0x9E, 0x9E];

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ShotScene {
    pub version: u32,
    /// Scene name, used as the glTF scene name.
    pub scene: Option<String>,
    #[serde(default)]
    pub shots: Vec<Shot>,
    #[serde(default)]
    pub props: Vec<Prop>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Shot {
    /// Unique across shots and props; becomes the node name.
    pub id: String,
    pub name: Option<String>,
    pub camera: Camera,
}

/// Camera placement and lens.
///
/// Orientation comes from `look_at`, or from `pan_deg` / `tilt_deg` when there
/// is no `look_at`; `roll_deg` applies to both. With no orientation fields
/// the camera looks down -Z.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Camera {
    pub position: Vec3,
    pub look_at: Option<Vec3>,
    /// Turn about world +Y; positive turns left.
    pub pan_deg: Option<f64>,
    /// Turn about the camera's X axis; positive looks up.
    pub tilt_deg: Option<f64>,
    /// Turn about the view axis; positive turns the top of the frame left.
    pub roll_deg: Option<f64>,
    pub focal_length_mm: Option<f64>,
    /// Gate width and height.
    pub sensor_mm: Option<[f64; 2]>,
    pub near_m: Option<f64>,
    /// No far plane (infinite projection) when absent.
    pub far_m: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shape {
    #[default]
    Box,
    Cylinder,
    Sphere,
}

/// Placeholder prop: a shape stretched to fill its bounding box.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Prop {
    /// Unique across shots and props; becomes the node name.
    pub id: String,
    pub name: Option<String>,
    #[serde(default)]
    pub shape: Shape,
    /// Bottom centre of the bounding box, so props on the floor have y = 0.
    pub position: Vec3,
    /// Bounding box extent along X, Y and Z before yaw.
    pub size: Vec3,
    /// Turn about +Y; positive is counter-clockwise seen from above.
    pub yaw_deg: Option<f64>,
    /// sRGB colour as `#RRGGBB`.
    pub color: Option<String>,
}

impl ShotScene {
    pub fn validate(&self) -> Result<()> {
        if self.version != SCHEMA_VERSION {
            return Err(Error::invalid(
                "version",
                format!(
                    "unsupported version {}, expected {SCHEMA_VERSION}",
                    self.version
                ),
            ));
        }
        if self.shots.is_empty() && self.props.is_empty() {
            return Err(Error::invalid(
                "shots",
                "the scene needs at least one shot or prop",
            ));
        }
        let mut ids = HashMap::new();
        for (i, shot) in self.shots.iter().enumerate() {
            let at = format!("shots[{i}]");
            check_id(&mut ids, &shot.id, &at)?;
            shot.camera.validate(&format!("{at}.camera"))?;
        }
        for (i, prop) in self.props.iter().enumerate() {
            let at = format!("props[{i}]");
            check_id(&mut ids, &prop.id, &at)?;
            prop.validate(&at)?;
        }
        Ok(())
    }
}

impl Camera {
    pub fn focal_length(&self) -> f64 {
        self.focal_length_mm.unwrap_or(DEFAULT_FOCAL_LENGTH_MM)
    }

    pub fn sensor(&self) -> [f64; 2] {
        self.sensor_mm.unwrap_or(DEFAULT_SENSOR_MM)
    }

    pub fn near(&self) -> f64 {
        self.near_m.unwrap_or(DEFAULT_NEAR_M)
    }

    /// Vertical field of view in radians.
    pub fn yfov(&self) -> f64 {
        2.0 * (self.sensor()[1] / (2.0 * self.focal_length())).atan()
    }

    pub fn aspect_ratio(&self) -> f64 {
        let [width, height] = self.sensor();
        width / height
    }

    /// Node rotation as a unit quaternion `[x, y, z, w]`.
    pub fn rotation(&self) -> Quat {
        let (pan, tilt) = match self.look_at {
            Some(target) => math::pan_tilt_towards(math::sub(target, self.position)),
            None => (
                self.pan_deg.unwrap_or(0.0).to_radians(),
                self.tilt_deg.unwrap_or(0.0).to_radians(),
            ),
        };
        math::pan_tilt_roll(pan, tilt, self.roll_deg.unwrap_or(0.0).to_radians())
    }

    fn validate(&self, at: &str) -> Result<()> {
        finite_all(at, "position", &self.position)?;
        if let Some(target) = self.look_at {
            finite_all(at, "look_at", &target)?;
            if self.pan_deg.is_some() || self.tilt_deg.is_some() {
                return Err(Error::invalid(
                    format!("{at}.look_at"),
                    "use either look_at or pan_deg/tilt_deg, not both",
                ));
            }
            if math::length(math::sub(target, self.position)) < 1e-6 {
                return Err(Error::invalid(
                    format!("{at}.look_at"),
                    "must differ from position",
                ));
            }
        }
        for (field, value) in [
            ("pan_deg", self.pan_deg),
            ("tilt_deg", self.tilt_deg),
            ("roll_deg", self.roll_deg),
        ] {
            if let Some(v) = value {
                finite_all(at, field, &[v])?;
            }
        }
        positive(at, "focal_length_mm", self.focal_length())?;
        let [width, height] = self.sensor();
        positive(at, "sensor_mm[0]", width)?;
        positive(at, "sensor_mm[1]", height)?;
        let (yfov, aspect) = (self.yfov(), self.aspect_ratio());
        if !(yfov > 0.0 && yfov < PI && aspect.is_finite()) {
            return Err(Error::invalid(
                at,
                "focal_length_mm and sensor_mm give an unusable field of view",
            ));
        }
        positive(at, "near_m", self.near())?;
        if let Some(far) = self.far_m {
            if !(far.is_finite() && far > self.near()) {
                return Err(Error::invalid(
                    format!("{at}.far_m"),
                    format!("must be greater than near_m ({}), got {far}", self.near()),
                ));
            }
        }
        Ok(())
    }
}

impl Shape {
    pub fn as_str(self) -> &'static str {
        match self {
            Shape::Box => "box",
            Shape::Cylinder => "cylinder",
            Shape::Sphere => "sphere",
        }
    }
}

impl Prop {
    /// Node rotation as a unit quaternion `[x, y, z, w]`.
    pub fn rotation(&self) -> Quat {
        math::yaw(self.yaw_deg.unwrap_or(0.0).to_radians())
    }

    /// sRGB colour; [`DEFAULT_PROP_COLOR`] when `color` is absent or malformed.
    pub fn rgb(&self) -> [u8; 3] {
        self.color
            .as_deref()
            .and_then(parse_color)
            .unwrap_or(DEFAULT_RGB)
    }

    fn validate(&self, at: &str) -> Result<()> {
        finite_all(at, "position", &self.position)?;
        for (i, v) in self.size.iter().enumerate() {
            positive(at, &format!("size[{i}]"), *v)?;
        }
        if let Some(v) = self.yaw_deg {
            finite_all(at, "yaw_deg", &[v])?;
        }
        if let Some(color) = &self.color {
            if parse_color(color).is_none() {
                return Err(Error::invalid(
                    format!("{at}.color"),
                    format!("expected #RRGGBB, got {color:?}"),
                ));
            }
        }
        Ok(())
    }
}

fn parse_color(s: &str) -> Option<[u8; 3]> {
    let hex = s.strip_prefix('#')?;
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some([channel(0)?, channel(2)?, channel(4)?])
}

fn check_id<'a>(seen: &mut HashMap<&'a str, String>, id: &'a str, at: &str) -> Result<()> {
    if id.trim().is_empty() {
        return Err(Error::invalid(format!("{at}.id"), "must not be empty"));
    }
    if let Some(first) = seen.get(id) {
        return Err(Error::invalid(
            format!("{at}.id"),
            format!("duplicate id {id:?}, first used at {first}"),
        ));
    }
    seen.insert(id, at.to_owned());
    Ok(())
}

fn finite_all(at: &str, field: &str, values: &[f64]) -> Result<()> {
    if values.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(Error::invalid(
            format!("{at}.{field}"),
            "must be finite numbers",
        ))
    }
}

fn positive(at: &str, field: &str, value: f64) -> Result<()> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(Error::invalid(
            format!("{at}.{field}"),
            format!("must be greater than 0, got {value}"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors() {
        assert_eq!(parse_color(DEFAULT_PROP_COLOR), Some(DEFAULT_RGB));
        assert_eq!(parse_color("#8b5A2B"), Some([0x8B, 0x5A, 0x2B]));
        for bad in ["8B5A2B", "#8B5A2", "#8B5A2BB", "#GG0000", "#ÿÿÿ"] {
            assert_eq!(parse_color(bad), None, "{bad}");
        }
    }

    #[test]
    fn lens_defaults() {
        let camera: Camera = serde_json::from_str(r#"{ "position": [0, 0, 0] }"#).unwrap();
        assert_eq!(camera.focal_length(), 35.0);
        assert!((camera.yfov() - 2.0 * (12.0f64 / 35.0).atan()).abs() < 1e-15);
        assert_eq!(camera.aspect_ratio(), 1.5);
        assert_eq!(camera.near(), 0.1);
        assert_eq!(camera.rotation(), [0.0, 0.0, 0.0, 1.0]);
    }
}
