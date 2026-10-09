// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::bounded::{MAX_TIMELINE_BYTES, parse};
use crate::model::{TextBudget, assign_missing_ids};
use crate::otio_writer::{DEFAULT_MEDIA, OTIO_METADATA_KEY};
use crate::{
    Error, FrameRate, MAX_FRAMES, MAX_SHOTS, Media, Result, Shot, ShotList, format_timecode,
};
use serde_json::Value;

/// Reads a single untrimmed, enabled video track. Unsupported edits are errors.
pub fn from_otio(text: &str) -> Result<ShotList> {
    let root = parse(text, MAX_TIMELINE_BYTES)?;
    schema(&root, &["Timeline.1"], "timeline")?;
    let stack = field(&root, "tracks", "timeline.tracks")?;
    schema(stack, &["Stack.1"], "timeline.tracks")?;
    simple(stack, true, "timeline.tracks")?;
    let tracks = children(stack, "timeline.tracks.children")?;
    if tracks.len() != 1 {
        return Err(Error::unsupported("timeline.tracks.children"));
    }
    let track = &tracks[0];
    schema(track, &["Track.1"], "track")?;
    if track.get("kind").and_then(Value::as_str) != Some("Video") {
        return Err(Error::unsupported("track.kind"));
    }
    simple(track, true, "track")?;
    let items = children(track, "track.children")?;
    if items.len() > MAX_SHOTS * 2 {
        return Err(Error::new("record_limit", "track.children"));
    }
    let meta = namespace(&root, "timeline.metadata")?;
    if meta
        .and_then(|m| m.get("version"))
        .is_some_and(|v| v.as_u64() != Some(1))
    {
        return Err(Error::unsupported("timeline.metadata.shot_list.version"));
    }
    let rate = if let Some(value) = meta.and_then(|m| m.get("frame_rate")) {
        let rate: FrameRate = serde_json::from_value(value.clone())?;
        rate.validate()?;
        rate.reduced()
    } else {
        let number = items
            .iter()
            .find_map(|c| {
                c.pointer("/source_range/duration/rate")
                    .and_then(Value::as_f64)
            })
            .or_else(|| {
                root.pointer("/global_start_time/rate")
                    .and_then(Value::as_f64)
            })
            .ok_or_else(|| Error::invalid("track.frame_rate"))?;
        infer_rate(number)?
    };
    let start = root
        .get("global_start_time")
        .filter(|v| !v.is_null())
        .map(|v| frames_of(v, rate, "timeline.global_start_time"))
        .transpose()?
        .unwrap_or(0);
    let mut list = ShotList::new(rate);
    let mut budget = TextBudget::default();
    let title = string(&root, "name", "timeline.name")?;
    budget.field("timeline.name", title.unwrap_or(""), 4096)?;
    list.title = title.map(str::to_owned);
    list.record_start = Some(format_timecode(start, rate)?);
    list.resolution = meta
        .and_then(|m| m.get("resolution"))
        .map(|v| serde_json::from_value(v.clone()))
        .transpose()?;

    let mut gap = 0u64;
    for (index, item) in items.iter().enumerate() {
        let at = format!("track.children[{index}]");
        schema(item, &["Gap.1", "Clip.1", "Clip.2"], &at)?;
        simple(item, false, &at)?;
        let (source_in, duration) = range(
            field(item, "source_range", &format!("{at}.source_range"))?,
            rate,
            &at,
        )?;
        if duration == 0 {
            return Err(Error::invalid(format!("{at}.duration")));
        }
        if item.get("OTIO_SCHEMA").and_then(Value::as_str) == Some("Gap.1") {
            if source_in != 0 || namespace(item, &at)?.is_some() {
                return Err(Error::unsupported(&at));
            }
            gap = gap
                .checked_add(duration)
                .filter(|f| *f <= MAX_FRAMES)
                .ok_or_else(|| Error::new("frame_limit", &at))?;
            continue;
        }
        let reference = reference(item, &at)?;
        schema(
            reference,
            &["ExternalReference.1", "MissingReference.1"],
            &format!("{at}.reference"),
        )?;
        let media = if reference.get("OTIO_SCHEMA").and_then(Value::as_str)
            == Some("ExternalReference.1")
        {
            let mut origin = 0;
            if let Some(available) = reference.get("available_range").filter(|v| !v.is_null()) {
                let (first, length) = range(available, rate, &format!("{at}.available_range"))?;
                if source_in < first
                    || source_in
                        .checked_add(duration)
                        .is_none_or(|end| end > first + length)
                {
                    return Err(Error::invalid(format!("{at}.source_range")));
                }
                origin = first;
            }
            let path = format!("{at}.target_url");
            let url =
                string(reference, "target_url", &path)?.ok_or_else(|| Error::invalid(&path))?;
            budget.field(&path, url, 8192)?;
            Some(Media {
                url: url.to_owned(),
                source_in_frame: Some(source_in - origin),
            })
        } else {
            if source_in != 0 {
                return Err(Error::unsupported(format!("{at}.source_range.start_time")));
            }
            None
        };
        let id = match namespace(item, &at)?.and_then(|v| v.get("id")) {
            Some(v) => v
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| Error::invalid(format!("{at}.metadata.shot_list.id")))?,
            None => "",
        };
        budget.field(&format!("{at}.id"), id, 256)?;
        let name = string(item, "name", &format!("{at}.name"))?;
        budget.name(&format!("{at}.name"), name, id)?;
        list.shots.push(Shot {
            id: id.to_owned(),
            name: name.map(str::to_owned),
            duration_frames: duration,
            gap_before_frames: Some(gap),
            media,
        });
        gap = 0;
    }
    if gap != 0 {
        return Err(Error::unsupported("track.trailing_gap"));
    }
    assign_missing_ids(&mut list.shots);
    list.normalized()
}

