// SPDX-License-Identifier: MIT OR Apache-2.0

use edl_exchange::{
    FrameRate,
    TimecodeMode::{Drop, NonDrop},
    format_timecode, parse_timecode,
};

const DF: FrameRate = FrameRate::new(30_000, 1001);

#[test]
fn drop_frame_known_minute_ten_minute_hour_and_day_vectors() {
    let vectors = [
        (0, "00:00:00;00"),
        (1798, "00:00:59;28"),
        (1799, "00:00:59;29"),
        (1800, "00:01:00;02"),
        (3597, "00:01:59;29"),
        (3598, "00:02:00;02"),
        (17_981, "00:09:59;29"),
        (17_982, "00:10:00;00"),
        (107_892, "01:00:00;00"),
        (2_589_407, "23:59:59;29"),
    ];
    for (frame, tc) in vectors {
        assert_eq!(format_timecode(frame, DF, Drop).unwrap(), tc);
        assert_eq!(parse_timecode(tc, DF, Drop).unwrap(), frame);
        assert_eq!(
            parse_timecode(&tc.replace(';', ":"), DF, Drop).unwrap(),
            frame
        );
    }
    assert!(format_timecode(2_589_408, DF, Drop).is_err());
}

#[test]
fn every_frame_of_drop_frame_day_inverts_without_rollover() {
    for frame in 0..2_589_408 {
        let tc = format_timecode(frame, DF, Drop).unwrap();
        assert_eq!(parse_timecode(&tc, DF, Drop).unwrap(), frame);
    }
}

#[test]
fn dropped_and_invalid_labels_are_errors() {
    for tc in [
        "00:01:00;00",
        "00:01:00;01",
        "01:59:00;01",
        "24:00:00;00",
        "00:60:00;00",
        "00:00:60;00",
        "00:00:00;30",
        "0:00:00;00",
        "00:00:00.00",
        "00:00:00;0é",
        "00:00:00:000",
    ] {
        assert!(parse_timecode(tc, DF, Drop).is_err(), "{tc}");
    }
    assert!(parse_timecode("00:10:00;01", DF, Drop).is_ok());
    assert!(parse_timecode("00:00:00;00", DF, NonDrop).is_err());
    assert!(format_timecode(0, FrameRate::new(24, 1), Drop).is_err());
}

#[test]
fn non_drop_counts_nominal_fps_without_rounding_or_clock_correction() {
    for rate in [
        FrameRate::new(24_000, 1001),
        FrameRate::new(24, 1),
        FrameRate::new(25, 1),
        FrameRate::new(30, 1),
        DF,
    ] {
        let base = rate.timecode_base().unwrap();
        for frame in [
            0,
            1,
            base - 1,
            base,
            base * 60,
            base * 3600,
            base * 86_400 - 1,
        ] {
            let tc = format_timecode(frame, rate, NonDrop).unwrap();
            assert_eq!(parse_timecode(&tc, rate, NonDrop).unwrap(), frame);
        }
        assert_eq!(
            parse_timecode("01:00:00:00", rate, NonDrop).unwrap(),
            base * 3600
        );
        assert!(format_timecode(base * 86_400, rate, NonDrop).is_err());
    }
}
