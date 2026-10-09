// SPDX-License-Identifier: MIT OR Apache-2.0

use edl_exchange::{FrameRate, Media, Shot, ShotList, from_edl, from_json, to_edl, to_json};

#[test]
fn neutral_defaults_unknowns_and_rational_rate_normalize() {
    let list = from_json(
        r#"{
        "version":1,"title":"","frame_rate":{"num":48000,"den":2002},
        "record_start":"00:00:00:00","resolution":[1920,1080],"extension":{"opaque":1},
        "shots":[{"id":"one","name":"one","duration_frames":24,"gap_before_frames":0,
          "media":{"reel":"AX","url":null,"source_in_frame":0,"unknown":"ignored"}}]
    }"#,
    )
    .unwrap();
    assert_eq!(list.frame_rate, FrameRate::new(24000, 1001));
    assert_eq!(
        to_json(&list).unwrap(),
        r#"{"version":1,"frame_rate":{"num":24000,"den":1001},"shots":[{"id":"one","duration_frames":24,"media":{}}]}"#
    );
    assert_eq!(from_edl(&to_edl(&list).unwrap(), None).unwrap(), list);
}

#[test]
fn model_distinguishes_placeholder_from_reel_only_and_unresolved_auxiliary_media() {
    let mut list = ShotList::new(FrameRate::new(24, 1));
    list.shots = vec![
        Shot::new("missing", 24),
        Shot {
            media: Some(Media::default()),
            ..Shot::new("auxiliary", 24)
        },
        Shot {
            media: Some(Media {
                reel: Some("CAMERA_A".into()),
                ..Media::default()
            }),
            ..Shot::new("reel", 24)
        },
    ];
    let edl = to_edl(&list).unwrap();
    let reels: Vec<_> = edl
        .lines()
        .filter(|l| l.starts_with(|c: char| c.is_ascii_digit()))
        .map(|l| l.split_whitespace().nth(1).unwrap())
        .collect();
    assert_eq!(reels, ["BL", "AX", "R0000001"]);
    assert_eq!(from_edl(&edl, None).unwrap(), list);
}

#[test]
fn invalid_neutral_values_are_rejected_without_echoing_contents() {
    let invalid = [
        r#"{"version":2,"frame_rate":24,"shots":[]}"#,
        r#"{"version":1,"frame_rate":23.976,"shots":[]}"#,
        r#"{"version":1,"frame_rate":60,"shots":[]}"#,
        r#"{"version":1,"frame_rate":{"num":24,"den":0},"shots":[]}"#,
        r#"{"version":1,"frame_rate":24,"timecode_mode":"drop","shots":[]}"#,
        r#"{"version":1,"frame_rate":24,"shots":[{"id":" PRIVATE ","duration_frames":1}]}"#,
        r#"{"version":1,"frame_rate":24,"shots":[{"id":"PRIVATE","duration_frames":0}]}"#,
        r#"{"version":1,"frame_rate":24,"shots":[{"id":"PRIVATE","duration_frames":1.5}]}"#,
        r#"{"version":1,"frame_rate":24,"resolution":[0,1080],"shots":[]}"#,
        r#"{"version":1,"frame_rate":24,"shots":[{"id":"PRIVATE","duration_frames":1},{"id":"PRIVATE","duration_frames":1}]}"#,
    ];
    for input in invalid {
        let error = from_json(input).unwrap_err();
        assert!(!format!("{error:?} {error}").contains("PRIVATE"));
    }
}

#[test]
fn duplicate_keys_are_rejected_including_ignored_extensions() {
    for input in [
        r#"{"version":1,"version":1,"frame_rate":24,"shots":[]}"#,
        r#"{"version":1,"frame_rate":24,"shots":[],"extra":{"a":1,"a":2}}"#,
        r#"{"version":1,"frame_rate":24,"shots":[{"id":"a","duration_frames":1,"extra":{"a":1,"a":2}}]}"#,
    ] {
        assert_eq!(from_json(input).unwrap_err().code, "invalid_json");
    }
}

#[test]
fn source_and_record_out_cannot_cross_midnight_even_for_one_frame() {
    let mut list = ShotList::new(FrameRate::new(24, 1));
    list.record_start = Some("23:59:59:23".into());
    list.shots.push(Shot::new("one", 1));
    assert!(list.normalized().is_err());
    list.record_start = None;
    list.shots[0].media = Some(Media {
        source_in_frame: Some(2_073_599),
        ..Media::default()
    });
    assert!(list.normalized().is_err());
    list.shots[0].media.as_mut().unwrap().source_in_frame = Some(u64::MAX);
    assert!(list.normalized().is_err());
    list.shots[0].gap_before_frames = Some(u64::MAX);
    assert!(list.duration_frames().is_err());
}

#[test]
fn media_uri_validation_rejects_repaired_and_non_reference_urls() {
    for url in [
        "https:example.test/a",
        "https:///example.test/a",
        "https://",
        "https://user:secret@example.test/a",
        "http://example.test/a",
        "file:///",
        "file://relative",
        "file:///clip.mov?secret",
        "https://example.test/a#fragment",
        "https://example.test/%XY",
        "https://example.test/a b",
        "https://example.test/a\\b",
        "https://example.test/é",
        "https://example.test/<tag>",
        "https://example.test/a[1]",
    ] {
        let mut list = ShotList::new(FrameRate::new(24, 1));
        list.shots.push(Shot {
            media: Some(Media {
                url: Some(url.into()),
                ..Media::default()
            }),
            ..Shot::new("one", 1)
        });
        let error = list.normalized().unwrap_err();
        assert!(!format!("{error:?} {error}").contains(url));
    }
    for url in [
        "file:///synthetic/clip.mov",
        "https://example.invalid/a%20b.mov?version=1",
    ] {
        let mut list = ShotList::new(FrameRate::new(24, 1));
        list.shots.push(Shot {
            media: Some(Media {
                url: Some(url.into()),
                ..Media::default()
            }),
            ..Shot::new("one", 1)
        });
        assert_eq!(from_edl(&to_edl(&list).unwrap(), None).unwrap(), list);
    }
}
