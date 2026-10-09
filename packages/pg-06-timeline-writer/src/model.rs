// SPDX-License-Identifier: MIT OR Apache-2.0

//! Neutral shot list JSON: the schema, normalization and validation.

use std::collections::HashSet;
use std::fmt;

use serde::de::{self, Deserializer};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

pub const SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_RESOLUTION: [u32; 2] = [1920, 1080];
pub const DEFAULT_RECORD_START: &str = "00:00:00:00";
/// Exact in OTIO and, at any supported rational rate, within FCPXML's i64 time numerator.
pub const MAX_FRAMES: u64 = 1 << 30;
pub const MAX_SHOTS: usize = 10_000;
const MAX_TEXT: usize = 1024 * 1024;
const MAX_RESOLUTION: u32 = 32_768;

/// Charges canonical owned text before reader copies or model clones.
#[derive(Default)]
pub(crate) struct TextBudget {
    used: usize,
}

impl TextBudget {
    pub(crate) fn field(&mut self, path: &str, value: &str, limit: usize) -> Result<()> {
        text_size(path, value, limit)?;
        self.charge(path, value.len())
    }

    fn charge(&mut self, path: &str, bytes: usize) -> Result<()> {
        self.used = self
            .used
            .checked_add(bytes)
            .ok_or_else(|| Error::new("text_limit", path))?;
        if self.used > MAX_TEXT {
            return Err(Error::new("text_limit", path));
        }
        Ok(())
    }

    pub(crate) fn name(&mut self, path: &str, name: Option<&str>, id: &str) -> Result<()> {
        if let Some(name) = name {
            text_size(path, name, 4096)?;
            if !name.is_empty() && name != id {
                self.charge(path, name.len())?;
            }
        }
        Ok(())
    }
}

pub(crate) fn text_size(path: &str, value: &str, limit: usize) -> Result<()> {
    if value.len() > limit {
        return Err(Error::new("text_limit", path));
    }
    check_text(path, value)
}

/// One video track of shots in cut order.
///
/// Unknown fields are ignored when reading, so a file can also carry data for
/// other tools.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShotList {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub frame_rate: FrameRate,
    /// Non-drop timecode of the first frame; `00:00:00:00` when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_start: Option<String>,
    /// Width and height in pixels; 1920x1080 when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<[u32; 2]>,
    pub shots: Vec<Shot>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shot {
    /// Unique within the list; stored in clip metadata of both timeline formats.
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub duration_frames: u64,
    /// Empty frames on the track before this shot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap_before_frames: Option<u64>,
    /// Source media; a shot without media is a placeholder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<Media>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Media {
    /// Media location, written as-is. Final Cut Pro expects an absolute `file://` URL.
    pub url: String,
    /// First frame used from the media, counted from the start of the media.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_in_frame: Option<u64>,
}

/// Exact frame rate as `num / den` frames per second, for example 24000/1001.
///
/// In JSON an integer rate is written as a number (`24`), any other rate as
/// `{"num": 24000, "den": 1001}`. Both forms are accepted when reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameRate {
    pub num: u32,
    pub den: u32,
}

impl FrameRate {
    pub const fn new(num: u32, den: u32) -> Self {
        FrameRate { num, den }
    }

    pub fn as_f64(self) -> f64 {
        f64::from(self.num) / f64::from(self.den)
    }

    /// Frames per second used to count timecode, e.g. 24 for 23.976.
    pub fn timecode_base(self) -> Result<u64> {
        self.validate()?;
        Ok(u64::from(self.num).div_ceil(u64::from(self.den)))
    }

    pub(crate) fn reduced(self) -> Self {
        let g = gcd(self.num, self.den).max(1);
        FrameRate::new(self.num / g, self.den / g)
    }

    pub fn validate(self) -> Result<()> {
        let fps = self.as_f64();
        if self.num == 0 || self.den == 0 || !(1.0..=1000.0).contains(&fps) {
            return Err(Error::invalid("frame_rate"));
        }
        Ok(())
    }
}

impl fmt::Display for FrameRate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.den == 1 {
            write!(f, "{}", self.num)
        } else {
            write!(f, "{}/{}", self.num, self.den)
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum FrameRateRepr {
    Whole(u32),
    Ratio { num: u32, den: u32 },
}

impl Serialize for FrameRate {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        if self.den == 1 {
            FrameRateRepr::Whole(self.num).serialize(s)
        } else {
            FrameRateRepr::Ratio {
                num: self.num,
                den: self.den,
            }
            .serialize(s)
        }
    }
}

impl<'de> Deserialize<'de> for FrameRate {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        match FrameRateRepr::deserialize(d).map_err(|_| {
            de::Error::custom("frame_rate must be a whole number or {\"num\", \"den\"}")
        })? {
            FrameRateRepr::Whole(n) => Ok(FrameRate::new(n, 1)),
            FrameRateRepr::Ratio { num, den } => Ok(FrameRate::new(num, den)),
        }
    }
}

