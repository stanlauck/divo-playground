// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::{Error, FrameRate, Result};
use serde::{Deserialize, Serialize};

/// Drop-frame drops labels 00 and 01 at each minute except every tenth minute.
/// It does not drop pictures or change the rational frame rate.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimecodeMode {
    #[default]
    NonDrop,
    Drop,
}
impl TimecodeMode {
    pub(crate) fn is_default(&self) -> bool {
        *self == Self::NonDrop
    }
    pub(crate) fn validate(self, rate: FrameRate) -> Result<()> {
        rate.validate()?;
        if self == Self::Drop && rate.reduced() != FrameRate::new(30_000, 1001) {
            return Err(Error::unsupported("timecode_mode"));
        }
        Ok(())
    }
    pub(crate) fn day_frames(self, rate: FrameRate) -> Result<u64> {
        self.validate(rate)?;
        Ok(if self == Self::Drop {
            2_589_408
        } else {
            rate.timecode_base()? * 86_400
        })
    }
}

/// A rate/mode pair validated once per conversion, so per-event calls do not
/// repeat the rate checks. Internal; the public functions below validate too.
#[derive(Clone, Copy)]
pub(crate) struct Timecode {
    base: u64,
    day: u64,
    drop: bool,
}
impl Timecode {
    pub(crate) fn new(rate: FrameRate, mode: TimecodeMode) -> Result<Self> {
        mode.validate(rate)?;
        Ok(Self {
            base: rate.timecode_base()?,
            day: mode.day_frames(rate)?,
            drop: mode == TimecodeMode::Drop,
        })
    }
    pub(crate) fn parse(&self, text: &str) -> Result<u64> {
        let b = text.as_bytes();
        if b.len() != 11
            || b[2] != b':'
            || b[5] != b':'
            || !(b[8] == b':' || self.drop && b[8] == b';')
            || [0, 1, 3, 4, 6, 7, 9, 10]
                .iter()
                .any(|i| !b[*i].is_ascii_digit())
        {
            return Err(Error::invalid("timecode"));
        }
        let pair = |i| u64::from(b[i] - b'0') * 10 + u64::from(b[i + 1] - b'0');
        let (h, m, s, f) = (pair(0), pair(3), pair(6), pair(9));
        if h >= 24 || m >= 60 || s >= 60 || f >= self.base {
            return Err(Error::invalid("timecode"));
        }
        let total_minutes = h * 60 + m;
        if self.drop && m % 10 != 0 && s == 0 && f < 2 {
            return Err(Error::invalid("timecode.dropped_label"));
        }
        let nominal = ((h * 3600 + m * 60 + s) * self.base) + f;
        Ok(if self.drop {
            nominal - 2 * (total_minutes - total_minutes / 10)
        } else {
            nominal
        })
    }
    fn format(&self, frame: u64, sep: char) -> Result<String> {
        if frame >= self.day {
            return Err(Error::unsupported("timecode.midnight"));
        }
        let nominal = if self.drop {
            let blocks = frame / 17_982;
            let remainder = frame % 17_982;
            blocks * 18_000 + remainder + 2 * (remainder.saturating_sub(2) / 1798)
        } else {
            frame
        };
        Ok(format!(
            "{:02}:{:02}:{:02}{sep}{:02}",
            nominal / (self.base * 3600),
            nominal / (self.base * 60) % 60,
            nominal / self.base % 60,
            nominal % self.base
        ))
    }
    /// Neutral form: drop labels use `;`.
    pub(crate) fn format_neutral(&self, frame: u64) -> Result<String> {
        self.format(frame, if self.drop { ';' } else { ':' })
    }
    /// CMX event rows use colons in both modes; FCM defines interpretation.
    pub(crate) fn format_cmx(&self, frame: u64) -> Result<String> {
        self.format(frame, ':')
    }
}

/// Parses a label without midnight rollover. Drop mode accepts colon or
/// semicolon before FF; non-drop accepts only colon. Hours must be 00–23.
pub fn parse_timecode(text: &str, rate: FrameRate, mode: TimecodeMode) -> Result<u64> {
    Timecode::new(rate, mode)?.parse(text)
}

/// Formats a zero-based frame count. Neutral drop timecodes use semicolons;
/// CMX event rows use colons and the global FCM header defines their mode.
pub fn format_timecode(frame: u64, rate: FrameRate, mode: TimecodeMode) -> Result<String> {
    Timecode::new(rate, mode)?.format_neutral(frame)
}