fn namespace<'a>(v: &'a Value, at: &str) -> Result<Option<&'a Value>> {
    let Some(meta) = v.get("metadata") else {
        return Ok(None);
    };
    if !meta.is_object() {
        return Err(Error::invalid(format!("{at}.metadata")));
    }
    let namespace = meta.get(OTIO_METADATA_KEY);
    if namespace.is_some_and(|v| !v.is_object()) {
        return Err(Error::invalid(format!("{at}.metadata.shot_list")));
    }
    Ok(namespace)
}

fn reference<'a>(item: &'a Value, at: &str) -> Result<&'a Value> {
    if item.get("OTIO_SCHEMA").and_then(Value::as_str) == Some("Clip.1") {
        if item.get("media_references").is_some() {
            return Err(Error::unsupported(format!("{at}.media_references")));
        }
        return field(item, "media_reference", &format!("{at}.media_reference"));
    }
    if item.get("media_reference").is_some() {
        return Err(Error::unsupported(format!("{at}.media_reference")));
    }
    let map = item
        .get("media_references")
        .and_then(Value::as_object)
        .ok_or_else(|| Error::invalid(format!("{at}.media_references")))?;
    if map.len() != 1 {
        return Err(Error::unsupported(format!("{at}.media_references")));
    }
    let key = string(
        item,
        "active_media_reference_key",
        &format!("{at}.active_media_reference_key"),
    )?
    .unwrap_or(DEFAULT_MEDIA);
    map.get(key)
        .ok_or_else(|| Error::invalid(format!("{at}.active_media_reference_key")))
}

fn simple(v: &Value, composition: bool, at: &str) -> Result<()> {
    if let Some(enabled) = v.get("enabled") {
        if enabled != &Value::Bool(true) {
            return Err(Error::unsupported(format!("{at}.enabled")));
        }
    }
    for key in ["effects", "markers"] {
        if v.get(key)
            .is_some_and(|v| v.as_array().is_none_or(|a| !a.is_empty()))
        {
            return Err(Error::unsupported(format!("{at}.{key}")));
        }
    }
    if composition && v.get("source_range").is_some_and(|v| !v.is_null()) {
        return Err(Error::unsupported(format!("{at}.source_range")));
    }
    Ok(())
}

fn schema(v: &Value, names: &[&str], at: &str) -> Result<()> {
    if !v
        .get("OTIO_SCHEMA")
        .and_then(Value::as_str)
        .is_some_and(|s| names.contains(&s))
    {
        return Err(Error::unsupported(format!("{at}.OTIO_SCHEMA")));
    }
    Ok(())
}
fn field<'a>(v: &'a Value, name: &str, at: &str) -> Result<&'a Value> {
    v.get(name)
        .filter(|v| !v.is_null())
        .ok_or_else(|| Error::invalid(at))
}
fn string<'a>(v: &'a Value, name: &str, at: &str) -> Result<Option<&'a str>> {
    match v.get(name) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        _ => Err(Error::invalid(at)),
    }
}
fn children<'a>(v: &'a Value, at: &str) -> Result<&'a [Value]> {
    v.get("children")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| Error::invalid(at))
}
fn range(v: &Value, rate: FrameRate, at: &str) -> Result<(u64, u64)> {
    schema(v, &["TimeRange.1"], at)?;
    let start = frames_of(field(v, "start_time", at)?, rate, at)?;
    let duration = frames_of(field(v, "duration", at)?, rate, at)?;
    if start
        .checked_add(duration)
        .is_none_or(|end| end > MAX_FRAMES)
    {
        return Err(Error::new("frame_limit", at));
    }
    Ok((start, duration))
}

// Whole-valued input times only. Different supported rates rescale with u128,
// never with a floating-point multiply or a permissive rounding tolerance.
fn frames_of(v: &Value, rate: FrameRate, at: &str) -> Result<u64> {
    schema(v, &["RationalTime.1"], at)?;
    let value = v
        .get("value")
        .and_then(Value::as_f64)
        .ok_or_else(|| Error::invalid(at))?;
    let fps = v
        .get("rate")
        .and_then(Value::as_f64)
        .ok_or_else(|| Error::invalid(at))?;
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value > MAX_FRAMES as f64 {
        return Err(Error::unsupported(at));
    }
    let source_rate = if fps == rate.as_f64() {
        rate
    } else {
        infer_rate(fps)?
    };
    let numerator = u128::from(value as u64) * u128::from(rate.num) * u128::from(source_rate.den);
    let denominator = u128::from(rate.den) * u128::from(source_rate.num);
    if numerator % denominator != 0 || numerator / denominator > u128::from(MAX_FRAMES) {
        return Err(Error::unsupported(at));
    }
    Ok((numerator / denominator) as u64)
}
fn infer_rate(fps: f64) -> Result<FrameRate> {
    if fps.is_finite() && fps.fract() == 0.0 && (1.0..=1000.0).contains(&fps) {
        return Ok(FrameRate::new(fps as u32, 1));
    }
    [24_000, 30_000, 60_000, 120_000]
        .into_iter()
        .map(|num| FrameRate::new(num, 1001))
        .find(|r| (r.as_f64() - fps).abs() < 1e-10)
        .ok_or_else(|| Error::unsupported("frame_rate"))
}
