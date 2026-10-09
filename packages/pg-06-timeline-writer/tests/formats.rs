// SPDX-License-Identifier: MIT OR Apache-2.0

use serde_json::{Value, json};
use timeline_writer::*;

fn sample() -> ShotList {
    from_json(include_str!("../samples/synthetic.json")).unwrap()
}
fn otio() -> Value {
    serde_json::from_str(&to_otio(&sample()).unwrap()).unwrap()
}
fn read(v: &Value) -> Result<ShotList> {
    from_otio(&v.to_string())
}
fn children(v: &mut Value) -> &mut Vec<Value> {
    v["tracks"]["children"][0]["children"]
        .as_array_mut()
        .unwrap()
}

#[test]
fn whole_document_roundtrip_all_formats_rates_and_defaults() {
    for rate in [
        FrameRate::new(24, 1),
        FrameRate::new(25, 1),
        FrameRate::new(30, 1),
        FrameRate::new(24000, 1001),
        FrameRate::new(30000, 1001),
        FrameRate::new(60000, 1001),
        FrameRate::new(120, 1),
        FrameRate::new(12345, 543),
        FrameRate::new(u32::MAX, u32::MAX - 1),
    ] {
        let mut original = sample();
        original.frame_rate = rate;
        if rate.as_f64() > 100.0 {
            original.record_start = Some("01:00:00:000".into());
        }
        let original = original.normalized().unwrap();
        let via_otio = from_otio(&to_otio(&original).unwrap()).unwrap();
        let via_xml = from_fcpxml(&to_fcpxml(&via_otio).unwrap()).unwrap();
        let final_list = from_json(&to_json(&via_xml).unwrap()).unwrap();
        assert_eq!(final_list, original, "rate {rate}");
        assert_eq!(from_otio(&to_otio(&via_xml).unwrap()).unwrap(), original);
        let mut defaults = original.clone();
        defaults.title = None;
        defaults.record_start = None;
        defaults.resolution = None;
        assert_eq!(
            from_fcpxml(&to_fcpxml(&defaults).unwrap()).unwrap(),
            defaults
        );
    }
}

#[test]
fn empty_and_placeholder_only_timelines_roundtrip() {
    for with_shots in [false, true] {
        let mut list = ShotList::new(FrameRate::new(24, 1));
        if with_shots {
            list.shots.push(Shot::new("missing", 48));
        }
        assert_eq!(from_otio(&to_otio(&list).unwrap()).unwrap(), list);
        assert_eq!(from_fcpxml(&to_fcpxml(&list).unwrap()).unwrap(), list);
    }
}

#[test]
fn maximum_exact_positions_have_bounded_fcpxml_numerators() {
    let mut list = ShotList::new(FrameRate::new(u32::MAX, u32::MAX - 1));
    let mut shot = Shot::new("bound", 1);
    shot.media = Some(Media {
        url: "file:///synthetic/limit.mov".into(),
        source_in_frame: Some(MAX_FRAMES - 1),
    });
    shot.gap_before_frames = Some(MAX_FRAMES - 1);
    list.shots.push(shot);
    let xml = to_fcpxml(&list).unwrap();
    let bare = xml.replace("<!DOCTYPE fcpxml>", "");
    let doc = roxmltree::Document::parse(&bare).unwrap();
    for attr in doc.descendants().flat_map(|n| n.attributes()) {
        if let Some(raw) = attr.value().strip_suffix('s') {
            if let Some((n, d)) = raw.split_once('/') {
                assert!(n.parse::<i64>().is_ok());
                assert!(d.parse::<u32>().is_ok());
            }
        }
    }
    assert_eq!(from_fcpxml(&xml).unwrap(), list);
    assert_eq!(from_otio(&to_otio(&list).unwrap()).unwrap(), list);
}

