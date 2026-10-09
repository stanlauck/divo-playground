// SPDX-License-Identifier: MIT OR Apache-2.0

use serde_json::{Value, json};
use timeline_writer::*;

fn sample() -> Value {
    serde_json::from_str(include_str!("../samples/synthetic.json")).unwrap()
}
fn decode(v: &Value) -> Result<ShotList> {
    from_json(&v.to_string())
}

#[test]
fn normalizes_defaults_and_ignores_extensions() {
    let mut v = sample();
    v["frame_rate"] = json!({"num": 48, "den": 2, "extension": true});
    v["title"] = json!("");
    v["record_start"] = json!("00:00:00:00");
    v["resolution"] = json!([1920, 1080]);
    v["shots"][0]["name"] = v["shots"][0]["id"].clone();
    v["shots"][0]["gap_before_frames"] = json!(0);
    v["shots"][0]["media"]["source_in_frame"] = json!(0);
    let list = decode(&v).unwrap();
    assert_eq!(list.frame_rate, FrameRate::new(24, 1));
    assert_eq!(
        (list.title, list.record_start, list.resolution),
        (None, None, None)
    );
    assert!(list.shots[0].name.is_none() && list.shots[0].gap_before_frames.is_none());
}

#[test]
fn rates_and_timecodes_are_checked_without_panics() {
    for rate in [
        FrameRate::new(0, 1),
        FrameRate::new(24, 0),
        FrameRate::new(1, 2),
        FrameRate::new(1001, 1),
    ] {
        assert!(parse_timecode("00:00:00:00", rate).is_err());
        assert!(format_timecode(0, rate).is_err());
        assert!(ShotList::new(rate).normalized().is_err());
    }
    let ntsc = FrameRate::new(24000, 1001);
    assert_eq!(parse_timecode("01:00:00:00", ntsc).unwrap(), 86400);
    assert_eq!(format_timecode(86400, ntsc).unwrap(), "01:00:00:00");
    for tc in [
        "1:00:00:00",
        "00:60:00:00",
        "00:00:60:00",
        "00:00:00:24",
        "00:00:00;00",
        "-1:00:00:00",
        "00:00:00:000",
    ] {
        assert!(parse_timecode(tc, ntsc).is_err(), "{tc}");
    }
    assert!(format_timecode(24 * 3600 * 100, FrameRate::new(24, 1)).is_err());
    let fast = FrameRate::new(120, 1);
    assert_eq!(format_timecode(119, fast).unwrap(), "00:00:00:119");
    assert_eq!(parse_timecode("00:00:00:119", fast).unwrap(), 119);
}

#[test]
fn rejects_invalid_numbers_ids_and_shapes() {
    let cases = [
        ("/version", json!(2)),
        ("/frame_rate", json!(23.976)),
        ("/frame_rate", json!({"num":24,"den":0})),
        ("/resolution", json!([0, 720])),
        ("/resolution", json!([1920, 32769])),
        ("/shots", json!({})),
        ("/shots/0/id", json!("")),
        ("/shots/0/id", json!(" private ")),
        ("/shots/1/id", json!("shot-001")),
        ("/shots/0/duration_frames", json!(0)),
        ("/shots/0/duration_frames", json!(-1)),
        ("/shots/0/duration_frames", json!(1.5)),
        ("/shots/0/duration_frames", json!(u64::MAX)),
        ("/shots/0/media/source_in_frame", json!(u64::MAX)),
        ("/shots/0/gap_before_frames", json!(u64::MAX)),
        ("/shots/0/media", json!({})),
        ("/shots/0/name", json!(false)),
    ];
    for (pointer, bad) in cases {
        let mut v = sample();
        v["shots"][0]["gap_before_frames"] = json!(0);
        *v.pointer_mut(pointer).unwrap() = bad;
        assert!(decode(&v).is_err(), "{pointer}");
    }
    let mut v = sample();
    v["shots"][0]
        .as_object_mut()
        .unwrap()
        .remove("duration_frames");
    assert!(decode(&v).is_err());
}

#[test]
fn rejects_control_noncharacters_separators_and_limits() {
    for ch in [
        '\0', '\n', '\r', '\t', '\u{7f}', '\u{85}', '\u{fffe}', '\u{ffff}', '\u{2028}', '\u{2029}',
    ] {
        for pointer in ["/title", "/shots/0/id", "/shots/0/name"] {
            let mut v = sample();
            *v.pointer_mut(pointer).unwrap() = json!(format!("invented{ch}text"));
            assert!(decode(&v).is_err());
        }
    }
    for (pointer, limit) in [
        ("/title", 4096),
        ("/shots/0/id", 256),
        ("/shots/0/name", 4096),
        ("/shots/0/media/url", 8192),
    ] {
        let mut v = sample();
        *v.pointer_mut(pointer).unwrap() = json!("x".repeat(limit + 1));
        assert!(decode(&v).is_err());
    }
    let mut list = ShotList::new(FrameRate::new(24, 1));
    list.shots = (0..MAX_SHOTS + 1)
        .map(|i| Shot::new(format!("shot-{i}"), 1))
        .collect();
    assert_eq!(list.normalized().unwrap_err().code, "record_limit");
    list.shots.truncate(300);
    for shot in &mut list.shots {
        shot.name = Some("x".repeat(4096));
    }
    assert_eq!(list.normalized().unwrap_err().code, "text_limit");
}

