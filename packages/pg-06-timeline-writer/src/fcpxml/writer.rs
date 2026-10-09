// SPDX-License-Identifier: MIT OR Apache-2.0

use super::{SHOT_ID_KEY, time};
use crate::bounded::{MAX_TIMELINE_BYTES, Output};
use crate::{Error, Result, ShotList};
use std::collections::HashMap;
use std::io::Write;

/// FCPXML 1.9: one project, one spine, shared video-only assets, explicit gaps.
/// HTTPS media references are carried through; Final Cut media relinking is not tested.
pub fn to_fcpxml(list: &ShotList) -> Result<String> {
    let list = list.normalized()?;
    let rate = list.frame_rate;
    let [width, height] = list.resolution_or_default();
    let mut lookup = HashMap::new();
    let mut assets: Vec<(&str, u64)> = Vec::new();
    for shot in &list.shots {
        if let Some(media) = &shot.media {
            let end = media.source_in_frame.unwrap_or(0) + shot.duration_frames;
            let index = *lookup.entry(media.url.as_str()).or_insert_with(|| {
                let index = assets.len();
                assets.push((media.url.as_str(), 0));
                index
            });
            assets[index].1 = assets[index].1.max(end);
        }
    }
    let mut out = Output::new(MAX_TIMELINE_BYTES);
    macro_rules! xml {
        ($($arg:tt)*) => { write!(&mut out, $($arg)*).map_err(|_| Error::new("output_limit", "output"))? };
    }
    xml!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE fcpxml>\n<fcpxml version=\"1.9\">\n  <resources>\n"
    );
    xml!(
        "    <format id=\"r1\" frameDuration=\"{}\" width=\"{width}\" height=\"{height}\"/>\n",
        time(1, rate)
    );
    for (index, (url, length)) in assets.iter().enumerate() {
        xml!(
            "    <asset id=\"r{}\" name=\"Media {}\" start=\"0s\" duration=\"{}\" hasVideo=\"1\" hasAudio=\"0\" videoSources=\"1\" format=\"r1\">\n",
            index + 2,
            index + 1,
            time(*length, rate)
        );
        xml!(
            "      <media-rep kind=\"original-media\" src=\"{}\"/>\n    </asset>\n",
            escape(url)
        );
    }
    xml!(
        "  </resources>\n  <library><event name=\"Timeline\"><project name=\"{}\">\n",
        escape(list.title.as_deref().unwrap_or(""))
    );
    xml!(
        "    <sequence format=\"r1\" duration=\"{}\" tcStart=\"{}\" tcFormat=\"NDF\">\n      <spine>\n",
        time(list.duration_frames()?, rate),
        time(list.start_frame()?, rate)
    );
    // A spine inherits sequence local time: its first item's offset is tcStart.
    let mut cursor = list.start_frame()?;
    for shot in &list.shots {
        if let Some(gap) = shot.gap_before_frames {
            xml!(
                "        <gap name=\"Gap\" offset=\"{}\" start=\"0s\" duration=\"{}\"/>\n",
                time(cursor, rate),
                time(gap, rate)
            );
            cursor += gap;
        }
        let tag = if shot.media.is_some() {
            "asset-clip"
        } else {
            "gap"
        };
        xml!(
            "        <{tag} name=\"{}\" offset=\"{}\" start=\"{}\" duration=\"{}\"",
            escape(shot.display_name()),
            time(cursor, rate),
            time(
                shot.media
                    .as_ref()
                    .and_then(|m| m.source_in_frame)
                    .unwrap_or(0),
                rate
            ),
            time(shot.duration_frames, rate)
        );
        if let Some(media) = &shot.media {
            xml!(
                " ref=\"r{}\" srcEnable=\"video\"",
                lookup[media.url.as_str()] + 2
            );
        }
        xml!(
            "><metadata><md key=\"{SHOT_ID_KEY}\" value=\"{}\"/></metadata></{tag}>\n",
            escape(&shot.id)
        );
        cursor += shot.duration_frames;
    }
    xml!("      </spine>\n    </sequence>\n  </project></event></library>\n</fcpxml>\n");
    out.finish()
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}