#[test]
fn xml_assets_are_shared_and_cover_used_source_ranges() {
    let xml = to_fcpxml(&sample()).unwrap();
    let bare = xml.replace("<!DOCTYPE fcpxml>", "");
    let doc = roxmltree::Document::parse(&bare).unwrap();
    let assets: Vec<_> = doc
        .descendants()
        .filter(|n| n.has_tag_name("asset"))
        .collect();
    assert_eq!(assets.len(), 2);
    assert_eq!(assets[0].attribute("duration"), Some("9009/2000s")); // 108 * 1001 / 24000
    let clips: Vec<_> = doc
        .descendants()
        .filter(|n| n.has_tag_name("asset-clip"))
        .collect();
    assert_eq!(clips[0].attribute("ref"), clips[1].attribute("ref"));
    assert_eq!(clips[0].attribute("offset"), Some("18018/5s")); // 86400 * 1001 / 24000
    assert_eq!(
        doc.descendants()
            .filter(|n| n.has_tag_name("md") && n.attribute("key") == Some(SHOT_ID_KEY))
            .count(),
        4
    );
    assert!(xml.contains("&lt;orbit&gt; &amp;") && xml.contains("&quot;amber&quot;"));
    assert!(xml.contains("variant=1&amp;take=2"));
    assert_eq!(to_fcpxml(&sample()).unwrap(), xml);
}

#[test]
fn otio_schema_and_missing_reference_are_native_shapes() {
    let v = otio();
    assert_eq!(v["OTIO_SCHEMA"], "Timeline.1");
    assert_eq!(v["tracks"]["children"][0]["kind"], "Video");
    let items = v["tracks"]["children"][0]["children"].as_array().unwrap();
    assert_eq!(items.len(), 6);
    assert_eq!(items[1]["OTIO_SCHEMA"], "Gap.1");
    assert_eq!(
        items[4]["media_references"]["DEFAULT_MEDIA"]["OTIO_SCHEMA"],
        "MissingReference.1"
    );
    assert_eq!(items[4]["metadata"]["shot_list"]["id"], "कक्षा-003");
}

#[test]
fn external_otio_without_metadata_derives_unique_ids_and_rate() {
    let mut v = otio();
    v["metadata"] = json!({});
    for item in children(&mut v) {
        item["metadata"] = json!({});
        if item["OTIO_SCHEMA"] == "Clip.2" {
            item["name"] = json!("same-name");
        }
    }
    let list = read(&v).unwrap();
    assert_eq!(list.frame_rate, FrameRate::new(24000, 1001));
    assert_eq!(
        list.shots.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        ["same-name", "shot-2", "shot-3", "shot-4"]
    );
    let mut v = otio();
    children(&mut v)[0]["metadata"] = json!({});
    children(&mut v)[0]["name"] = json!("शॉट");
    assert_eq!(read(&v).unwrap().shots[0].id, "शॉट");
}

#[test]
fn otio_clip1_and_exact_mixed_rate_conversion_are_supported() {
    let mut list = ShotList::new(FrameRate::new(24, 1));
    let mut s = Shot::new("external", 24);
    s.media = Some(Media {
        url: "file:///synthetic/a.mov".into(),
        source_in_frame: Some(12),
    });
    list.shots.push(s);
    let mut v: Value = serde_json::from_str(&to_otio(&list).unwrap()).unwrap();
    let clip = &mut children(&mut v)[0];
    clip["OTIO_SCHEMA"] = json!("Clip.1");
    let object = clip.as_object_mut().unwrap();
    let reference = object.remove("media_references").unwrap()["DEFAULT_MEDIA"].clone();
    object.remove("active_media_reference_key");
    object.insert("media_reference".into(), reference);
    clip["source_range"]["start_time"]["rate"] = json!(48);
    clip["source_range"]["start_time"]["value"] = json!(24);
    clip["source_range"]["duration"]["rate"] = json!(48);
    clip["source_range"]["duration"]["value"] = json!(48);
    assert_eq!(read(&v).unwrap(), list);
    children(&mut v)[0]["source_range"]["duration"]["value"] = json!(47);
    assert!(read(&v).is_err());
}

