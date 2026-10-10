// SPDX-License-Identifier: MIT OR Apache-2.0

use subtitle_writer::{
    parse, read_input, Cue, Error, MAX_CUES, MAX_DEPTH, MAX_INPUT_BYTES, MAX_LINES_PER_CUE,
    MAX_LINE_CHARS, MAX_NAME_CHARS, MAX_TIME_MS,
};

fn cue_json(id: &str, lines: &str, start: u64, end: u64) -> String {
    format!(r#"{{"id":"{id}","lines":[{lines}],"start_ms":{start},"end_ms":{end}}}"#)
}
fn wrap(cues: &str) -> String {
    format!(r#"{{"version":1,"cues":[{cues}]}}"#)
}

#[test]
fn parses_minimal_and_full_cues() {
    let d = parse(&wrap(&cue_json("a", r#""x""#, 0, 1000))).unwrap();
    assert_eq!(d.cues, vec![Cue::new("a", 0, 1000, &["x"])]);
    assert_eq!(d.title, None);
    let d = parse(
        r#"{"version":1,"title":"T","language":"en-GB","cues":[{"id":"a","speaker":"S","lines":["x","y"],"start_ms":5,"end_ms":1000,"extra":true}],"unknown":{}}"#,
    )
    .unwrap();
    assert_eq!(d.title.as_deref(), Some("T"));
    assert_eq!(d.language.as_deref(), Some("en-GB"));
    assert_eq!(d.cues[0].speaker.as_deref(), Some("S"));
    assert_eq!(d.cues[0].lines, vec!["x", "y"]);
}

#[test]
fn null_speaker_equals_absent_speaker() {
    let a = parse(&wrap(
        r#"{"id":"a","speaker":null,"lines":["x"],"start_ms":0,"end_ms":1}"#,
    ))
    .unwrap();
    let b = parse(&wrap(&cue_json("a", r#""x""#, 0, 1))).unwrap();
    assert_eq!(a, b);
}

#[test]
fn bom_is_skipped() {
    let text = format!("\u{feff}{}", wrap(&cue_json("a", r#""x""#, 0, 1)));
    assert!(parse(&text).is_ok());
}

#[test]
fn malformed_json_reports_position_only() {
    for text in [
        "",
        "{",
        "[]",
        "null",
        r#"{"version":1,"cues":[{"id":"a"}]}"#,
        r#"{"version":1,"cues":[{"id":"a","lines":"x","start_ms":0,"end_ms":1}]}"#,
        r#"{"version":1,"cues":[{"id":1,"lines":["x"],"start_ms":0,"end_ms":1}]}"#,
        r#"{"version":1,"cues":[{"id":"a","lines":["x"],"start_ms":-1,"end_ms":1}]}"#,
        r#"{"version":1,"cues":[{"id":"a","lines":["x"],"start_ms":1.5,"end_ms":2}]}"#,
        r#"{"version":1,"cues":[{"id":"a","lines":["x"],"start_ms":"0","end_ms":2}]}"#,
        r#"{"version":1,"cues":[{"id":"a","lines":[null],"start_ms":0,"end_ms":2}]}"#,
        r#"{"version":1,"cues":[{"id":"a","speaker":5,"lines":["x"],"start_ms":0,"end_ms":2}]}"#,
        r#"{"version":1,"cues":[{"id":"a","lines":["x"],"start_ms":0,"end_ms":2}]} trailing"#,
        r#"{"version":1,"cues":[{"id":"a","lines":["x"],"start_ms":0,"end_ms":99999999999999999999}]}"#,
        r#"{"version":1}"#,
        r#"{"cues":[]}"#,
    ] {
        let error = parse(text).unwrap_err();
        assert!(
            matches!(error, Error::InvalidJson { .. }),
            "{text}: {error:?}"
        );
        assert!(!error.to_string().contains("trailing"), "no input echo");
    }
}

#[test]
fn version_must_be_one() {
    let error = parse(r#"{"version":2,"cues":[]}"#).unwrap_err();
    assert!(matches!(error, Error::Invalid { path, .. } if path == "version"));
    assert!(parse(r#"{"version":1,"cues":[]}"#).unwrap().cues.is_empty());
}

#[test]
fn empty_id_and_empty_speaker_are_rejected() {
    let error = parse(&wrap(&cue_json("", r#""x""#, 0, 1))).unwrap_err();
    assert!(matches!(error, Error::Invalid { path, .. } if path == "cues[0].id"));
    let error = parse(&wrap(
        r#"{"id":"a","speaker":"  ","lines":["x"],"start_ms":0,"end_ms":1}"#,
    ))
    .unwrap_err();
    assert!(matches!(error, Error::Invalid { path, .. } if path == "cues[0].speaker"));
}

#[test]
fn lines_must_not_be_empty_array_and_are_bounded() {
    let error = parse(&wrap(&cue_json("a", "", 0, 1))).unwrap_err();
    assert!(matches!(error, Error::Invalid { path, .. } if path == "cues[0].lines"));
    let ok = vec![r#""x""#; MAX_LINES_PER_CUE].join(",");
    assert!(parse(&wrap(&cue_json("a", &ok, 0, 1))).is_ok());
    let too_many = vec![r#""x""#; MAX_LINES_PER_CUE + 1].join(",");
    let error = parse(&wrap(&cue_json("a", &too_many, 0, 1))).unwrap_err();
    assert_eq!(
        error,
        Error::Limit {
            path: "cues[0].lines".into(),
            value: MAX_LINES_PER_CUE + 1,
            limit: MAX_LINES_PER_CUE
        }
    );
}

#[test]
fn line_and_name_lengths_are_bounded_in_characters() {
    let ok = format!("\"{}\"", "й".repeat(MAX_LINE_CHARS));
    assert!(parse(&wrap(&cue_json("a", &ok, 0, 1))).is_ok());
    let long = format!("\"{}\"", "й".repeat(MAX_LINE_CHARS + 1));
    let error = parse(&wrap(&cue_json("a", &long, 0, 1))).unwrap_err();
    assert_eq!(
        error,
        Error::Limit {
            path: "cues[0].lines[0]".into(),
            value: MAX_LINE_CHARS + 1,
            limit: MAX_LINE_CHARS
        }
    );
    let id = "i".repeat(MAX_NAME_CHARS + 1);
    let error = parse(&wrap(&cue_json(&id, r#""x""#, 0, 1))).unwrap_err();
    assert!(matches!(error, Error::Limit { path, .. } if path == "cues[0].id"));
    let json = format!(
        r#"{{"version":1,"cues":[{{"id":"a","speaker":"{}","lines":["x"],"start_ms":0,"end_ms":1}}]}}"#,
        "s".repeat(MAX_NAME_CHARS + 1)
    );
    let error = parse(&json).unwrap_err();
    assert!(matches!(error, Error::Limit { path, .. } if path == "cues[0].speaker"));
}

#[test]
fn control_characters_separators_and_timing_arrows_are_rejected_in_text() {
    for bad in [
        r#""a\nb""#,
        r#""a\tb""#,
        r#""a\u0007b""#,
        r#""a b""#,
        r#""a --> b""#,
    ] {
        let error = parse(&wrap(&cue_json("a", bad, 0, 1))).unwrap_err();
        assert!(
            matches!(error, Error::Invalid { path, .. } if path == "cues[0].lines[0]"),
            "{bad}"
        );
    }
    let error = parse(&wrap(&cue_json("a\\nb", r#""x""#, 0, 1))).unwrap_err();
    assert!(matches!(error, Error::Invalid { path, .. } if path == "cues[0].id"));
    let error = parse(r#"{"version":1,"title":"t\u0000","cues":[]}"#).unwrap_err();
    assert!(matches!(error, Error::Invalid { path, .. } if path == "title"));
}

#[test]
fn language_tag_is_restricted_to_ascii_letters_digits_and_hyphens() {
    for bad in ["", "en us", "en\"", "ру"] {
        let json = format!(
            r#"{{"version":1,"language":{},"cues":[]}}"#,
            serde_json::json!(bad)
        );
        let error = parse(&json).unwrap_err();
        assert!(
            matches!(error, Error::Invalid { path, .. } if path == "language"),
            "{bad}"
        );
    }
    assert!(parse(r#"{"version":1,"language":"zh-Hant-TW","cues":[]}"#).is_ok());
}

#[test]
fn times_are_capped_below_one_hundred_hours() {
    assert!(parse(&wrap(&cue_json("a", r#""x""#, 0, MAX_TIME_MS))).is_ok());
    let error = parse(&wrap(&cue_json("a", r#""x""#, 0, MAX_TIME_MS + 1))).unwrap_err();
    assert!(matches!(error, Error::Limit { path, .. } if path == "cues[0].end_ms"));
    let error = parse(&wrap(&cue_json("a", r#""x""#, MAX_TIME_MS + 1, 0))).unwrap_err();
    assert!(matches!(error, Error::Limit { path, .. } if path == "cues[0].start_ms"));
}

#[test]
fn unsorted_overlapping_and_inverted_cues_parse_without_error() {
    let d = parse(&wrap(&format!(
        "{},{},{}",
        cue_json("b", r#""x""#, 5000, 6000),
        cue_json("a", r#""x""#, 0, 7000),
        cue_json("a", r#""x""#, 3000, 2000),
    )))
    .unwrap();
    assert_eq!(d.cues.len(), 3);
}

#[test]
fn cue_count_is_bounded() {
    let cues: Vec<String> = (0..MAX_CUES + 1)
        .map(|i| cue_json(&i.to_string(), r#""x""#, 0, 1))
        .collect();
    let error = parse(&wrap(&cues.join(","))).unwrap_err();
    assert_eq!(
        error,
        Error::Limit {
            path: "cues".into(),
            value: MAX_CUES + 1,
            limit: MAX_CUES
        }
    );
}

#[test]
fn input_size_is_bounded_before_parsing() {
    let text = " ".repeat(MAX_INPUT_BYTES + 1);
    assert!(matches!(parse(&text), Err(Error::InputTooLarge { .. })));
}

#[test]
fn read_input_enforces_limit_and_utf8() {
    assert_eq!(read_input(&b"abc"[..], 3).unwrap(), "abc");
    assert!(matches!(
        read_input(&b"abcd"[..], 3),
        Err(Error::InputTooLarge { bytes: 4, limit: 3 })
    ));
    assert!(matches!(
        read_input(&[0xff, 0xfe][..], 10),
        Err(Error::InvalidUtf8)
    ));
    // Limit is capped at MAX_INPUT_BYTES, which still accepts small inputs.
    assert!(read_input(&b"{}"[..], usize::MAX).is_ok());
}

#[test]
fn nesting_depth_is_bounded_even_in_ignored_fields() {
    let nested = format!(
        r#"{{"version":1,"cues":[],"x":{}{}}}"#,
        "[".repeat(100_000),
        "]".repeat(100_000)
    );
    assert_eq!(
        parse(&nested).unwrap_err(),
        Error::Limit {
            path: "input".into(),
            value: 100_001,
            limit: MAX_DEPTH
        }
    );
    // Brackets inside strings do not count.
    let shallow = format!(
        r#"{{"version":1,"title":"{}","cues":[],"x":{}{}}}"#,
        "[".repeat(100),
        "[".repeat(MAX_DEPTH - 1),
        "]".repeat(MAX_DEPTH - 1)
    );
    assert!(parse(&shallow).is_ok());
    let escaped = r#"{"version":1,"title":"a\"[[[[","cues":[]}"#;
    assert!(parse(escaped).is_ok());
}

#[test]
fn errors_display_without_input_text() {
    let error = parse("{\"version\":1,\"cues\":[{\"id\":\"SECRET\"").unwrap_err();
    let text = error.to_string();
    assert!(text.starts_with("invalid JSON at line "), "{text}");
    assert!(!text.contains("SECRET"));
}