impl ShotList {
    pub fn new(frame_rate: FrameRate) -> Self {
        ShotList {
            version: SCHEMA_VERSION,
            title: None,
            frame_rate,
            record_start: None,
            resolution: None,
            shots: Vec::new(),
        }
    }

    /// Frame number of `record_start`.
    pub fn start_frame(&self) -> Result<u64> {
        match &self.record_start {
            Some(tc) => parse_timecode(tc, self.frame_rate),
            None => Ok(0),
        }
    }

    pub fn resolution_or_default(&self) -> [u32; 2] {
        self.resolution.unwrap_or(DEFAULT_RESOLUTION)
    }

    /// Track length in frames, gaps included.
    pub fn duration_frames(&self) -> Result<u64> {
        self.shots.iter().try_fold(0u64, |total, s| {
            let end = total
                .checked_add(s.gap_before_frames.unwrap_or(0))
                .and_then(|n| n.checked_add(s.duration_frames))
                .ok_or_else(|| Error::invalid("shots.duration_frames"))?;
            check_frames("shots.duration_frames", end)?;
            Ok(end)
        })
    }

    /// Returns the canonical form and checks it.
    ///
    /// Values equal to their defaults become absent, empty strings become
    /// absent, a name equal to the id is dropped, the frame rate is reduced
    /// and `record_start` is rewritten as `HH:MM:SS:FF`. Every reader and
    /// writer in this crate goes through this, so the formats agree.
    pub fn normalized(&self) -> Result<ShotList> {
        if self.version != SCHEMA_VERSION {
            return Err(Error::unsupported("version"));
        }
        self.frame_rate.validate()?;
        let frame_rate = self.frame_rate.reduced();
        if self.shots.len() > MAX_SHOTS {
            return Err(Error::new("record_limit", "shots"));
        }
        let mut budget = TextBudget::default();
        if let Some(title) = &self.title {
            budget.field("title", title, 4096)?;
        }

        let mut out = ShotList {
            version: SCHEMA_VERSION,
            title: non_empty(&self.title),
            frame_rate,
            record_start: None,
            resolution: self.resolution.filter(|r| *r != DEFAULT_RESOLUTION),
            shots: Vec::with_capacity(self.shots.len()),
        };
        if let Some([w, h]) = out.resolution {
            if !(1..=MAX_RESOLUTION).contains(&w) || !(1..=MAX_RESOLUTION).contains(&h) {
                return Err(Error::invalid("resolution"));
            }
        }
        let start = match &self.record_start {
            Some(tc) => parse_timecode(tc, frame_rate)?,
            None => 0,
        };
        if start > 0 {
            out.record_start = Some(format_timecode(start, frame_rate)?);
        }

        let mut seen = HashSet::with_capacity(self.shots.len());
        let mut end = start;
        for (index, shot) in self.shots.iter().enumerate() {
            let at = format!("shots[{index}]");
            budget.field(&format!("{at}.id"), &shot.id, 256)?;
            if shot.id.is_empty() || shot.id.trim() != shot.id {
                return Err(Error::invalid(format!("{at}.id")));
            }
            if !seen.insert(shot.id.as_str()) {
                return Err(Error::new("duplicate_id", format!("{at}.id")));
            }
            if shot.duration_frames == 0 {
                return Err(Error::invalid(format!("{at}.duration_frames")));
            }
            budget.name(&format!("{at}.name"), shot.name.as_deref(), &shot.id)?;
            let name = non_empty(&shot.name).filter(|n| *n != shot.id);
            let media = match &shot.media {
                Some(m) => {
                    let path = format!("{at}.media.url");
                    budget.field(&path, &m.url, 8192)?;
                    check_url(&path, &m.url)?;
                    let source_in = m.source_in_frame.filter(|f| *f > 0);
                    let source_end = source_in
                        .unwrap_or(0)
                        .checked_add(shot.duration_frames)
                        .ok_or_else(|| Error::invalid(format!("{at}.media.source_in_frame")))?;
                    check_frames(&at, source_end)?;
                    Some(Media {
                        url: m.url.clone(),
                        source_in_frame: source_in,
                    })
                }
                None => None,
            };
            let gap = shot.gap_before_frames.filter(|g| *g > 0);
            end = end
                .checked_add(gap.unwrap_or(0))
                .and_then(|n| n.checked_add(shot.duration_frames))
                .ok_or_else(|| Error::invalid(&at))?;
            check_frames(&at, end)?;
            out.shots.push(Shot {
                id: shot.id.clone(),
                name,
                duration_frames: shot.duration_frames,
                gap_before_frames: gap,
                media,
            });
        }
        Ok(out)
    }
}

impl Shot {
    pub fn new(id: impl Into<String>, duration_frames: u64) -> Self {
        Shot {
            id: id.into(),
            duration_frames,
            ..Shot::default()
        }
    }

    /// Clip name in the timeline formats: the name, or the id when there is none.
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.id)
    }
}

