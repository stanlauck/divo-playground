// SPDX-License-Identifier: MIT OR Apache-2.0

//! glTF 2.0 scene mock-ups from neutral shot JSON.
//!
//! A [`ShotScene`] lists shots (camera placements with lens data) and
//! placeholder props (boxes, cylinders, spheres). [`to_gltf`] and [`to_glb`]
//! turn it into a self-contained glTF 2.0 asset: every shot becomes a node
//! with a perspective camera and a wireframe frustum marker, every prop a
//! node with a unit mesh scaled to its size. Input ids are kept in node
//! `extras` so tools can map glTF nodes back to shots and props.

mod error;
mod geometry;
mod math;
mod model;
mod writer;

pub use error::{Error, Result};
pub use model::{
    Camera, DEFAULT_FOCAL_LENGTH_MM, DEFAULT_NEAR_M, DEFAULT_PROP_COLOR, DEFAULT_SENSOR_MM, Prop,
    SCHEMA_VERSION, Shape, Shot, ShotScene,
};
pub use writer::{WriteOptions, to_glb, to_gltf};

/// Parses and validates neutral shot JSON.
pub fn from_json(json: &str) -> Result<ShotScene> {
    let scene: ShotScene = serde_json::from_str(json)?;
    scene.validate()?;
    Ok(scene)
}
