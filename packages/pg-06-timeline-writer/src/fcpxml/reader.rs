// SPDX-License-Identifier: MIT OR Apache-2.0

use super::{SHOT_ID_KEY, frames, rational};
use crate::bounded::MAX_TIMELINE_BYTES;
use crate::model::{TextBudget, assign_missing_ids, check_url, text_size};
use crate::{
    Error, FrameRate, MAX_FRAMES, MAX_SHOTS, Media, Result, Shot, ShotList, format_timecode,
};
use roxmltree::{Document, Node, ParsingOptions};
use std::borrow::Cow;
use std::collections::HashMap;

type Element<'a> = Node<'a, 'a>;

/// Reads a flat, progressive, video-only FCPXML 1.9 project.
/// Connected clips, effects, audio, timing changes and trailing gaps are errors.
pub fn from_fcpxml(text: &str) -> Result<ShotList> {
    if text.len() > MAX_TIMELINE_BYTES {
        return Err(Error::new("input_limit", "input"));
    }
    let text = header_doctype(text)?;
    let doc = Document::parse_with_options(
        &text,
        ParsingOptions {
            allow_dtd: false,
            nodes_limit: 300_000,
            entity_resolver: None,
        },
    )?;
    for node in doc.descendants() {
        if node.is_pi()
            || node.is_element()
                && (node.tag_name().namespace().is_some()
                    || node.attributes().any(|a| a.namespace().is_some()))
        {
            return Err(Error::unsupported(
                "input.namespaces_or_processing_instructions",
            ));
        }
        if node.ancestors().take(34).count() > 32 {
            return Err(Error::new("depth_limit", "input"));
        }
        if node.is_text() && node.text().is_some_and(|s| !s.trim().is_empty()) {
            return Err(Error::unsupported("input.text"));
        }
    }
    let root = doc.root_element();
    if !root.has_tag_name("fcpxml") || root.attribute("version") != Some("1.9") {
        return Err(Error::unsupported("fcpxml.version"));
    }
    attrs(root, &["version"], "fcpxml")?;
    allowed_children(
        root,
        &["resources", "library", "event", "project"],
        "fcpxml",
    )?;
    let resources = one(root, "resources", "resources")?;
    let mut formats = HashMap::new();
    let mut assets = HashMap::new();
    let mut ids = std::collections::HashSet::new();
    allowed_children(resources, &["format", "asset"], "resources")?;
    attrs(resources, &[], "resources")?;
    for (i, node) in resources.children().filter(Node::is_element).enumerate() {
        let at = format!("resources[{i}]");
        let id = required(node, "id", &at)?;
        if id.is_empty() || id.len() > 256 || !ids.insert(id) {
            return Err(Error::invalid(format!("{at}.id")));
        }
        if node.has_tag_name("format") {
            formats.insert(id, node);
        } else {
            assets.insert(id, node);
        }
    }
    let containers: Vec<_> = root
        .children()
        .filter(|n| n.is_element() && !n.has_tag_name("resources"))
        .collect();
    if containers.len() != 1 {
        return Err(Error::unsupported("fcpxml.projects"));
    }
    let mut container = containers[0];
    if container.has_tag_name("library") {
        attrs(container, &["location", "colorProcessing"], "library")?;
        if container
            .attribute("colorProcessing")
            .is_some_and(|s| s != "standard")
        {
            return Err(Error::unsupported("library.colorProcessing"));
        }
        allowed_children(container, &["event"], "library")?;
        container = one(container, "event", "library.event")?;
    }
    if container.has_tag_name("event") {
        attrs(container, &["name", "uid"], "event")?;
        allowed_children(container, &["project"], "event")?;
        container = one(container, "project", "event.project")?;
    }
    if !container.has_tag_name("project") {
        return Err(Error::unsupported("project"));
    }
    attrs(container, &["name", "uid", "id", "modDate"], "project")?;
    allowed_children(container, &["sequence"], "project")?;
    let sequence = one(container, "sequence", "project.sequence")?;
    attrs(
        sequence,
        &[
            "format",
            "duration",
            "tcStart",
            "tcFormat",
            "audioLayout",
            "audioRate",
        ],
        "sequence",
    )?;
    if sequence.attribute("tcFormat").is_some_and(|v| v != "NDF") {
        return Err(Error::unsupported("sequence.tcFormat"));
    }
    allowed_children(sequence, &["spine", "metadata"], "sequence")?;
    let format = formats
        .get(required(sequence, "format", "sequence.format")?)
        .ok_or_else(|| Error::invalid("sequence.format"))?;
    let (rate, resolution) = read_format(*format)?;
    // Validate even unused resources; do not quietly strip unsupported audio assets.
    for node in formats.values() {
        read_format(*node)?;
    }
    let mut media_assets = HashMap::new();
    for (id, node) in assets {
        attrs(
            node,
            &[
                "id",
                "name",
                "uid",
                "start",
                "duration",
                "hasVideo",
                "hasAudio",
                "format",
                "videoSources",
            ],
            "asset",
        )?;
        if node.attribute("hasVideo") != Some("1")
            || node.attribute("hasAudio").is_some_and(|v| v != "0")
            || node.attribute("videoSources").is_some_and(|v| v != "1")
        {
            return Err(Error::unsupported("asset.video_only"));
        }
        if let Some(reference) = node.attribute("format") {
            let other = formats
                .get(reference)
                .ok_or_else(|| Error::invalid("asset.format"))?;
            if read_format(*other)? != (rate, resolution) {
                return Err(Error::unsupported("asset.format"));
            }
        }
        allowed_children(node, &["media-rep", "metadata"], "asset")?;
        let rep = one(node, "media-rep", "asset.media-rep")?;
        attrs(
            rep,
            &["kind", "src", "sig", "suggestedFilename"],
            "asset.media-rep",
        )?;
        allowed_children(rep, &[], "asset.media-rep")?;
        if rep.attribute("kind").is_some_and(|v| v != "original-media") {
            return Err(Error::unsupported("asset.media-rep.kind"));
        }
        let start = frames(node.attribute("start").unwrap_or("0s"), rate, "asset.start")?;
        let length = node
            .attribute("duration")
            .map(|s| frames(s, rate, "asset.duration"))
            .transpose()?;
        if length.is_some_and(|n| n == 0 || start.checked_add(n).is_none_or(|e| e > MAX_FRAMES)) {
            return Err(Error::invalid("asset.duration"));
        }
        metadata(node)?;
        text_size(
            "asset.media-rep.src",
            required(rep, "src", "asset.media-rep.src")?,
            8192,
        )?;
        check_url(
            "asset.media-rep.src",
            required(rep, "src", "asset.media-rep.src")?,
        )?;
        media_assets.insert(
            id,
            Asset {
                url: required(rep, "src", "asset.media-rep.src")?,
                start,
                length,
            },
        );
    }
    metadata(sequence)?;
    let record_start = frames(
        sequence.attribute("tcStart").unwrap_or("0s"),
        rate,
        "sequence.tcStart",
    )?;
    let spine = one(sequence, "spine", "sequence.spine")?;
    attrs(spine, &["name", "offset", "lane", "format"], "spine")?;
    if spine.attribute("lane").is_some_and(|v| v != "0")
        || spine.attribute("offset").is_some_and(|v| v != "0s")
        || spine
            .attribute("format")
            .is_some_and(|v| Some(v) != sequence.attribute("format"))
    {
        return Err(Error::unsupported("spine"));
    }
    allowed_children(spine, &["asset-clip", "gap"], "spine")?;
    let mut list = ShotList::new(rate);
    let mut budget = TextBudget::default();
    budget.field(
        "project.name",
        container.attribute("name").unwrap_or(""),
        4096,
    )?;
    list.title = container.attribute("name").map(str::to_owned);
    list.resolution = Some(resolution);
    list.record_start = Some(format_timecode(record_start, rate)?);
    let mut cursor = record_start;
    let mut pending = 0u64;
    let items: Vec<_> = spine.children().filter(Node::is_element).collect();
    if items.len() > MAX_SHOTS * 2 {
        return Err(Error::new("record_limit", "spine"));
    }
    for (index, item) in items.into_iter().enumerate() {
        let at = format!("spine[{index}]");
        attrs(
            item,
            &[
                "name",
                "offset",
                "start",
                "duration",
                "enabled",
                "ref",
                "srcEnable",
                "lane",
                "format",
                "tcStart",
                "tcFormat",
                "modDate",
                "videoRole",
            ],
            &at,
        )?;
        allowed_children(item, &["metadata"], &at)?;
        if item.attribute("enabled").is_some_and(|v| v != "1")
            || item.attribute("lane").is_some_and(|v| v != "0")
        {
            return Err(Error::unsupported(&at));
        }
        if item.attribute("tcFormat").is_some_and(|v| v != "NDF")
            || item
                .attribute("format")
                .is_some_and(|v| Some(v) != sequence.attribute("format"))
            || item
                .attribute("srcEnable")
                .is_some_and(|v| !matches!(v, "video" | "all"))
        {
            return Err(Error::unsupported(&at));
        }
        let offset = frames(
            item.attribute("offset").unwrap_or("0s"),
            rate,
            &format!("{at}.offset"),
        )?;
        if offset < cursor {
            return Err(Error::unsupported(format!("{at}.overlap")));
        }
        pending = pending
            .checked_add(offset - cursor)
            .filter(|n| *n <= MAX_FRAMES)
            .ok_or_else(|| Error::new("frame_limit", &at))?;
        let duration = frames(
            required(item, "duration", &at)?,
            rate,
            &format!("{at}.duration"),
        )?;
        if duration == 0 {
            return Err(Error::invalid(format!("{at}.duration")));
        }
        cursor = offset
            .checked_add(duration)
            .filter(|n| *n <= MAX_FRAMES)
            .ok_or_else(|| Error::new("frame_limit", &at))?;
        let source = frames(
            item.attribute("start").unwrap_or("0s"),
            rate,
            &format!("{at}.start"),
        )?;
        let id = metadata(item)?;
        let name = item.attribute("name");
        let media = if item.has_tag_name("asset-clip") {
            let asset = media_assets
                .get(required(item, "ref", &at)?)
                .ok_or_else(|| Error::invalid(format!("{at}.ref")))?;
            let source_in = source
                .checked_sub(asset.start)
                .ok_or_else(|| Error::invalid(format!("{at}.start")))?;
            if source_in.checked_add(duration).is_none_or(|end| {
                end > MAX_FRAMES || asset.length.is_some_and(|length| end > length)
            }) {
                return Err(Error::invalid(format!("{at}.source_range")));
            }
            budget.field(&format!("{at}.media.url"), asset.url, 8192)?;
            Some(Media {
                url: asset.url.to_owned(),
                source_in_frame: Some(source_in),
            })
        } else {
            if item.attribute("ref").is_some() || item.attribute("srcEnable").is_some() {
                return Err(Error::invalid(&at));
            }
            // A gap's local start does not affect a flat gap's record position.
            if id.is_none() {
                pending = pending
                    .checked_add(duration)
                    .filter(|n| *n <= MAX_FRAMES)
                    .ok_or_else(|| Error::new("frame_limit", &at))?;
                continue;
            }
            None
        };
        budget.field(&format!("{at}.id"), id.unwrap_or(""), 256)?;
        budget.name(&format!("{at}.name"), name, id.unwrap_or(""))?;
        list.shots.push(Shot {
            id: id.unwrap_or("").to_owned(),
            name: name.map(str::to_owned),
            duration_frames: duration,
            gap_before_frames: Some(pending),
            media,
        });
        pending = 0;
    }
    if pending != 0 {
        return Err(Error::unsupported("spine.trailing_gap"));
    }
    if let Some(duration) = sequence.attribute("duration") {
        if frames(duration, rate, "sequence.duration")? != cursor - record_start {
            return Err(Error::invalid("sequence.duration"));
        }
    }
    assign_missing_ids(&mut list.shots);
    list.normalized()
}

