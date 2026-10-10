// SPDX-License-Identifier: MIT OR Apache-2.0

use subtitle_writer::{
    check, reading_speed, report_json, Checks, Cue, Document, Error, Metric, Rule, Violation,
};

fn doc(cues: Vec<Cue>) -> Document {
    let mut d = Document::new();
    d.cues = cues;
    d
}
fn rules(violations: &[Violation]) -> Vec<(&str, Rule)> {
    violations
        .iter()
        .map(|v| (v.cue.as_str(), v.rule))
        .collect()
}
fn run(cues: Vec<Cue>) -> Vec<Violation> {
    check(&doc(cues), &Checks::default()).unwrap()
}
fn text(chars: usize) -> String {
    "x".repeat(chars)
}

#[test]
fn clean_document_has_no_violations() {
    let v = run(vec![
        Cue::new("a", 0, 3000, &["Twenty characters ok", "and a second line"]),
        Cue::new("b", 3000, 5000, &["Touching cues are fine."]),
    ]);
    assert!(v.is_empty(), "{v:?}");
    assert_eq!(report_json(&v), "[]\n");
}

#[test]
fn reading_speed_exactly_at_limit_passes_and_one_character_more_fails() {
    // 17 cps * 2 s = 34 characters.
    let v = run(vec![Cue::new("a", 0, 2000, &[&text(34)])]);
    assert!(v.is_empty());
    let v = run(vec![Cue::new("a", 0, 2000, &[&text(35)])]);
    assert_eq!(rules(&v), vec![("a", Rule::ReadingSpeed)]);
    assert_eq!(v[0].value, Metric::Float(17.5));
    assert_eq!(v[0].limit, Metric::Int(17));
    assert_eq!(
        v[0].message,
        "35 characters in 2000 ms is 17.5 characters per second, maximum is 17"
    );
}

#[test]
fn reading_speed_counts_scalar_values_across_lines_not_bytes_or_newlines() {
    // 20 Cyrillic characters on each of two lines = 40 chars in 2 s = 20 cps.
    let line = "ш".repeat(20);
    let v = run(vec![Cue::new("a", 0, 2000, &[&line, &line])]);
    assert_eq!(rules(&v), vec![("a", Rule::ReadingSpeed)]);
    assert_eq!(v[0].value, Metric::Float(20.0));
    // 17 per line = 34 chars (the newline is not counted): exactly at the limit.
    let line = "ш".repeat(17);
    assert!(run(vec![Cue::new("a", 0, 2000, &[&line, &line])]).is_empty());
}

#[test]
fn reading_speed_is_not_computed_for_cues_without_positive_duration() {
    let v = run(vec![Cue::new("a", 1000, 1000, &[&text(40)])]);
    assert_eq!(rules(&v), vec![("a", Rule::EndNotAfterStart)]);
    assert_eq!(v[0].value, Metric::Int(0));
    let v = run(vec![Cue::new("a", 2000, 1000, &[&text(40)])]);
    assert_eq!(rules(&v), vec![("a", Rule::EndNotAfterStart)]);
    assert_eq!(v[0].value, Metric::Int(-1000));
}

#[test]
fn reading_speed_rounds_to_two_decimals_deterministically() {
    assert_eq!(reading_speed(1, 3000), 0.33);
    assert_eq!(reading_speed(2, 3000), 0.67);
    assert_eq!(reading_speed(35, 2000), 17.5);
    assert_eq!(reading_speed(56, 800), 70.0);
    assert_eq!(reading_speed(0, 1000), 0.0);
}

#[test]
fn line_length_at_limit_passes_and_reports_each_long_line() {
    assert!(run(vec![Cue::new("a", 0, 5000, &[&text(42), &text(42)])]).is_empty());
    let v = run(vec![Cue::new(
        "a",
        0,
        7000,
        &[&text(43), &text(10), &text(50)],
    )]);
    assert_eq!(
        rules(&v),
        vec![
            ("a", Rule::TooManyLines),
            ("a", Rule::LineTooLong),
            ("a", Rule::LineTooLong)
        ]
    );
    assert_eq!(v[1].message, "line 0 has 43 characters, maximum is 42");
    assert_eq!(v[2].message, "line 2 has 50 characters, maximum is 42");
    assert_eq!(v[2].value, Metric::Int(50));
}

