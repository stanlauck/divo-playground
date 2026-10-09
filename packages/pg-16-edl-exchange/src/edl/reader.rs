// SPDX-License-Identifier: MIT OR Apache-2.0

use super::metadata::{ShotMeta, TimelineMeta, decode, decode_json, native_title};
use super::reels::{Reels, token};
use super::{GAP_COMMENT, MAX_LINE, MAX_LINES, SHOT_ID_PREFIX, SHOT_PREFIX, TIMELINE_PREFIX};
use crate::model::assign_missing_ids;
use crate::timecode::Timecode;
use crate::{
    Error, FrameRate, MAX_EDL_BYTES, MAX_EVENTS, Media, Result, Shot, ShotList, TimecodeMode,
};

#[derive(Default)]
struct Raw<'a> {
    number: usize,
    reel: &'a str,
    source_in: &'a str,
    source_out: &'a str,
    record_in: &'a str,
    record_out: &'a str,
    id: Option<&'a str>,
    metadata: Option<&'a str>,
    gap: bool,
    name: Option<&'a str>,
    url: Option<&'a str>,
}
impl<'a> Raw<'a> {
    fn parse(line: &'a str) -> Result<Self> {
        let p: Vec<_> = line.split_ascii_whitespace().take(10).collect();
        if p.len() != 8 {
            return Err(Error::unsupported("event.fields"));
        }
        if p[0].len() > 3 || !p[0].bytes().all(|b| b.is_ascii_digit()) {
            return Err(Error::invalid("event.number"));
        }
        let number = p[0]
            .parse::<usize>()
            .map_err(|_| Error::invalid("event.number"))?;
        if number == 0 || number > MAX_EVENTS {
            return Err(Error::invalid("event.number"));
        }
        if !token(p[1]) {
            return Err(Error::invalid("event.reel"));
        }
        if p[1] == "BARS" {
            return Err(Error::unsupported("event.generator"));
        }
        if p[2] != "V" {
            return Err(Error::unsupported("event.channel"));
        }
        if p[3] != "C" {
            return Err(Error::unsupported("event.transition"));
        }
        Ok(Self {
            number,
            reel: p[1],
            source_in: p[4],
            source_out: p[5],
            record_in: p[6],
            record_out: p[7],
            ..Self::default()
        })
    }
}
fn unique<'a>(target: &mut Option<&'a str>, value: &'a str) -> Result<()> {
    if target.replace(value).is_some() {
        return Err(Error::new("duplicate_field", "comment"));
    }
    Ok(())
}
fn semantic_comment(body: &str) -> bool {
    let first = body
        .split_ascii_whitespace()
        .next()
        .unwrap_or_default()
        .trim_end_matches(':');
    [
        "M2",
        "FREEZE",
        "MOTION",
        "SPEED",
        "TIMEWARP",
        "TIME",
        "VARY",
        "REVERSE",
        "TRANSITION",
        "DISSOLVE",
        "WIPE",
        "KEY",
        "EFFECT",
        "SPLIT",
        "VIDEO",
        "AUDIO",
        "ASC_SOP",
        "ASC_SAT",
        "ASC_CDL",
        "CDL",
        "LUT",
        "OTIO",
    ]
    .iter()
    .any(|s| first.eq_ignore_ascii_case(s))
}