struct Asset<'a> {
    url: &'a str,
    start: u64,
    length: Option<u64>,
}

// Scan only the XML prolog, skipping complete comments and the XML declaration.
// Leave invalid or later declarations for the parser; never repair root content.
fn header_doctype(text: &str) -> Result<Cow<'_, str>> {
    let space = [' ', '\t', '\r', '\n'];
    let mut cursor = usize::from(text.starts_with('\u{feff}')) * '\u{feff}'.len_utf8();
    loop {
        cursor += text[cursor..].len() - text[cursor..].trim_start_matches(space).len();
        let rest = &text[cursor..];
        if rest.starts_with("<!--") {
            let end = rest
                .find("-->")
                .ok_or_else(|| Error::new("invalid_xml", "input"))?;
            cursor += end + 3;
        } else if rest.starts_with("<?xml")
            && rest
                .as_bytes()
                .get(5)
                .is_some_and(|b| b" \t\r\n".contains(b))
        {
            let end = rest
                .find("?>")
                .ok_or_else(|| Error::new("invalid_xml", "input"))?;
            cursor += end + 2;
        } else if rest.starts_with("<!DOCTYPE") {
            let end = rest
                .find('>')
                .ok_or_else(|| Error::new("invalid_xml", "input"))?;
            let declaration = &rest[9..end];
            if !declaration.starts_with(space) || declaration.trim_matches(space) != "fcpxml" {
                return Err(Error::unsupported("input.doctype"));
            }
            let mut copy = text.to_owned();
            copy.replace_range(cursor..=cursor + end, &" ".repeat(end + 1));
            return Ok(Cow::Owned(copy));
        } else {
            return Ok(Cow::Borrowed(text));
        }
    }
}