#[test]
fn line_length_counts_scalar_values() {
    let line = "ё".repeat(42); // 84 bytes, 42 chars
    assert!(run(vec![Cue::new("a", 0, 5000, &[&line])]).is_empty());
    let line = "ё".repeat(43);
    assert_eq!(
        rules(&run(vec![Cue::new("a", 0, 5000, &[&line])])),
        vec![("a", Rule::LineTooLong)]
    );
}

#[test]
fn lines_per_cue_boundary() {
    assert!(run(vec![Cue::new("a", 0, 2000, &["x", "y"])]).is_empty());
    let v = run(vec![Cue::new("a", 0, 2000, &["x", "y", "z"])]);
    assert_eq!(rules(&v), vec![("a", Rule::TooManyLines)]);
    assert_eq!((v[0].value, v[0].limit), (Metric::Int(3), Metric::Int(2)));
}

#[test]
fn duration_boundaries() {
    assert!(run(vec![Cue::new("a", 0, 1000, &["x"])]).is_empty());
    assert!(run(vec![Cue::new("a", 0, 7000, &["x"])]).is_empty());
    let v = run(vec![Cue::new("a", 0, 999, &["x"])]);
    assert_eq!(rules(&v), vec![("a", Rule::TooShort)]);
    assert_eq!(
        (v[0].value, v[0].limit),
        (Metric::Int(999), Metric::Int(1000))
    );
    let v = run(vec![Cue::new("a", 0, 7001, &["x"])]);
    assert_eq!(rules(&v), vec![("a", Rule::TooLong)]);
    assert_eq!(
        (v[0].value, v[0].limit),
        (Metric::Int(7001), Metric::Int(7000))
    );
}

#[test]
fn overlap_and_ordering() {
    // Touching: ok.
    assert!(run(vec![
        Cue::new("a", 0, 2000, &["x"]),
        Cue::new("b", 2000, 4000, &["y"])
    ])
    .is_empty());
    // Overlap by 1 ms.
    let v = run(vec![
        Cue::new("a", 0, 2000, &["x"]),
        Cue::new("b", 1999, 4000, &["y"]),
    ]);
    assert_eq!(rules(&v), vec![("b", Rule::Overlap)]);
    assert_eq!(v[0].value, Metric::Int(1));
    assert_eq!(v[0].message, "overlaps the previous cue by 1 ms");
    // Starts before the previous start: not_sorted, not reported as overlap too.
    let v = run(vec![
        Cue::new("a", 3000, 5000, &["x"]),
        Cue::new("b", 1000, 2500, &["y"]),
    ]);
    assert_eq!(rules(&v), vec![("b", Rule::NotSorted)]);
    assert_eq!(
        (v[0].value, v[0].limit),
        (Metric::Int(1000), Metric::Int(3000))
    );
    // Same start as the previous cue: overlap, not not_sorted.
    let v = run(vec![
        Cue::new("a", 1000, 3000, &["x"]),
        Cue::new("b", 1000, 3000, &["y"]),
    ]);
    assert_eq!(rules(&v), vec![("b", Rule::Overlap)]);
    assert_eq!(v[0].value, Metric::Int(2000));
}

#[test]
fn duplicate_ids_are_reported_on_every_repeat() {
    let v = run(vec![
        Cue::new("a", 0, 1000, &["x"]),
        Cue::new("a", 1000, 2000, &["y"]),
        Cue::new("a", 2000, 3000, &["z"]),
    ]);
    assert_eq!(
        rules(&v),
        vec![("a", Rule::DuplicateId), ("a", Rule::DuplicateId)]
    );
    assert_eq!(v[0].message, "id used by 2 cues");
    assert_eq!(v[1].message, "id used by 3 cues");
    assert_eq!(v[1].value, Metric::Int(3));
}