/// Reads our metadata-bearing export, or a strict external cut-only subset.
/// An external EDL has no exact rate field: `rate` is mandatory without our
/// timeline metadata. Supplying a rate for our export checks, not overrides it.
pub fn from_edl(text: &str, rate: Option<FrameRate>) -> Result<ShotList> {
    if text.len() > MAX_EDL_BYTES {
        return Err(Error::new("input_limit", "input"));
    }
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut title = None;
    let mut fcm = None;
    let mut meta_text = None;
    let mut rows: Vec<Raw<'_>> = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let at = format!("lines[{}]", i + 1);
        if i >= MAX_LINES || line.len() > MAX_LINE {
            return Err(Error::new("input_limit", at));
        }
        let line = line.trim_matches([' ', '\t']);
        if line.chars().any(|c| c.is_control() && c != '\t') {
            return Err(Error::invalid(at));
        }
        if line.is_empty() {
            continue;
        }
        let result = if let Some(value) = line.strip_prefix("TITLE:") {
            if !rows.is_empty() || meta_text.is_some() {
                Err(Error::unsupported("header.position"))
            } else {
                unique(&mut title, value.trim())
            }
        } else if let Some(value) = line.strip_prefix("FCM:") {
            if !rows.is_empty() || fcm.is_some() || meta_text.is_some() {
                Err(Error::unsupported("header.fcm"))
            } else {
                fcm = Some(match value.trim() {
                    "NON-DROP FRAME" | "NON DROP FRAME" => TimecodeMode::NonDrop,
                    "DROP FRAME" => TimecodeMode::Drop,
                    _ => return Err(Error::unsupported(at)),
                });
                Ok(())
            }
        } else if let Some(value) = line.strip_prefix(TIMELINE_PREFIX) {
            unique(&mut meta_text, value)
        } else if line.starts_with('*') {
            comment(line, rows.last_mut())
        } else {
            if meta_text.is_some() {
                return Err(Error::unsupported("metadata.position"));
            }
            if rows.len() == MAX_EVENTS {
                return Err(Error::new("record_limit", "events"));
            }
            let row = Raw::parse(line).map_err(|e| e.at(&at))?;
            if rows.last().is_some_and(|prev| prev.number >= row.number) {
                return Err(Error::unsupported(at));
            }
            rows.push(row);
            Ok(())
        };
        result.map_err(|e| e.at(at))?;
    }
    let meta: Option<TimelineMeta> = meta_text
        .map(|s| decode_json(s, "metadata.timeline"))
        .transpose()?;
    let own = meta.is_some();
    let mut list = if let Some(m) = meta {
        if m.version != 1
            || m.event_count != rows.len()
            || title.is_none()
            || fcm != Some(m.timecode_mode)
            || title != Some(native_title(m.title.as_deref()).as_str())
        {
            return Err(Error::new("metadata_mismatch", "metadata.timeline"));
        }
        if rate.is_some_and(|r| r.reduced() != m.frame_rate.reduced()) {
            return Err(Error::new("rate_mismatch", "frame_rate"));
        }
        ShotList {
            version: 1,
            title: m.title,
            frame_rate: m.frame_rate,
            timecode_mode: m.timecode_mode,
            record_start: m.record_start,
            resolution: m.resolution,
            shots: Vec::with_capacity(rows.len()),
        }
    } else {
        let rate = rate.ok_or_else(|| Error::new("missing_frame_rate", "frame_rate"))?;
        let mut list = ShotList::new(rate);
        list.timecode_mode = fcm.unwrap_or_default();
        if let Some(title) = title {
            list.title = Some(title.to_owned());
        }
        list
    };
    let tcx = Timecode::new(list.frame_rate, list.timecode_mode)?;
    if !own && list.record_start.is_none() {
        // The first event's record-in defines the external timeline start.
        if let Some(first) = rows.first() {
            let start = tcx.parse(first.record_in)?;
            list.record_start = Some(tcx.format_neutral(start)?);
        }
    }
    let mut cursor = list.start_frame()?;
    let mut pending_gap = 0u64;
    let mut actual_reels = Vec::with_capacity(rows.len());
    for (i, row) in rows.iter().enumerate() {
        let at = format!("events[{i}]");
        if own && row.number != i + 1 {
            return Err(Error::new("metadata_mismatch", format!("{at}.number")));
        }
        let tc = |s| tcx.parse(s).map_err(|e| e.at(&at));
        let (source_in, source_out, record_in, record_out) = (
            tc(row.source_in)?,
            tc(row.source_out)?,
            tc(row.record_in)?,
            tc(row.record_out)?,
        );
        if source_out <= source_in || record_out <= record_in || record_in < cursor {
            return Err(Error::unsupported(format!("{at}.ranges")));
        }
        let duration = record_out - record_in;
        if source_out - source_in != duration {
            return Err(Error::unsupported(format!("{at}.retime")));
        }
        if own && record_in != cursor {
            return Err(Error::new("metadata_mismatch", format!("{at}.record_in")));
        }
        pending_gap += record_in - cursor;
        cursor = record_out;
        let id = row.id.map(|s| decode(s, &format!("{at}.id"))).transpose()?;
        if id
            .as_ref()
            .is_some_and(|id| id.is_empty() || id.trim() != id)
        {
            return Err(Error::invalid(format!("{at}.id")));
        }
        let metadata: Option<ShotMeta> = row
            .metadata
            .map(|s| decode_json(s, &format!("{at}.metadata")))
            .transpose()?;
        if !own && (metadata.is_some() || row.gap) {
            return Err(Error::new("orphan_metadata", at));
        }
        let gap = if own {
            row.gap
        } else {
            matches!(row.reel, "BL" | "BLACK") && id.is_none()
        };
        if gap {
            if source_in != 0
                || !matches!(row.reel, "BL" | "BLACK")
                || id.is_some()
                || metadata.is_some()
                || row.url.is_some()
                || own && (row.name.is_some() || row.reel != "BL")
            {
                return Err(Error::new("metadata_mismatch", at));
            }
            pending_gap += duration;
            continue;
        }
        let shot = if own {
            let m = metadata.ok_or_else(|| Error::new("missing_metadata", &at))?;
            if m.version != 1
                || id.as_deref() != Some(m.id.as_str())
                || row.name.is_some()
                || row.url.is_some()
                || m.media
                    .as_ref()
                    .and_then(|m| m.source_in_frame)
                    .unwrap_or(0)
                    != source_in
            {
                return Err(Error::new("metadata_mismatch", at));
            }
            if m.media.is_none() && (row.reel != "BL" || source_in != 0) {
                return Err(Error::new("metadata_mismatch", at));
            }
            Shot {
                id: m.id,
                name: m.name,
                duration_frames: duration,
                gap_before_frames: (pending_gap > 0).then_some(pending_gap),
                media: m.media,
            }
        } else {
            // URL size and lexical checks run in the final normalization.
            let url = row.url.map(str::to_owned);
            let black = matches!(row.reel, "BL" | "BLACK");
            if black && (source_in != 0 || url.is_some()) {
                return Err(Error::unsupported(at));
            }
            Shot {
                id: id.unwrap_or_default(),
                name: row.name.map(str::to_owned),
                duration_frames: duration,
                gap_before_frames: (pending_gap > 0).then_some(pending_gap),
                media: if black {
                    None
                } else {
                    Some(Media {
                        url,
                        reel: Some(row.reel.to_owned()),
                        source_in_frame: (source_in > 0).then_some(source_in),
                    })
                },
            }
        };
        // Text sizes and the shared budget are enforced by normalization.
        list.shots.push(shot);
        actual_reels.push(row.reel);
        pending_gap = 0;
    }
    if pending_gap != 0 {
        return Err(Error::unsupported("events.trailing_gap"));
    }
    assign_missing_ids(&mut list.shots);
    let list = list.normalized()?;
    if own {
        let reels = Reels::new(&list)?;
        for (i, (shot, actual)) in list.shots.iter().zip(actual_reels).enumerate() {
            let expected = match &shot.media {
                Some(m) => reels.get(m.reel_name())?,
                None => "BL",
            };
            if expected != actual {
                return Err(Error::new("metadata_mismatch", format!("events[{i}].reel")));
            }
        }
    }
    Ok(list)
}
fn comment<'a>(line: &'a str, row: Option<&mut Raw<'a>>) -> Result<()> {
    let body = line.trim_start_matches('*').trim();
    if semantic_comment(body) {
        return Err(Error::unsupported("comment.effect"));
    }
    let recognized = line.starts_with(SHOT_ID_PREFIX)
        || line.starts_with(SHOT_PREFIX)
        || line == GAP_COMMENT
        || body.starts_with("FROM CLIP NAME:")
        || body.starts_with("FROM CLIP:")
        || body.starts_with("FROM FILE:");
    if !recognized {
        if body.starts_with("PG16") || body.starts_with("SHOT ID") {
            return Err(Error::new("invalid_metadata", "comment"));
        }
        return Ok(());
    }
    let row = row.ok_or_else(|| Error::unsupported("comment.position"))?;
    if let Some(v) = line.strip_prefix(SHOT_ID_PREFIX) {
        unique(&mut row.id, v)
    } else if let Some(v) = line.strip_prefix(SHOT_PREFIX) {
        unique(&mut row.metadata, v)
    } else if line == GAP_COMMENT {
        if row.gap {
            return Err(Error::new("duplicate_field", "comment"));
        }
        row.gap = true;
        Ok(())
    } else if let Some(v) = body.strip_prefix("FROM CLIP NAME:") {
        unique(&mut row.name, v.trim())
    } else {
        let v = body
            .strip_prefix("FROM CLIP:")
            .or_else(|| body.strip_prefix("FROM FILE:"))
            .ok_or_else(|| Error::invalid("comment"))?;
        unique(&mut row.url, v.trim())
    }
}
