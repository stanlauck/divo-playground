// SPDX-License-Identifier: MIT OR Apache-2.0

#![doc = include_str!("../README.md")]

//! Editorial timeline writer.
//!
//! A [`ShotList`] is one video track of shots in cut order, each with a
//! duration, an optional gap before it and an optional media reference.
//! It converts to and from:
//!
//! - neutral shot list JSON ([`from_json`], [`to_json`]), the canonical form;
//! - OpenTimelineIO JSON ([`from_otio`], [`to_otio`]);
//! - FCPXML 1.9 ([`from_fcpxml`], [`to_fcpxml`]).
//!
//! Shot ids are stored in clip metadata in both timeline formats, so they
//! survive JSON -> OTIO -> FCPXML -> JSON.

mod bounded;
mod error;
mod fcpxml;
mod model;
mod otio_reader;
mod otio_writer;

pub use bounded::{MAX_JSON_BYTES, MAX_TIMELINE_BYTES, read_input};
pub use error::{Error, Result};
pub use fcpxml::{SHOT_ID_KEY, from_fcpxml, to_fcpxml};
pub use model::{
    DEFAULT_RECORD_START, DEFAULT_RESOLUTION, FrameRate, MAX_FRAMES, MAX_SHOTS, Media,
    SCHEMA_VERSION, Shot, ShotList, format_timecode, parse_timecode,
};
pub use otio_reader::from_otio;
pub use otio_writer::{OTIO_METADATA_KEY, to_otio};

/// Parses, normalizes and validates a shot list from JSON.
///
/// Unknown fields are ignored, so the same file can carry data for other
/// tools (for example camera placement).
pub fn from_json(json: &str) -> Result<ShotList> {
    let list: ShotList = serde_json::from_value(bounded::parse(json, MAX_JSON_BYTES)?)?;
    list.normalized()
}

/// Normalizes, validates and serializes compact neutral JSON.
pub fn to_json(list: &ShotList) -> Result<String> {
    bounded::json_string(&list.normalized()?, false, MAX_JSON_BYTES)
}
