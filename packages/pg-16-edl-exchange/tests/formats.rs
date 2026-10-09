// SPDX-License-Identifier: MIT OR Apache-2.0

use edl_exchange::{FrameRate, Media, Shot, ShotList, TimecodeMode, from_edl, from_json, to_edl};

fn rows(edl: &str) -> Vec<Vec<&str>> {
    edl.lines()
        .filter(|s| s.starts_with(|c: char| c.is_ascii_digit()))
        .map(|s| s.split_whitespace().collect())
        .collect()
}

#[test]
fn normalized_roundtrip_for_all_supported_rates_and_both_29_97_modes() {
    for rate in [
        FrameRate::new(24000, 1001),
        FrameRate::new(24, 1),
        FrameRate::new(25, 1),
        FrameRate::new(30, 1),
        FrameRate::new(30000, 1001),
    ] {
        for mode in [TimecodeMode::NonDrop, TimecodeMode::Drop] {
            if mode == TimecodeMode::Drop && rate != FrameRate::new(30000, 1001) {
                continue;
            }
            let mut list = ShotList::new(rate);
            list.title = Some("Invented / 测试".into());
            list.timecode_mode = mode;
            list.resolution = Some([2048, 1152]);
            list.record_start = Some(
                if mode == TimecodeMode::Drop {
                    "01:00:00;00"
                } else {
                    "01:00:00:00"
                }
                .into(),
            );
            list.shots = vec![
                Shot {
                    media: Some(Media {
                        url: Some("file:///synthetic/a.mov".into()),
                        reel: Some("a long reel".into()),
                        source_in_frame: Some(1700),
                    }),
                    ..Shot::new("α", 48)
                },
                Shot {
                    gap_before_frames: Some(12),
                    name: Some("Invented gap".into()),
                    ..Shot::new("β", 36)
                },
                Shot {
                    media: Some(Media::default()),
                    ..Shot::new("unresolved", 24)
                },
            ];
            let edl = to_edl(&list).unwrap();
            assert!(edl.is_ascii());
            assert_eq!(from_edl(&edl, None).unwrap(), list.normalized().unwrap());
            assert_eq!(
                from_edl(&edl, Some(rate)).unwrap(),
                list.normalized().unwrap()
            );
            assert_eq!(rows(&edl).len(), 4);
        }
    }
}

#[test]
fn empty_export_preserves_nonzero_start_rate_and_resolution() {
    let mut list = ShotList::new(FrameRate::new(30000, 1001));
    list.timecode_mode = TimecodeMode::Drop;
    list.record_start = Some("01:00:00:00".into());
    list.resolution = Some([1280, 720]);
    let edl = to_edl(&list).unwrap();
    assert!(rows(&edl).is_empty());
    assert_eq!(from_edl(&edl, None).unwrap(), list.normalized().unwrap());
    assert_eq!(
        from_edl(&edl, Some(FrameRate::new(25, 1)))
            .unwrap_err()
            .code,
        "rate_mismatch"
    );
}

#[test]
fn alias_reserves_later_short_names_reuses_original_and_escapes_reserved_reels() {
    let mut list = ShotList::new(FrameRate::new(24, 1));
    for (i, reel) in [
        "long_reel_name",
        "R0000001",
        "BL",
        "BLACK",
        "BARS",
        "long_reel_name",
        "AX",
        "cam",
    ]
    .into_iter()
    .enumerate()
    {
        list.shots.push(Shot {
            media: Some(Media {
                reel: Some(reel.into()),
                ..Media::default()
            }),
            ..Shot::new(format!("id-{i}"), 1)
        });
    }
    let edl = to_edl(&list).unwrap();
    let reels: Vec<_> = rows(&edl).iter().map(|r| r[1]).collect();
    assert_eq!(
        reels,
        [
            "R0000002", "R0000001", "R0000003", "R0000004", "R0000005", "R0000002", "AX",
            "R0000006"
        ]
    );
    assert_eq!(to_edl(&list).unwrap(), edl);
    assert_eq!(from_edl(&edl, None).unwrap(), list.normalized().unwrap());
}

#[test]
fn comments_preserve_unicode_quotes_percent_and_backslashes_without_injection() {
    let mut list = ShotList::new(FrameRate::new(24, 1));
    list.title = Some("Invented \"quoted\" % \\ 测试".into());
    list.shots.push(Shot {
        name: Some("A \"shot\" with % and \\ λ".into()),
        media: Some(Media {
            reel: Some("bizarre % \" \\ reel".into()),
            ..Media::default()
        }),
        ..Shot::new("id:%/λ", 24)
    });
    let edl = to_edl(&list).unwrap();
    assert!(edl.is_ascii());
    assert!(
        edl.lines().all(|s| s
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b" /:+->%=.()*".contains(&b)))
    );
    assert_eq!(from_edl(&edl, None).unwrap(), list);
    assert_eq!(from_edl(&edl.replace('\n', "\r\n"), None).unwrap(), list);
}

#[test]
fn drop_frame_event_labels_use_colons_with_global_fcm_not_semicolons() {
    let mut list = ShotList::new(FrameRate::new(30000, 1001));
    list.timecode_mode = TimecodeMode::Drop;
    list.record_start = Some("00:00:59;29".into());
    list.shots.push(Shot::new("one", 1));
    let edl = to_edl(&list).unwrap();
    assert!(edl.contains("FCM: DROP FRAME"));
    assert_eq!(&rows(&edl)[0][6..], ["00:00:59:29", "00:01:00:02"]);
    assert_eq!(from_edl(&edl, None).unwrap(), list);
}

#[test]
fn worst_case_per_field_escaping_remains_within_metadata_line_budget() {
    let mut list = ShotList::new(FrameRate::new(24, 1));
    list.shots.push(Shot {
        id: "\\".repeat(256),
        name: Some("\"".repeat(4096)),
        media: Some(Media {
            reel: Some("\\".repeat(4096)),
            url: Some(format!("https://example.invalid/{}", "'".repeat(8192 - 24))),
            source_in_frame: Some(12),
        }),
        ..Shot::new("unused", 24)
    });
    // Keep the URL exactly within its byte budget.
    let url = list.shots[0].media.as_mut().unwrap().url.as_mut().unwrap();
    url.truncate(8192);
    let edl = to_edl(&list).unwrap();
    assert!(edl.lines().any(|l| l.len() > 64 * 1024));
    assert_eq!(from_edl(&edl, None).unwrap(), list);
}

#[test]
fn synthetic_golden_files_are_current_and_roundtrip() {
    for name in ["synthetic", "drop-frame"] {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("samples");
        let list = from_json(&std::fs::read_to_string(root.join(format!("{name}.json"))).unwrap())
            .unwrap();
        let edl = std::fs::read_to_string(root.join(format!("{name}.edl"))).unwrap();
        assert_eq!(to_edl(&list).unwrap(), edl);
        assert_eq!(from_edl(&edl, None).unwrap(), list);
    }
}
