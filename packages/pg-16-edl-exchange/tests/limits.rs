// SPDX-License-Identifier: MIT OR Apache-2.0

use edl_exchange::{
    FrameRate, MAX_EDL_BYTES, MAX_EVENTS, MAX_JSON_BYTES, Media, Shot, ShotList, from_edl,
    from_json, read_input, to_edl, to_json,
};

fn list() -> ShotList {
    let mut list = ShotList::new(FrameRate::new(24, 1));
    list.shots.push(Shot::new("ONE", 1));
    list
}
fn pct_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 3);
    for b in bytes {
        write!(out, "%{b:02X}").unwrap();
    }
    out
}
fn mutate_metadata(edl: &str, prefix: &str, change: impl Fn(&mut serde_json::Value)) -> String {
    edl.lines()
        .map(|line| {
            let Some(text) = line.strip_prefix(prefix) else {
                return line.to_owned();
            };
            let mut bytes = Vec::new();
            let mut input = text.bytes();
            while let Some(b) = input.next() {
                if b == b'%' {
                    let hex = [input.next().unwrap(), input.next().unwrap()];
                    bytes.push(u8::from_str_radix(std::str::from_utf8(&hex).unwrap(), 16).unwrap());
                } else {
                    bytes.push(b);
                }
            }
            let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            change(&mut value);
            let json = serde_json::to_vec(&value).unwrap();
            format!("{prefix}{}", pct_encode(&json))
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

#[test]
fn io_json_depth_size_and_edl_line_budgets_are_bounded() {
    assert_eq!(
        read_input(&b"12345"[..], 4).unwrap_err().code,
        "input_limit"
    );
    assert!(read_input(&[255][..], MAX_JSON_BYTES).is_err());
    assert!(read_input(&b""[..], usize::MAX).is_err());
    assert_eq!(
        from_json(&" ".repeat(MAX_JSON_BYTES + 1)).unwrap_err().code,
        "input_limit"
    );
    assert_eq!(
        from_edl(&" ".repeat(MAX_EDL_BYTES + 1), None)
            .unwrap_err()
            .code,
        "input_limit"
    );
    let nested = format!("{}0{}", "[".repeat(34), "]".repeat(34));
    assert!(
        from_json(&format!(
            r#"{{"version":1,"frame_rate":24,"shots":[],"unknown":{nested}}}"#
        ))
        .is_err()
    );
    assert!(
        from_edl(
            &format!("* {}\n", "x".repeat(128 * 1024)),
            Some(FrameRate::new(24, 1))
        )
        .is_err()
    );
    assert!(from_edl(&"\n".repeat(20_001), Some(FrameRate::new(24, 1))).is_err());
}

#[test]
fn single_line_text_and_per_field_limits_are_checked_for_every_string() {
    for bad in [
        "bad\nline",
        "bad\tline",
        "bad\u{2028}line",
        "bad\u{2029}line",
        "bad\u{fffe}",
    ] {
        let mut l = list();
        l.title = Some(bad.into());
        assert!(l.normalized().is_err());
        l.title = None;
        l.shots[0].name = Some(bad.into());
        assert!(l.normalized().is_err());
        l.shots[0].name = None;
        l.shots[0].media = Some(Media {
            reel: Some(bad.into()),
            ..Media::default()
        });
        assert!(l.normalized().is_err());
    }
    let mut l = list();
    l.shots[0].id = "a".repeat(257);
    assert!(l.normalized().is_err());
    l = list();
    l.shots[0].name = Some("a".repeat(4097));
    assert!(l.normalized().is_err());
    l = list();
    l.shots[0].media = Some(Media {
        reel: Some("a".repeat(4097)),
        ..Media::default()
    });
    assert!(l.normalized().is_err());
}

#[test]
fn maximum_event_count_includes_explicit_gaps() {
    let mut l = ShotList::new(FrameRate::new(24, 1));
    for n in 0..MAX_EVENTS {
        l.shots.push(Shot::new(format!("id-{n}"), 1));
    }
    let edl = to_edl(&l).unwrap();
    assert_eq!(from_edl(&edl, None).unwrap(), l);
    l.shots[0].gap_before_frames = Some(1);
    assert_eq!(l.normalized().unwrap_err().code, "record_limit");
    l.shots[0].gap_before_frames = None;
    l.shots.push(Shot::new("overflow", 1));
    assert_eq!(l.normalized().unwrap_err().code, "record_limit");
}

#[test]
fn redundant_display_name_does_not_consume_canonical_budget_twice() {
    let mut l = ShotList::new(FrameRate::new(24, 1));
    for n in 0..250 {
        let id = format!("{n:03}{}", "i".repeat(253));
        l.shots.push(Shot {
            name: Some(id.clone()),
            media: Some(Media {
                reel: Some(format!("{n:03}{}", "r".repeat(3841))),
                ..Media::default()
            }),
            ..Shot::new(id, 1)
        });
    }
    l.title = Some("t".repeat(4096));
    // 250*(256+3844)+4096 = 1,029,096 canonical UTF-8 bytes.
    let normalized = l.normalized().unwrap();
    assert!(normalized.shots.iter().all(|s| s.name.is_none()));
    let edl = to_edl(&l).unwrap();
    assert_eq!(from_edl(&edl, None).unwrap(), normalized);
    assert_eq!(from_json(&to_json(&l).unwrap()).unwrap(), normalized);
    l.shots[0].name = Some("n".repeat(4096));
    for s in &mut l.shots[1..] {
        s.name = Some("n".repeat(4096));
    }
    assert_eq!(l.normalized().unwrap_err().code, "text_limit");
}

#[test]
fn metadata_rejects_wrong_version_count_rate_mode_reel_id_and_source() {
    let edl = to_edl(&list()).unwrap();
    for (key, value) in [
        ("version", serde_json::json!(2)),
        ("event_count", serde_json::json!(2)),
        ("frame_rate", serde_json::json!(60)),
        ("timecode_mode", serde_json::json!("drop")),
        ("record_start", serde_json::json!("01:00:00:00")),
        ("surprise", serde_json::json!(true)),
    ] {
        let changed = mutate_metadata(&edl, "* PG16 TIMELINE: ", |v| {
            v[key] = value.clone();
        });
        assert!(from_edl(&changed, None).is_err(), "{key}");
    }
    // EDL has no standard FPS header: a supported changed rate can only be
    // detected by a caller-supplied exact rate, not by one-frame cut labels.
    let different_rate = mutate_metadata(&edl, "* PG16 TIMELINE: ", |v| {
        v["frame_rate"] = serde_json::json!(25);
    });
    assert_eq!(
        from_edl(&different_rate, Some(FrameRate::new(24, 1)))
            .unwrap_err()
            .code,
        "rate_mismatch"
    );
    for (key, value) in [
        ("version", serde_json::json!(2)),
        ("id", serde_json::json!("OTHER")),
        (
            "media",
            serde_json::json!({"reel":"CAM","source_in_frame":24}),
        ),
        ("surprise", serde_json::json!(true)),
    ] {
        let changed = mutate_metadata(&edl, "* PG16 SHOT: ", |v| {
            v[key] = value.clone();
        });
        assert!(from_edl(&changed, None).is_err(), "{key}");
    }
    for changed in [
        edl.replace("TITLE: UNTITLED", "TITLE: OTHER"),
        edl.replace("001  BL", "002  BL"),
        edl.replace("001  BL", "001  AX"),
        edl.replace("* SHOT ID: ONE", "* SHOT ID: OTHER"),
        edl.lines()
            .filter(|s| !s.starts_with("* PG16 SHOT:"))
            .collect::<Vec<_>>()
            .join("\n"),
        edl.lines()
            .filter(|s| !s.starts_with("* PG16 TIMELINE:"))
            .collect::<Vec<_>>()
            .join("\n"),
    ] {
        assert!(from_edl(&changed, Some(FrameRate::new(24, 1))).is_err());
    }
}

#[test]
fn duplicate_metadata_and_injection_strings_are_never_accepted() {
    let edl = to_edl(&list()).unwrap();
    let header = edl
        .lines()
        .find(|s| s.starts_with("* PG16 TIMELINE:"))
        .unwrap();
    assert!(from_edl(&edl.replace(header, &format!("{header}\n{header}")), None).is_err());
    let shot = edl.lines().find(|s| s.starts_with("* PG16 SHOT:")).unwrap();
    assert!(from_edl(&edl.replace(shot, &format!("{shot}\n{shot}")), None).is_err());
    let duplicate = pct_encode(br#"{"version":1,"version":1}"#);
    assert!(
        from_edl(
            &edl.replace(shot, &format!("* PG16 SHOT: {duplicate}")),
            None
        )
        .is_err()
    );
    for name in [
        "private\n* M2 AX 0",
        "private\u{2028}* FREEZE FRAME",
        "\tPRIVATE",
    ] {
        let changed = mutate_metadata(&edl, "* PG16 SHOT: ", |v| {
            v["name"] = serde_json::json!(name);
        });
        let e = from_edl(&changed, None).unwrap_err();
        assert!(!format!("{e:?} {e}").contains("private"));
    }
}

#[test]
fn external_oversized_names_are_rejected_before_id_assignment() {
    let text = format!(
        "001 AX V C 00:00:00:00 00:00:00:01 00:00:00:00 00:00:00:01\n* FROM CLIP NAME: {}\n",
        "a".repeat(4097)
    );
    assert_eq!(
        from_edl(&text, Some(FrameRate::new(24, 1)))
            .unwrap_err()
            .code,
        "text_limit"
    );
}
