// SPDX-License-Identifier: MIT OR Apache-2.0

// PG-06 model/budget/URI conventions adapted for independent EDL exchange.
use crate::{Error, Result, TimecodeMode, format_timecode, parse_timecode};
use serde::de::{self, Deserializer};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;

pub const SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_RESOLUTION: [u32; 2] = [1920, 1080];
pub const MAX_EVENTS: usize = 999;
pub const MAX_SHOTS: usize = MAX_EVENTS;
/// Auxiliary/unknown-reel token; omission of `media.reel` means the same.
pub(crate) const AUX_REEL: &str = "AX";
const MAX_TEXT: usize = 1024 * 1024;

#[derive(Default)]
pub(crate) struct TextBudget {
    used: usize,
}
impl TextBudget {
    pub(crate) fn field(&mut self, path: &str, value: &str, limit: usize) -> Result<()> {
        text_size(path, value, limit)?;
        self.used = self
            .used
            .checked_add(value.len())
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
                self.field(path, name, 4096)?;
            }
        }
        Ok(())
    }
}
pub(crate) fn text_size(path: &str, text: &str, limit: usize) -> Result<()> {
    if text.len() > limit {
        return Err(Error::new("text_limit", path));
    }
    if text
        .chars()
        .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}' | '\u{fffe}' | '\u{ffff}'))
    {
        return Err(Error::invalid(path));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShotList {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub frame_rate: FrameRate,
    #[serde(default, skip_serializing_if = "TimecodeMode::is_default")]
    pub timecode_mode: TimecodeMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_start: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<[u32; 2]>,
    pub shots: Vec<Shot>,
}
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shot {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub duration_frames: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap_before_frames: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<Media>,
}
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Media {
    /// Optional absolute file/HTTPS URI; never dereferenced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Original reel name. Omission means auxiliary reel AX.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reel: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_in_frame: Option<u64>,
}
impl Media {
    pub(crate) fn reel_name(&self) -> &str {
        self.reel.as_deref().unwrap_or(AUX_REEL)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameRate {
    pub num: u32,
    pub den: u32,
}
impl FrameRate {
    pub const fn new(num: u32, den: u32) -> Self {
        Self { num, den }
    }
    pub fn timecode_base(self) -> Result<u64> {
        self.validate()?;
        Ok(u64::from(self.num).div_ceil(u64::from(self.den)))
    }
    pub(crate) fn reduced(self) -> Self {
        let g = gcd(self.num, self.den).max(1);
        Self::new(self.num / g, self.den / g)
    }
    pub fn validate(self) -> Result<()> {
        if !matches!(
            self.reduced(),
            Self {
                num: 24 | 25 | 30,
                den: 1
            } | Self {
                num: 24_000 | 30_000,
                den: 1001
            }
        ) {
            return Err(Error::unsupported("frame_rate"));
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
enum RateRepr {
    Whole(u32),
    Ratio { num: u32, den: u32 },
}
impl Serialize for FrameRate {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        if self.den == 1 {
            RateRepr::Whole(self.num).serialize(s)
        } else {
            RateRepr::Ratio {
                num: self.num,
                den: self.den,
            }
            .serialize(s)
        }
    }
}
impl<'de> Deserialize<'de> for FrameRate {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        match RateRepr::deserialize(d).map_err(|_| de::Error::custom("exact rate"))? {
            RateRepr::Whole(n) => Ok(Self::new(n, 1)),
            RateRepr::Ratio { num, den } => Ok(Self::new(num, den)),
        }
    }
}

impl ShotList {
    pub fn new(frame_rate: FrameRate) -> Self {
        Self {
            version: 1,
            title: None,
            frame_rate,
            timecode_mode: TimecodeMode::NonDrop,
            record_start: None,
            resolution: None,
            shots: Vec::new(),
        }
    }
    pub fn start_frame(&self) -> Result<u64> {
        match &self.record_start {
            Some(tc) => parse_timecode(tc, self.frame_rate, self.timecode_mode)
                .map_err(|e| e.at("record_start")),
            None => {
                self.timecode_mode.validate(self.frame_rate)?;
                Ok(0)
            }
        }
    }
    pub fn duration_frames(&self) -> Result<u64> {
        self.shots.iter().try_fold(0u64, |n, s| {
            n.checked_add(s.gap_before_frames.unwrap_or(0))
                .and_then(|n| n.checked_add(s.duration_frames))
                .ok_or_else(|| Error::invalid("shots.duration_frames"))
        })
    }
    /// Removes defaults, reduces the rate, canonicalizes timecode and
    /// validates both the model and the bounded cut-only CMX representation.
    pub fn normalized(&self) -> Result<Self> {
        if self.version != SCHEMA_VERSION {
            return Err(Error::unsupported("version"));
        }
        self.timecode_mode.validate(self.frame_rate)?;
        if self.shots.len() > MAX_SHOTS {
            return Err(Error::new("record_limit", "shots"));
        }
        let frame_rate = self.frame_rate.reduced();
        let day = self.timecode_mode.day_frames(frame_rate)?;
        let mut budget = TextBudget::default();
        if let Some(title) = &self.title {
            budget.field("title", title, 4096)?;
        }
        let start = self.start_frame()?;
        let resolution = self.resolution.filter(|r| *r != DEFAULT_RESOLUTION);
        if resolution.is_some_and(|[w, h]| !(1..=32768).contains(&w) || !(1..=32768).contains(&h)) {
            return Err(Error::invalid("resolution"));
        }
        let mut out = Self {
            version: 1,
            title: non_empty(&self.title),
            frame_rate,
            timecode_mode: self.timecode_mode,
            record_start: if start == 0 {
                None
            } else {
                Some(format_timecode(start, frame_rate, self.timecode_mode)?)
            },
            resolution,
            shots: Vec::with_capacity(self.shots.len()),
        };
        let mut ids = HashSet::with_capacity(self.shots.len());
        let mut end = start;
        let mut events = 0;
        for (i, s) in self.shots.iter().enumerate() {
            let at = format!("shots[{i}]");
            budget.field(&format!("{at}.id"), &s.id, 256)?;
            if s.id.is_empty() || s.id.trim() != s.id {
                return Err(Error::invalid(format!("{at}.id")));
            }
            if !ids.insert(s.id.as_str()) {
                return Err(Error::new("duplicate_id", format!("{at}.id")));
            }
            if s.duration_frames == 0 {
                return Err(Error::invalid(format!("{at}.duration_frames")));
            }
            budget.name(&format!("{at}.name"), s.name.as_deref(), &s.id)?;
            let gap = s.gap_before_frames.filter(|g| *g > 0);
            end = end
                .checked_add(gap.unwrap_or(0))
                .and_then(|n| n.checked_add(s.duration_frames))
                .ok_or_else(|| Error::invalid(&at))?;
            if end >= day {
                return Err(Error::unsupported(format!("{at}.record_out.midnight")));
            }
            events += 1 + usize::from(gap.is_some());
            if events > MAX_EVENTS {
                return Err(Error::new("record_limit", "events"));
            }
            let media = if let Some(m) = &s.media {
                if let Some(url) = &m.url {
                    budget.field(&format!("{at}.media.url"), url, 8192)?;
                    check_url(&format!("{at}.media.url"), url)?;
                }
                if let Some(reel) = &m.reel {
                    text_size(&format!("{at}.media.reel"), reel, 4096)?;
                    if reel.is_empty() || reel.trim() != reel {
                        return Err(Error::invalid(format!("{at}.media.reel")));
                    }
                    if reel != AUX_REEL {
                        budget.field(&format!("{at}.media.reel"), reel, 4096)?;
                    }
                }
                let source_in = m.source_in_frame.filter(|f| *f > 0);
                if source_in
                    .unwrap_or(0)
                    .checked_add(s.duration_frames)
                    .is_none_or(|n| n >= day)
                {
                    return Err(Error::unsupported(format!(
                        "{at}.media.source_out.midnight"
                    )));
                }
                Some(Media {
                    url: m.url.clone(),
                    reel: m.reel.as_ref().filter(|r| *r != AUX_REEL).cloned(),
                    source_in_frame: source_in,
                })
            } else {
                None
            };
            out.shots.push(Shot {
                id: s.id.clone(),
                name: non_empty(&s.name).filter(|n| *n != s.id),
                duration_frames: s.duration_frames,
                gap_before_frames: gap,
                media,
            });
        }
        Ok(out)
    }
}
impl Shot {
    pub fn new(id: impl Into<String>, duration_frames: u64) -> Self {
        Self {
            id: id.into(),
            duration_frames,
            ..Self::default()
        }
    }
}
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
fn non_empty(s: &Option<String>) -> Option<String> {
    s.as_ref().filter(|s| !s.is_empty()).cloned()
}
fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
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
        || scheme.eq_ignore_ascii_case("file")
            && (!rest[end..].starts_with('/')
                || authority.contains(':')
                    && !(authority.starts_with('[') && authority.ends_with(']')))
    {
        return Err(Error::invalid(path));
    }
    let url = url::Url::parse(text).map_err(|_| Error::invalid(path))?;
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        return Err(Error::invalid(path));
    }
    match url.scheme() {
        "https" if scheme.eq_ignore_ascii_case("https") && url.host_str().is_some() => Ok(()),
        "file"
            if scheme.eq_ignore_ascii_case("file")
                && url.path().starts_with('/')
                && url.path().len() > 1
                && url.query().is_none() =>
        {
            Ok(())
        }
        _ => Err(Error::invalid(path)),
    }
}