fn read_format(node: Element<'_>) -> Result<(FrameRate, [u32; 2])> {
    attrs(
        node,
        &[
            "id",
            "name",
            "frameDuration",
            "width",
            "height",
            "fieldOrder",
            "paspH",
            "paspV",
            "colorSpace",
        ],
        "format",
    )?;
    allowed_children(node, &[], "format")?;
    if node
        .attribute("fieldOrder")
        .is_some_and(|v| v != "progressive")
        || ["paspH", "paspV"]
            .iter()
            .any(|a| node.attribute(*a).is_some_and(|v| v != "1"))
        || node
            .attribute("colorSpace")
            .is_some_and(|v| v != "1-1-1 (Rec. 709)")
    {
        return Err(Error::unsupported("format"));
    }
    let (n, d) = rational(
        required(node, "frameDuration", "format.frameDuration")?,
        "format.frameDuration",
    )?;
    let n = u32::try_from(n).map_err(|_| Error::unsupported("format.frameDuration"))?;
    let rate = FrameRate::new(d, n);
    rate.validate()?;
    let width = required(node, "width", "format.width")?
        .parse()
        .map_err(|_| Error::invalid("format.width"))?;
    let height = required(node, "height", "format.height")?
        .parse()
        .map_err(|_| Error::invalid("format.height"))?;
    if !(1..=32768).contains(&width) || !(1..=32768).contains(&height) {
        return Err(Error::invalid("format.resolution"));
    }
    Ok((rate.reduced(), [width, height]))
}

