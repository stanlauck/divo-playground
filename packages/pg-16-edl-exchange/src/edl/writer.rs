// SPDX-License-Identifier: MIT OR Apache-2.0

use super::metadata::{ShotMeta, TimelineMeta, encode, encode_json, native_title};
use super::reels::Reels;
use super::{GAP_COMMENT, SHOT_ID_PREFIX, SHOT_PREFIX, TIMELINE_PREFIX};
use crate::bounded::{MAX_EDL_BYTES, Output};
use crate::timecode::Timecode;
use crate::{Result, ShotList, TimecodeMode};
use std::io::Write;

/// One numbered cut event: a source range and a record range.
struct Event<'a> {
    number: usize,
    reel: &'a str,
    source_in: u64,
    source_out: u64,
    record_in: u64,
    record_out: u64,
}
impl Event<'_> {
    fn write(&self, out: &mut Output, tc: &Timecode) -> Result<()> {
        writeln!(
            out,
            "{:03}  {:<8} V     C        {} {} {} {}",
            self.number,
            self.reel,
            tc.format_cmx(self.source_in)?,
            tc.format_cmx(self.source_out)?,
            tc.format_cmx(self.record_in)?,
            tc.format_cmx(self.record_out)?
        )?;
        Ok(())
    }
}

/// Exports video-only cuts. One BL event precedes each nonzero gap; a BL shot
/// with ID metadata is a placeholder, never an ordinary gap.
pub fn to_edl(list: &ShotList) -> Result<String> {
    let list = list.normalized()?;
    let reels = Reels::new(&list)?;
    let tc = Timecode::new(list.frame_rate, list.timecode_mode)?;
    let mut out = Output::new(MAX_EDL_BYTES);
    writeln!(out, "TITLE: {}", native_title(list.title.as_deref()))?;
    writeln!(
        out,
        "FCM: {} FRAME",
        if list.timecode_mode == TimecodeMode::Drop {
            "DROP"
        } else {
            "NON-DROP"
        }
    )?;
    writeln!(out)?;
    let mut record = list.start_frame()?;
    let mut number = 1;
    for shot in &list.shots {
        if let Some(gap) = shot.gap_before_frames {
            Event {
                number,
                reel: "BL",
                source_in: 0,
                source_out: gap,
                record_in: record,
                record_out: record + gap,
            }
            .write(&mut out, &tc)?;
            writeln!(out, "{GAP_COMMENT}\n")?;
            number += 1;
            record += gap;
        }
        let reel = match &shot.media {
            Some(m) => reels.get(m.reel_name())?,
            None => "BL",
        };
        let source = shot
            .media
            .as_ref()
            .and_then(|m| m.source_in_frame)
            .unwrap_or(0);
        Event {
            number,
            reel,
            source_in: source,
            source_out: source + shot.duration_frames,
            record_in: record,
            record_out: record + shot.duration_frames,
        }
        .write(&mut out, &tc)?;
        writeln!(out, "{SHOT_ID_PREFIX}{}", encode(&shot.id)?)?;
        writeln!(
            out,
            "{SHOT_PREFIX}{}\n",
            encode_json(&ShotMeta {
                version: 1,
                id: shot.id.clone(),
                name: shot.name.clone(),
                media: shot.media.clone(),
            })?
        )?;
        number += 1;
        record += shot.duration_frames;
    }
    // Some native readers accept comments only after an event. Keep the
    // timeline manifest at EOF, rather than adding an unrecognized header.
    writeln!(
        out,
        "{TIMELINE_PREFIX}{}",
        encode_json(&TimelineMeta::from_list(&list))?
    )?;
    out.finish()
}