#[test]
fn otio_rejects_unsupported_semantics_and_wrong_types() {
    for (pointer, value) in [
        ("/tracks/children/0/kind", json!("Audio")),
        ("/tracks/children/0/enabled", json!(false)),
        ("/tracks/children/0/source_range", json!({})),
        (
            "/tracks/children/0/effects",
            json!([{"OTIO_SCHEMA":"TimeEffect.1"}]),
        ),
        (
            "/tracks/children/0/children/0/OTIO_SCHEMA",
            json!("Transition.1"),
        ),
        (
            "/tracks/children/0/children/0/OTIO_SCHEMA",
            json!("Clip.999"),
        ),
        ("/tracks/children/0/children/0/enabled", json!(false)),
        ("/tracks/children/0/children/0/effects", json!([{}])),
        ("/tracks/children/0/children/0/markers", json!([{}])),
        ("/tracks/children/0/children/0/source_range", Value::Null),
        (
            "/tracks/children/0/children/0/source_range/duration/value",
            json!(1.01),
        ),
        (
            "/tracks/children/0/children/0/source_range/duration/rate",
            json!(0),
        ),
        (
            "/tracks/children/0/children/0/active_media_reference_key",
            json!("absent"),
        ),
        (
            "/tracks/children/0/children/0/metadata/shot_list/id",
            json!(8),
        ),
        (
            "/tracks/children/0/children/0/metadata/shot_list/id",
            json!(""),
        ),
        ("/metadata/shot_list/version", json!(2)),
        ("/name", json!(false)),
    ] {
        let mut v = otio();
        *v.pointer_mut(pointer).unwrap() = value;
        assert!(read(&v).is_err(), "{pointer}");
    }
    let mut v = otio();
    let other = v["tracks"]["children"][0].clone();
    v["tracks"]["children"].as_array_mut().unwrap().push(other);
    assert!(read(&v).is_err());
    let mut v = otio();
    let trailing = children(&mut v)[1].clone();
    children(&mut v).push(trailing);
    assert!(read(&v).is_err());
    let mut v = otio();
    children(&mut v)[0]["media_references"]["alternate"] =
        json!({"OTIO_SCHEMA":"MissingReference.1"});
    assert!(read(&v).is_err());
}

#[test]
fn otio_bounds_and_fractional_times_are_not_rounded() {
    let mut v = otio();
    children(&mut v)[0]["media_references"]["DEFAULT_MEDIA"]["available_range"] = json!({
        "OTIO_SCHEMA":"TimeRange.1",
        "start_time":{"OTIO_SCHEMA":"RationalTime.1","rate":24000.0/1001.0,"value":0},
        "duration":{"OTIO_SCHEMA":"RationalTime.1","rate":24000.0/1001.0,"value":48}
    });
    assert!(read(&v).is_err()); // source end is 72, outside the available range
    let mut v = otio();
    children(&mut v)[0]["source_range"]["duration"]["value"] = json!(48.0001);
    assert!(read(&v).is_err());
    let mut v = otio();
    children(&mut v)[4]["source_range"]["start_time"]["value"] = json!(10);
    assert!(read(&v).is_err()); // placeholder cannot represent source in
}

const EXTERNAL: &str = r#"<fcpxml version="1.9"><resources>
<format id="fmt" frameDuration="1/24s" width="1920" height="1080"/>
<asset id="media" start="3600s" duration="4s" hasVideo="1" hasAudio="0" format="fmt"><media-rep src="file:///synthetic/external.mov"/></asset>
</resources><project name="External synthetic"><sequence format="fmt" tcStart="3600s" tcFormat="NDF" duration="3s"><spine>
<gap offset="3600s" start="3600s" duration="1s"/>
<asset-clip ref="media" offset="3601s" start="3601s" duration="2s" name="External shot"/>
</spine></sequence></project></fcpxml>"#;

#[test]
fn fcpxml_external_absolute_source_and_implicit_gaps_are_mapped() {
    let list = from_fcpxml(EXTERNAL).unwrap();
    assert_eq!(list.record_start.as_deref(), Some("01:00:00:00"));
    assert_eq!(list.shots[0].id, "External shot");
    assert_eq!(list.shots[0].gap_before_frames, Some(24));
    assert_eq!(
        list.shots[0].media.as_ref().unwrap().source_in_frame,
        Some(24)
    );
    let implicit = EXTERNAL.replace(
        "<gap offset=\"3600s\" start=\"3600s\" duration=\"1s\"/>",
        "",
    );
    assert_eq!(from_fcpxml(&implicit).unwrap(), list);
    assert_eq!(from_fcpxml(&to_fcpxml(&list).unwrap()).unwrap(), list);
}