fn attrs(node: Element<'_>, allowed: &[&str], at: &str) -> Result<()> {
    if node.attributes().any(|a| !allowed.contains(&a.name())) {
        return Err(Error::unsupported(format!("{at}.attributes")));
    }
    Ok(())
}
fn allowed_children(node: Element<'_>, allowed: &[&str], at: &str) -> Result<()> {
    if node
        .children()
        .filter(Node::is_element)
        .any(|n| !allowed.contains(&n.tag_name().name()))
    {
        return Err(Error::unsupported(format!("{at}.children")));
    }
    Ok(())
}
fn one<'a>(node: Element<'a>, tag: &str, at: &str) -> Result<Element<'a>> {
    let mut nodes = node.children().filter(|n| n.has_tag_name(tag));
    let first = nodes.next().ok_or_else(|| Error::invalid(at))?;
    if nodes.next().is_some() {
        return Err(Error::unsupported(at));
    }
    Ok(first)
}
fn required<'a>(node: Element<'a>, name: &str, at: &str) -> Result<&'a str> {
    node.attribute(name).ok_or_else(|| Error::invalid(at))
}
fn metadata(node: Element<'_>) -> Result<Option<&str>> {
    let mut containers = node.children().filter(|n| n.has_tag_name("metadata"));
    let Some(meta) = containers.next() else {
        return Ok(None);
    };
    if containers.next().is_some() {
        return Err(Error::invalid("metadata"));
    }
    attrs(meta, &[], "metadata")?;
    allowed_children(meta, &["md"], "metadata")?;
    let mut keys = std::collections::HashSet::new();
    let mut id = None;
    for item in meta.children().filter(Node::is_element) {
        attrs(
            item,
            &[
                "key",
                "value",
                "editable",
                "type",
                "displayName",
                "description",
                "source",
            ],
            "metadata.md",
        )?;
        allowed_children(item, &[], "metadata.md")?;
        let key = required(item, "key", "metadata.md.key")?;
        if !keys.insert(key) {
            return Err(Error::invalid("metadata.md.key"));
        }
        if key == SHOT_ID_KEY {
            let value = required(item, "value", "metadata.md.value")?;
            text_size("metadata.md.value", value, 256)?;
            if value.is_empty() {
                return Err(Error::invalid("metadata.md.value"));
            }
            id = Some(value);
        }
    }
    Ok(id)
}