/// Parses non-drop `HH:MM:SS:FF` timecode into a frame number.
pub fn parse_timecode(tc: &str, rate: FrameRate) -> Result<u64> {
    let base = rate.timecode_base()?;
    if tc.contains(';') {
        return Err(Error::unsupported("record_start.drop_frame"));
    }
    if tc.len() != if base > 100 { 12 } else { 11 } {
        return Err(Error::invalid("record_start"));
    }
    let parts: Vec<&str> = tc.split(':').collect();
    let nums: Option<Vec<u64>> = parts
        .iter()
        .enumerate()
        .map(|(i, p)| {
            (p.len() == if i == 3 && base > 100 { 3 } else { 2 }
                && p.bytes().all(|b| b.is_ascii_digit()))
            .then(|| p.parse().ok())
            .flatten()
        })
        .collect();
    match nums.as_deref() {
        Some(&[h, m, s, f]) if m < 60 && s < 60 && f < base => {
            Ok(((h * 60 + m) * 60 + s) * base + f)
        }
        _ => Err(Error::invalid("record_start")),
    }
}

/// Formats a frame number as non-drop `HH:MM:SS:FF` timecode.
pub fn format_timecode(frame: u64, rate: FrameRate) -> Result<String> {
    let base = rate.timecode_base()?;
    let (secs, f) = (frame / base, frame % base);
    if secs / 3600 > 99 {
        return Err(Error::unsupported("record_start.hours"));
    }
    let width = if base > 100 { 3 } else { 2 };
    Ok(format!(
        "{:02}:{:02}:{:02}:{:0width$}",
        secs / 3600,
        secs / 60 % 60,
        secs % 60,
        f
    ))
}

/// Gives shots read from other tools an id: the clip name when it is free,
/// otherwise `shot-<n>` with `n` the 1-based position.
pub(crate) fn assign_missing_ids(shots: &mut [Shot]) {
    let mut used: HashSet<String> = shots
        .iter()
        .filter(|s| !s.id.is_empty())
        .map(|s| s.id.clone())
        .collect();
    let mut cursor = 1;
    for (i, shot) in shots.iter_mut().enumerate() {
        if !shot.id.is_empty() {
            continue;
        }
        let name = shot.name.as_deref().unwrap_or_default().trim();
        shot.id = if !name.is_empty() && name.len() <= 256 && !used.contains(name) {
            name.to_owned()
        } else {
            let mut n = (i + 1).max(cursor);
            while used.contains(&format!("shot-{n}")) {
                n += 1;
            }
            cursor = n + 1;
            format!("shot-{n}")
        };
        used.insert(shot.id.clone());
    }
}

fn non_empty(v: &Option<String>) -> Option<String> {
    v.as_ref().filter(|s| !s.is_empty()).cloned()
}

fn check_frames(at: &str, frames: u64) -> Result<()> {
    if frames > MAX_FRAMES {
        return Err(Error::new("frame_limit", at));
    }
    Ok(())
}

// Text ends up in XML attributes, which cannot carry most control characters.
fn check_text(at: &str, s: &str) -> Result<()> {
    match s
        .chars()
        .find(|c| c.is_control() || matches!(*c, '\u{fffe}' | '\u{ffff}' | '\u{2028}' | '\u{2029}'))
    {
        Some(_) => Err(Error::invalid(at)),
        None => Ok(()),
    }
}

pub(crate) fn check_url(path: &str, text: &str) -> Result<()> {
    if !text.is_ascii()
        || text
            .bytes()
            .any(|b| !b.is_ascii_alphanumeric() && !b"-._~:/?#[]@!$&'()*+,;=%".contains(&b))
    {
        return Err(Error::invalid(path));
    }
    for (i, b) in text.bytes().enumerate() {
        if b == b'%'
            && !text
                .as_bytes()
                .get(i + 1..i + 3)
                .is_some_and(|s| s.iter().all(u8::is_ascii_hexdigit))
        {
            return Err(Error::invalid(path));
        }
    }
    let (scheme, rest) = text.split_once("://").ok_or_else(|| Error::invalid(path))?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    if authority.contains('@')
        || rest[end..].contains(['[', ']'])
        || scheme.eq_ignore_ascii_case("https") && authority.is_empty()
    {
        return Err(Error::invalid(path));
    }
    let url = url::Url::parse(text).map_err(|_| Error::invalid(path))?;
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        return Err(Error::invalid(path));
    }
    match url.scheme() {
        "https"
            if text
                .get(..8)
                .is_some_and(|s| s.eq_ignore_ascii_case("https://"))
                && url.host_str().is_some() =>
        {
            Ok(())
        }
        "file"
            if text
                .get(..7)
                .is_some_and(|s| s.eq_ignore_ascii_case("file://"))
                && url.path().starts_with('/')
                && url.path().len() > 1
                && url.query().is_none() =>
        {
            Ok(())
        }
        _ => Err(Error::invalid(path)),
    }
}

pub(crate) fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}