#[test]
fn empty_and_whitespace_lines_are_reported_per_line() {
    let v = run(vec![Cue::new("a", 0, 2000, &["", " \t"])]);
    assert_eq!(
        rules(&v),
        vec![("a", Rule::EmptyLine), ("a", Rule::EmptyLine)]
    );
    assert_eq!(v[0].message, "line 0 is empty");
    assert_eq!(v[1].message, "line 1 is empty");
}

#[test]
fn violations_are_ordered_by_cue_then_rule_then_line() {
    let v = run(vec![
        Cue::new("a", 0, 2000, &["x"]),
        Cue::new("a", 1000, 1500, &[&text(43), "", &text(60), "y"]),
    ]);
    assert_eq!(
        rules(&v),
        vec![
            ("a", Rule::DuplicateId),
            ("a", Rule::EmptyLine),
            ("a", Rule::Overlap),
            ("a", Rule::TooShort),
            ("a", Rule::TooManyLines),
            ("a", Rule::LineTooLong),
            ("a", Rule::LineTooLong),
            ("a", Rule::ReadingSpeed),
        ]
    );
    let again = run(vec![
        Cue::new("a", 0, 2000, &["x"]),
        Cue::new("a", 1000, 1500, &[&text(43), "", &text(60), "y"]),
    ]);
    assert_eq!(report_json(&v), report_json(&again));
}

#[test]
fn custom_limits_are_honoured() {
    let checks = Checks {
        max_cps: 10,
        max_line_length: 5,
        max_lines: 1,
        min_duration_ms: 500,
        max_duration_ms: 1000,
    };
    let d = doc(vec![
        Cue::new("a", 0, 500, &["12345"]), // 10 cps, 5 chars, 500 ms: all at limit
        Cue::new("b", 500, 1000, &["123456"]), // 12 cps, 6 chars
        Cue::new("c", 1000, 2001, &["1", "2"]), // 1001 ms, 2 lines
        Cue::new("d", 2001, 2500, &["1"]), // 499 ms
    ]);
    let v = check(&d, &checks).unwrap();
    assert_eq!(
        rules(&v),
        vec![
            ("b", Rule::LineTooLong),
            ("b", Rule::ReadingSpeed),
            ("c", Rule::TooLong),
            ("c", Rule::TooManyLines),
            ("d", Rule::TooShort),
        ]
    );
    assert_eq!(v[1].value, Metric::Float(12.0));
}

#[test]
fn invalid_check_configuration_is_rejected() {
    let base = Checks::default();
    for (checks, reason) in [
        (Checks { max_cps: 0, ..base }, "max_cps"),
        (
            Checks {
                max_line_length: 0,
                ..base
            },
            "max_line_length",
        ),
        (
            Checks {
                max_lines: 0,
                ..base
            },
            "max_lines",
        ),
        (
            Checks {
                min_duration_ms: 0,
                ..base
            },
            "min_duration_ms",
        ),
        (
            Checks {
                min_duration_ms: 2000,
                max_duration_ms: 1999,
                ..base
            },
            "max_duration_ms",
        ),
    ] {
        match check(&Document::new(), &checks) {
            Err(Error::Config(message)) => assert!(message.starts_with(reason), "{message}"),
            other => panic!("expected Config error for {reason}, got {other:?}"),
        }
    }
    assert!(check(
        &Document::new(),
        &Checks {
            min_duration_ms: 500,
            max_duration_ms: 500,
            ..base
        }
    )
    .is_ok());
}

#[test]
fn report_json_shape() {
    let v = run(vec![Cue::new("a", 0, 500, &["x"])]);
    let json: serde_json::Value = serde_json::from_str(&report_json(&v)).unwrap();
    assert_eq!(
        json,
        serde_json::json!([{
            "cue": "a",
            "rule": "too_short",
            "message": "duration 500 ms is below the minimum of 1000 ms",
            "value": 500,
            "limit": 1000
        }])
    );
}