#[test]
fn fcpxml_rejects_features_missing_resources_and_nonframe_times() {
    for (old, new) in [
        ("version=\"1.9\"", "version=\"1.10\""),
        ("tcFormat=\"NDF\"", "tcFormat=\"DF\""),
        ("hasAudio=\"0\"", "hasAudio=\"1\""),
        ("hasVideo=\"1\"", "hasVideo=\"0\""),
        ("<spine>", "<spine lane=\"1\">"),
        ("ref=\"media\"", "ref=\"absent\""),
        ("offset=\"3601s\"", "offset=\"3600s\""),
        ("start=\"3601s\"", "start=\"3599s\""),
        ("duration=\"2s\"", "duration=\"0s\""),
        ("duration=\"2s\"", "duration=\"1/25s\""),
        ("duration=\"2s\"", "duration=\"7s\""),
        ("duration=\"3s\"", "duration=\"9s\""),
        (
            "name=\"External shot\"",
            "name=\"External shot\" enabled=\"0\"",
        ),
        (
            "name=\"External shot\"",
            "name=\"External shot\" lane=\"1\"",
        ),
        ("<spine>", "<spine><transition duration=\"1s\"/>"),
        (
            "<format id=\"fmt\"",
            "<format id=\"fmt\" fieldOrder=\"upper first\"",
        ),
        ("<asset-clip ref", "<asset-clip srcEnable=\"audio\" ref"),
        ("<media-rep src", "<media-rep kind=\"proxy-media\" src"),
        ("<spine>", "<spine><unknown/>"),
        (
            "<fcpxml version",
            "<fcpxml xmlns=\"urn:unsupported\" version",
        ),
        (
            "duration=\"2s\" name=\"External shot\"/>",
            "duration=\"2s\" name=\"External shot\"><adjust-transform position=\"1 2\"/></asset-clip>",
        ),
    ] {
        let input = EXTERNAL.replacen(old, new, 1);
        assert_ne!(input, EXTERNAL);
        assert!(from_fcpxml(&input).is_err(), "{old} -> {new}");
    }
    let trailing = EXTERNAL.replace(
        "</spine>",
        "<gap offset=\"3603s\" duration=\"1s\"/></spine>",
    );
    assert!(from_fcpxml(&trailing).is_err());
    let multiple = EXTERNAL.replace(
        "</resources>",
        "<format id=\"media\" frameDuration=\"1/24s\" width=\"1920\" height=\"1080\"/></resources>",
    );
    assert!(from_fcpxml(&multiple).is_err());
    let duplicate=EXTERNAL.replace("<spine>","<spine><asset-clip ref=\"media\" offset=\"3600s\" start=\"3600s\" duration=\"1s\"><metadata><md key=\"shot_list.id\" value=\"x\"/><md key=\"shot_list.id\" value=\"y\"/></metadata></asset-clip>");
    assert!(from_fcpxml(&duplicate).is_err());
}

#[test]
fn external_entities_and_internal_dtd_are_never_resolved() {
    for declaration in [
        "<!DOCTYPE fcpxml SYSTEM \"https://example.invalid/external.dtd\">",
        "<!DOCTYPE fcpxml [<!ENTITY invented \"private\">]>",
        "<!DOCTYPE fcpxml [<!ENTITY invented SYSTEM \"file:///synthetic/private\">]>",
    ] {
        assert!(from_fcpxml(&format!("{declaration}{EXTERNAL}")).is_err());
    }
    assert!(from_fcpxml(&format!("<!DOCTYPE\n fcpxml >{EXTERNAL}")).is_ok());
    assert!(from_fcpxml(&format!("<?invented action?>{EXTERNAL}")).is_err());
    assert!(from_fcpxml(&" ".repeat(MAX_TIMELINE_BYTES + 1)).is_err());
}

#[test]
fn export_is_deterministic_and_preserves_adversarial_unicode_ids() {
    let mut list = sample();
    list.title = Some("“אבג” & कक्षा / e\u{301} / 镜头".into());
    list.shots[0].id = "a<\"&'>".into();
    list.shots[0].name = Some("Apostrophe ' and quotes \"".into());
    let xml = to_fcpxml(&list).unwrap();
    assert_eq!(from_fcpxml(&xml).unwrap(), list);
    assert_eq!(from_otio(&to_otio(&list).unwrap()).unwrap(), list);
    assert_eq!(to_fcpxml(&list).unwrap(), xml);
}
