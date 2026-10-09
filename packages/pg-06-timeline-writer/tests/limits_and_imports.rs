// SPDX-License-Identifier: MIT OR Apache-2.0

use serde_json::{Value, json};
use timeline_writer::*;

fn list() -> ShotList {
    from_json(include_str!("../samples/synthetic.json")).unwrap()
}
fn otio() -> Value {
    serde_json::from_str(&to_otio(&list()).unwrap()).unwrap()
}
fn items(v: &mut Value) -> &mut Vec<Value> {
    v["tracks"]["children"][0]["children"]
        .as_array_mut()
        .unwrap()
}

#[test]
fn ten_thousand_shots_roundtrip_through_both_timeline_readers() {
    let mut original = ShotList::new(FrameRate::new(30000, 1001));
    for i in 0..MAX_SHOTS {
        let mut shot = Shot::new(format!("s{i}"), 24);
        shot.name = Some("<\"&>".repeat(5));
        shot.gap_before_frames = Some(1);
        if i % 2 == 0 {
            shot.media = Some(Media {
                url: "file:///synthetic/shared.mov".into(),
                source_in_frame: Some(i as u64),
            });
        }
        original.shots.push(shot);
    }
    let original = original.normalized().unwrap();
    assert_eq!(from_otio(&to_otio(&original).unwrap()).unwrap(), original);
    assert_eq!(
        from_fcpxml(&to_fcpxml(&original).unwrap()).unwrap(),
        original
    );
}

#[test]
fn json_value_and_xml_depth_budgets_reject_before_mapping() {
    // 2,000,001 scalar values plus the array/root: under 8MiB but over value budget.
    let many = format!("{{\"ignored\":[{}0]}}", "0,".repeat(2_000_000));
    assert!(many.len() < MAX_JSON_BYTES);
    assert!(from_json(&many).is_err());
    let deep = format!("{}{}", "<a>".repeat(40), "</a>".repeat(40));
    assert_eq!(from_fcpxml(&deep).unwrap_err().code, "depth_limit");
    let mut model = list();
    model.record_start = Some(":".repeat(100_000));
    assert!(model.normalized().is_err());
}

#[test]
fn shot_id_fallback_reserves_supplied_ids_and_handles_long_names() {
    let mut v = otio();
    items(&mut v)[0]["metadata"] = json!({});
    items(&mut v)[0]["name"] = json!("shot-2");
    items(&mut v)[2]["metadata"] = json!({"shot_list":{"id":"shot-2"}});
    items(&mut v)[4]["metadata"] = json!({});
    items(&mut v)[4]["name"] = json!("x".repeat(257));
    let parsed = from_otio(&v.to_string()).unwrap();
    assert_eq!(parsed.shots[0].id, "shot-1");
    assert_eq!(parsed.shots[1].id, "shot-2");
    assert_eq!(parsed.shots[2].id, "shot-3");
}

#[test]
fn external_fcpxml_event_wrapper_empty_projects_and_unsupported_resources() {
    let xml = to_fcpxml(&list()).unwrap();
    let event = xml.replace("<library>", "").replace("</library>", "");
    assert_eq!(from_fcpxml(&event).unwrap(), list());
    let without_resources = xml.replace("<resources>", "<resources unsupported=\"1\">");
    assert!(from_fcpxml(&without_resources).is_err());
    let invalid_unused = xml.replace(
        "</resources>",
        "<format id=\"unused\" frameDuration=\"1/24s\" width=\"0\" height=\"720\"/></resources>",
    );
    assert!(from_fcpxml(&invalid_unused).is_err());
    let unknown_element = xml.replace("<resources>", "<resources><media id=\"unsupported\"/>");
    assert!(from_fcpxml(&unknown_element).is_err());
    let invalid_unused=xml.replace("</resources>","<asset id=\"unused\" hasVideo=\"1\" hasAudio=\"0\"><media-rep src=\"javascript:invented\"/></asset></resources>");
    assert!(from_fcpxml(&invalid_unused).is_err());
}

#[test]
fn invalid_active_and_metadata_shapes_do_not_turn_into_placeholders() {
    for (pointer, value) in [
        ("/metadata", json!([])),
        ("/metadata/shot_list", json!("invented")),
        (
            "/tracks/children/0/children/0/media_references/DEFAULT_MEDIA/OTIO_SCHEMA",
            json!("ImageSequenceReference.1"),
        ),
        (
            "/tracks/children/0/children/0/media_references/DEFAULT_MEDIA/target_url",
            json!(false),
        ),
        ("/tracks/children/0/children/0/media_references", json!({})),
        (
            "/tracks/children/0/children/0/source_range/start_time/OTIO_SCHEMA",
            json!("RationalTime.2"),
        ),
        (
            "/tracks/children/0/children/0/source_range/duration/value",
            json!(MAX_FRAMES + 1),
        ),
        (
            "/tracks/children/0/children/0/source_range/duration/rate",
            json!(23.976),
        ),
        ("/tracks/children/0/children/0/metadata", json!(null)),
    ] {
        let mut v = otio();
        *v.pointer_mut(pointer).unwrap() = value;
        assert!(from_otio(&v.to_string()).is_err(), "{pointer}");
    }
}
