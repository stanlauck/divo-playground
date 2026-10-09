// SPDX-License-Identifier: MIT OR Apache-2.0

use edl_exchange::{FrameRate, TimecodeMode, from_edl, parse_timecode};

const RATE: FrameRate = FrameRate::new(24, 1);
const CUT: &str = "001 AX V C 00:00:01:00 00:00:02:00 01:00:00:00 01:00:01:00\n";

#[test]
fn external_requires_exact_rate_even_when_empty_or_fcm_is_present() {
    for text in ["", "TITLE: Empty\nFCM: NON-DROP FRAME\n", CUT] {
        assert_eq!(from_edl(text, None).unwrap_err().code, "missing_frame_rate");
    }
    assert!(from_edl("", Some(RATE)).unwrap().shots.is_empty());
    assert!(from_edl(CUT, Some(FrameRate::new(60, 1))).is_err());
}

#[test]
fn external_source_record_gaps_reels_comments_and_auto_ids() {
    let text = format!(
        "TITLE: Invented external\nFCM: NON-DROP FRAME\n{CUT}\
        * FROM CLIP NAME: Angle A\n\
        * FROM FILE: file:///synthetic/a.mov\n\
        * SHOT ID: RESERVED\n\
        003 CAMB V C 00:00:00:00 00:00:01:00 01:00:01:12 01:00:02:12\n\
        * FROM CLIP NAME: Angle B\n\
        * harmless note\n"
    );
    let list = from_edl(&text, Some(RATE)).unwrap();
    assert_eq!(list.record_start.as_deref(), Some("01:00:00:00"));
    assert_eq!(list.shots[0].id, "RESERVED");
    assert_eq!(list.shots[0].name.as_deref(), Some("Angle A"));
    let m = list.shots[0].media.as_ref().unwrap();
    assert_eq!(m.url.as_deref(), Some("file:///synthetic/a.mov"));
    assert_eq!(m.reel, None);
    assert_eq!(m.source_in_frame, Some(24));
    assert_eq!(list.shots[1].id, "Angle B");
    assert_eq!(list.shots[1].name, None);
    assert_eq!(list.shots[1].gap_before_frames, Some(12));
}

#[test]
fn black_gap_and_id_bearing_black_placeholder_are_not_conflated() {
    let text = "001 BL V C 00:00:00:00 00:00:01:00 00:00:00:00 00:00:01:00\n\
        002 CAM V C 00:00:00:00 00:00:01:00 00:00:01:00 00:00:02:00\n\
        003 BLACK V C 00:00:00:00 00:00:01:00 00:00:02:00 00:00:03:00\n\
        * SHOT ID: PLACEHOLDER\n";
    let list = from_edl(text, Some(RATE)).unwrap();
    assert_eq!(list.shots.len(), 2);
    assert_eq!(list.shots[0].gap_before_frames, Some(24));
    assert!(list.shots[0].media.is_some());
    assert!(list.shots[1].media.is_none());
    assert_eq!(list.shots[1].id, "PLACEHOLDER");
    let trailing = "001 BL V C 00:00:00:00 00:00:01:00 00:00:00:00 00:00:01:00\n";
    assert!(from_edl(trailing, Some(RATE)).is_err());
}

#[test]
fn external_ids_reserve_later_supplied_values_and_assign_monotonically() {
    let text = "001 AX V C 00:00:00:00 00:00:00:01 00:00:00:00 00:00:00:01\n\
        002 AX V C 00:00:00:00 00:00:00:01 00:00:00:01 00:00:00:02\n\
        * SHOT ID: %73%68%6F%74-1\n\
        003 AX V C 00:00:00:00 00:00:00:01 00:00:00:02 00:00:00:03\n\
        * FROM CLIP NAME: shot-1\n";
    let list = from_edl(text, Some(RATE)).unwrap();
    assert_eq!(
        list.shots.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        ["shot-2", "shot-1", "shot-3"]
    );
}

#[test]
fn external_drop_fcm_parses_colons_and_rejects_dropped_labels() {
    let text = "FCM: DROP FRAME\n001 CAM V C 00:00:59:29 00:01:00:02 00:09:59:29 00:10:00:00\n";
    let df = FrameRate::new(30000, 1001);
    let list = from_edl(text, Some(df)).unwrap();
    assert_eq!(list.timecode_mode, TimecodeMode::Drop);
    assert_eq!(list.shots[0].duration_frames, 1);
    assert_eq!(
        list.shots[0].media.as_ref().unwrap().source_in_frame,
        Some(1799)
    );
    assert_eq!(list.record_start.as_deref(), Some("00:09:59;29"));
    assert!(from_edl(&text.replace("00:01:00:02", "00:01:00:00"), Some(df)).is_err());
    assert!(from_edl(text, Some(RATE)).is_err());
}

#[test]
fn unsupported_audio_transitions_effects_and_generators_are_not_silently_dropped() {
    for text in [
        CUT.replace(" V C ", " A C "),
        CUT.replace(" V C ", " AA/V C "),
        CUT.replace(" V C ", " V D 012 "),
        CUT.replace(" V C ", " V W001 012 "),
        CUT.replace(" V C ", " V K "),
        CUT.replace(" AX ", " BARS "),
        format!("{CUT}M2 AX 24.0 00:00:01:00\n"),
        format!("{CUT}* FREEZE FRAME\n"),
        format!("{CUT}* MOTION EFFECT: 2\n"),
        format!("{CUT}* SPEED: 0.5\n"),
        format!("{CUT}* ASC_SOP: (1 1 1)(0 0 0)(1 1 1)\n"),
        format!("{CUT}* ASC_SAT: 0.2\n"),
        format!("{CUT}* OTIO REFERENCE smptebars: true\n"),
        format!("SPLIT AUDIO DELAY 1\n{CUT}"),
    ] {
        assert!(from_edl(&text, Some(RATE)).is_err(), "{text}");
    }
}

#[test]
fn overlaps_retimes_repeated_events_midnight_and_midstream_fcm_are_errors() {
    for text in [
        CUT.replace("00:00:02:00", "00:00:03:00"),
        CUT.replace("01:00:01:00", "00:00:00:00"),
        CUT.replace("00:00:02:00", "24:00:00:00"),
        CUT.replace("001", "000"),
        CUT.replace("001", "1000"),
        format!("{CUT}{CUT}"),
        format!("{CUT}FCM: NON-DROP FRAME\n"),
        format!("{CUT}002 AX V C 00:00:00:00 00:00:00:01 01:00:00:00 01:00:00:01\n"),
    ] {
        assert!(from_edl(&text, Some(RATE)).is_err());
    }
}

#[test]
fn import_bom_crlf_decorative_comments_and_optional_fcm() {
    let list = from_edl(
        &format!("\u{feff}* invented header\r\n{CUT}* a note\r\n"),
        Some(RATE),
    )
    .unwrap();
    assert_eq!(list.shots[0].duration_frames, 24);
    assert_eq!(
        parse_timecode(
            list.record_start.as_ref().unwrap(),
            RATE,
            TimecodeMode::NonDrop
        )
        .unwrap(),
        86400
    );
    for bad in [
        "* SHOT ID: \n",
        "* SHOT ID: BAD%QQ\n",
        "* SHOT ID: %FF\n",
        "* PG16 UNKNOWN: 1\n",
    ] {
        assert!(
            from_edl(&format!("{CUT}{bad}"), Some(RATE)).is_err(),
            "{bad}"
        );
    }
    assert!(from_edl(&format!("{CUT}* SHOT ID: X\n* SHOT ID: Y\n"), Some(RATE)).is_err());
}
