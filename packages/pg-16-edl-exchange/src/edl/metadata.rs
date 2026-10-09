// SPDX-License-Identifier: MIT OR Apache-2.0

use super::MAX_LINE;
use crate::bounded::{self, Output};
use crate::{Error, FrameRate, Media, Result, ShotList, TimecodeMode};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TimelineMeta {
    pub version: u32,
    pub frame_rate: FrameRate,
    pub timecode_mode: TimecodeMode,
    pub title: Option<String>,
    pub record_start: Option<String>,
    pub resolution: Option<[u32; 2]>,
    pub event_count: usize,
}
impl TimelineMeta {
    pub(super) fn from_list(list: &ShotList) -> Self {
        Self {
            version: 1,
            frame_rate: list.frame_rate,
            timecode_mode: list.timecode_mode,
            title: list.title.clone(),
            record_start: list.record_start.clone(),
            resolution: list.resolution,
            event_count: list.shots.len()
                + list
                    .shots
                    .iter()
                    .filter(|s| s.gap_before_frames.is_some())
                    .count(),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ShotMeta {
    pub version: u32,
    pub id: String,
    pub name: Option<String>,
    pub media: Option<Media>,
}

// Percent encoding keeps comments ASCII, single-line, and within the original
// CMX printable character set. JSON remains internal to the extension.
const HEX: &[u8] = b"0123456789ABCDEF";

fn literal(b: u8) -> bool {
    b.is_ascii_uppercase() || b.is_ascii_digit() || b"-./:".contains(&b)
}
/// Streams percent-encoded bytes into a bounded output without a plain copy.
struct Encoder<'a> {
    out: &'a mut Output,
}
impl std::io::Write for Encoder<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        for &b in bytes {
            if literal(b) {
                self.out.write_all(&[b])?;
            } else {
                self.out
                    .write_all(&[b'%', HEX[(b >> 4) as usize], HEX[(b & 15) as usize]])?;
            }
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub(super) fn encode(text: &str) -> Result<String> {
    let mut out = Output::new(MAX_LINE);
    std::io::Write::write_all(&mut Encoder { out: &mut out }, text.as_bytes())?;
    out.finish()
}
pub(super) fn decode(text: &str, path: &str) -> Result<String> {
    if text.len() > MAX_LINE {
        return Err(Error::new("input_limit", path));
    }
    let mut out = Vec::with_capacity(text.len());
    let mut b = text.bytes();
    while let Some(v) = b.next() {
        if v == b'%' {
            let hi = b.next().and_then(hex);
            let lo = b.next().and_then(hex);
            out.push(match (hi, lo) {
                (Some(hi), Some(lo)) => (hi << 4) | lo,
                _ => return Err(Error::invalid(path)),
            });
        } else if literal(v) {
            out.push(v);
        } else {
            return Err(Error::invalid(path));
        }
    }
    String::from_utf8(out).map_err(|_| Error::new("invalid_utf8", path))
}
fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
pub(super) fn encode_json(value: &impl Serialize) -> Result<String> {
    let mut out = Output::new(MAX_LINE);
    serde_json::to_writer(Encoder { out: &mut out }, value).map_err(|e| {
        Error::new(
            if e.is_io() {
                "output_limit"
            } else {
                "invalid_value"
            },
            "output",
        )
    })?;
    out.finish()
}
pub(super) fn decode_json<T: DeserializeOwned>(text: &str, path: &str) -> Result<T> {
    let decoded = decode(text, path)?;
    let value = bounded::parse(&decoded, MAX_LINE).map_err(|e| e.at(path))?;
    serde_json::from_value(value).map_err(|_| Error::new("invalid_metadata", path))
}

/// Native title is a readable projection only; exact Unicode lives in metadata.
pub(super) fn native_title(title: Option<&str>) -> String {
    let mut title: String = title
        .unwrap_or("UNTITLED")
        .chars()
        .take(70)
        .map(|c| {
            let c = c.to_ascii_uppercase();
            if c.is_ascii_uppercase() || c.is_ascii_digit() || c == ' ' {
                c
            } else {
                ' '
            }
        })
        .collect();
    title.truncate(title.trim_end().len());
    title = title.trim_start().to_owned();
    if title.is_empty() {
        "UNTITLED".to_owned()
    } else {
        title
    }
}
