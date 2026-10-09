// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::bounded::{MAX_TIMELINE_BYTES, json_string};
use crate::{FrameRate, Result, SCHEMA_VERSION, Shot, ShotList};
use serde_json::{Value, json};

/// Metadata namespace on timelines and clips.
pub const OTIO_METADATA_KEY: &str = "shot_list";
pub(crate) const DEFAULT_MEDIA: &str = "DEFAULT_MEDIA";

/// Writes Timeline.1 → Stack.1 → one Video Track.1, with Clip.2 and Gap.1.
pub fn to_otio(list: &ShotList) -> Result<String> {
    let list = list.normalized()?;
    let rate = list.frame_rate;
    let mut children = Vec::with_capacity(list.shots.len() * 2);
    for shot in &list.shots {
        if let Some(gap) = shot.gap_before_frames {
            children.push(json!({
                "OTIO_SCHEMA": "Gap.1", "name": "", "metadata": {},
                "source_range": range(0, gap, rate),
                "effects": [], "markers": [], "enabled": true,
            }));
        }
        children.push(clip(shot, rate));
    }
    let mut timeline = json!({
        "OTIO_SCHEMA": "Timeline.1",
        "name": list.title.as_deref().unwrap_or(""),
        "metadata": { OTIO_METADATA_KEY: {
            "version": SCHEMA_VERSION,
            "frame_rate": rate,
            "resolution": list.resolution_or_default(),
        }},
        "global_start_time": time(list.start_frame()?, rate),
        "tracks": {
            "OTIO_SCHEMA": "Stack.1", "name": "tracks", "metadata": {},
            "source_range": null, "effects": [], "markers": [], "enabled": true,
            "children": [{
                "OTIO_SCHEMA": "Track.1", "name": "V1", "kind": "Video", "metadata": {},
                "source_range": null, "effects": [], "markers": [], "enabled": true,
                "children": [],
            }],
        },
    });
    timeline["tracks"]["children"][0]["children"] = Value::Array(children);
    json_string(&timeline, true, MAX_TIMELINE_BYTES)
}

fn clip(shot: &Shot, rate: FrameRate) -> Value {
    let reference = match &shot.media {
        Some(m) => json!({
            "OTIO_SCHEMA": "ExternalReference.1", "name": "", "metadata": {},
            "available_range": null, "available_image_bounds": null, "target_url": m.url,
        }),
        None => json!({
            "OTIO_SCHEMA": "MissingReference.1", "name": "", "metadata": {},
            "available_range": null, "available_image_bounds": null,
        }),
    };
    let source_in = shot
        .media
        .as_ref()
        .and_then(|m| m.source_in_frame)
        .unwrap_or(0);
    json!({
        "OTIO_SCHEMA": "Clip.2", "name": shot.display_name(),
        "metadata": { OTIO_METADATA_KEY: { "id": shot.id } },
        "source_range": range(source_in, shot.duration_frames, rate),
        "effects": [], "markers": [], "enabled": true,
        "media_references": { DEFAULT_MEDIA: reference },
        "active_media_reference_key": DEFAULT_MEDIA,
    })
}

fn time(frames: u64, rate: FrameRate) -> Value {
    json!({ "OTIO_SCHEMA": "RationalTime.1", "rate": rate.as_f64(), "value": frames as f64 })
}

fn range(start: u64, duration: u64, rate: FrameRate) -> Value {
    json!({
        "OTIO_SCHEMA": "TimeRange.1",
        "start_time": time(start, rate), "duration": time(duration, rate),
    })
}
