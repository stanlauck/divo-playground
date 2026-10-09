// SPDX-License-Identifier: MIT OR Apache-2.0

#![doc = include_str!("../README.md")]

mod bounded;
mod edl;
mod error;
mod model;
mod timecode;

pub use bounded::{MAX_EDL_BYTES, MAX_JSON_BYTES, read_input};
pub use edl::{from_edl, to_edl};
pub use error::{Error, Result};
pub use model::{
    DEFAULT_RESOLUTION, FrameRate, MAX_EVENTS, MAX_SHOTS, Media, SCHEMA_VERSION, Shot, ShotList,
};
pub use timecode::{TimecodeMode, format_timecode, parse_timecode};

/// Parses, normalizes and validates neutral JSON. Unknown fields are ignored;
/// duplicate keys (including in unknown fields) are rejected.
pub fn from_json(text: &str) -> Result<ShotList> {
    let list: ShotList = serde_json::from_value(bounded::parse(text, MAX_JSON_BYTES)?)?;
    list.normalized()
}

/// Serializes the validated canonical form without unnecessary defaults.
pub fn to_json(list: &ShotList) -> Result<String> {
    bounded::json_string(&list.normalized()?, MAX_JSON_BYTES)
}