#[test]
fn uri_scope_is_explicit_without_io() {
    for url in [
        "file:///synthetic/clip%20a.mov",
        "file://synthetic.invalid/share/clip.mov",
        "https://example.invalid/video.mov",
        "HTTPS://example.invalid/video.mov?a=1&b=2",
    ] {
        let mut v = sample();
        v["shots"][0]["media"]["url"] = json!(url);
        assert_eq!(
            decode(&v).unwrap().shots[0].media.as_ref().unwrap().url,
            url
        );
    }
    for url in [
        "relative.mov",
        "C:\\synthetic\\clip.mov",
        "http://example.invalid/clip.mov",
        "ftp://example.invalid/a",
        "data:video/x",
        "file:/synthetic/a",
        "file:///",
        "file:///a?x=1",
        "https:example.invalid/a",
        "https://",
        "https://user:password@example.invalid/a",
        "https://example.invalid/a#fragment",
        "https://example.invalid/a b",
        "https://example.invalid/a%GG",
        "file:///é.mov",
    ] {
        let mut v = sample();
        v["shots"][0]["media"]["url"] = json!(url);
        assert!(decode(&v).is_err(), "{url}");
    }
}

#[test]
fn duplicate_keys_and_depth_and_utf8_are_bounded() {
    assert!(from_json(r#"{"version":1,"version":1}"#).is_err());
    let v = sample().to_string().replace(
        "\"id\":\"shot-001\"",
        "\"id\":\"shot-001\",\"id\":\"other\"",
    );
    assert!(from_json(&v).is_err());
    let mut v = sample();
    let mut extra = json!(0);
    for _ in 0..40 {
        extra = json!([extra]);
    }
    v["unused"] = extra;
    assert!(decode(&v).is_err());
    assert_eq!(
        from_json(&" ".repeat(MAX_JSON_BYTES + 1)).unwrap_err().code,
        "input_limit"
    );
    assert_eq!(
        read_input([0xff].as_slice(), 10).unwrap_err().code,
        "invalid_utf8"
    );
    assert_eq!(
        read_input("12345".as_bytes(), 4).unwrap_err().code,
        "input_limit"
    );
    assert!(read_input("".as_bytes(), MAX_TIMELINE_BYTES + 1).is_err());
    assert!(from_json(&format!("\u{feff}{}", sample())).is_ok());
}

#[test]
fn model_apis_use_checked_arithmetic() {
    let mut list = ShotList::new(FrameRate::new(24, 1));
    let mut shot = Shot::new("synthetic", u64::MAX);
    shot.gap_before_frames = Some(u64::MAX);
    list.shots.push(shot);
    assert!(list.duration_frames().is_err() && list.normalized().is_err());
    let mut list = ShotList::new(FrameRate::new(24, 1));
    let mut shot = Shot::new("synthetic", 1);
    shot.media = Some(Media {
        url: "file:///synthetic/a.mov".into(),
        source_in_frame: Some(u64::MAX),
    });
    list.shots.push(shot);
    assert!(list.normalized().is_err());
}

#[test]
fn errors_never_echo_input_or_paths() {
    let secret = "invented-private-label";
    let mut v = sample();
    v["shots"][0]["id"] = json!(secret);
    v["shots"][1]["id"] = json!(secret);
    let error = decode(&v).unwrap_err();
    assert!(!format!("{error:?} {error}").contains(secret));
    let error = from_json(&format!("{{\"{secret}\":")).unwrap_err();
    assert!(!format!("{error:?} {error}").contains(secret));
    let error = from_fcpxml("<secret-media&>").unwrap_err();
    assert!(!format!("{error:?} {error}").contains("secret-media"));
    use std::io::Error as IoError;
    let error = Error::from(IoError::other(secret));
    assert!(!format!("{error:?} {error}").contains(secret));
}

#[test]
fn accepted_large_document_serializes_within_neutral_reader_budget() {
    let mut list = ShotList::new(FrameRate::new(24, 1));
    list.shots = (0..MAX_SHOTS)
        .map(|i| {
            let mut s = Shot::new(format!("s{i}"), 1);
            s.name = Some("\"".repeat(80));
            s
        })
        .collect();
    let list = list.normalized().unwrap();
    let text = to_json(&list).unwrap();
    assert!(text.len() < MAX_JSON_BYTES);
    assert_eq!(from_json(&text).unwrap(), list);
}
