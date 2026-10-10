// SPDX-License-Identifier: MIT OR Apache-2.0

use screenplay_length::{
    estimate, estimate_with, parse, to_json, Element, LayoutProfile, PageSize,
};

const SCRIPT: &str = include_str!("../samples/synthetic.fountain");
const LETTER: &str = include_str!("../samples/synthetic.letter.json");
const A4: &str = include_str!("../samples/synthetic.a4.json");

#[test]
fn letter_golden_matches() {
    let got = to_json(&estimate(SCRIPT, PageSize::Letter).unwrap());
    assert_eq!(got, LETTER);
}

#[test]
fn a4_golden_matches() {
    let got = to_json(&estimate(SCRIPT, PageSize::A4).unwrap());
    assert_eq!(got, A4);
}

#[test]
fn sample_covers_every_element_type() {
    let doc = parse(SCRIPT).unwrap();
    assert!(doc.has_title_page());
    assert_eq!(doc.title_page[0].key, "Title");
    assert_eq!(doc.title_page[4].value, "Nowhere Lane 0\nExample City");
    let has = |pred: fn(&Element) -> bool| doc.elements.iter().any(pred);
    assert!(has(
        |e| matches!(e, Element::SceneHeading { number: Some(n), .. } if n == "3")
    ));
    assert!(has(|e| matches!(e, Element::Action(_))));
    assert!(has(|e| matches!(e, Element::Centered(_))));
    assert!(has(|e| matches!(e, Element::Lyrics(_))));
    assert!(has(
        |e| matches!(e, Element::Transition(t) if t == "CUT TO:")
    ));
    assert!(has(
        |e| matches!(e, Element::Transition(t) if t == "FADE OUT.")
    ));
    assert!(has(|e| matches!(e, Element::PageBreak)));
    assert!(has(|e| matches!(e, Element::Dialogue(d) if d.dual)));
    assert!(has(
        |e| matches!(e, Element::Dialogue(d) if d.character == "OSKAR (V.O.)")
    ));
    // Notes, boneyard, sections and synopses leave no trace.
    let joined = format!("{:?}", doc.elements);
    assert!(!joined.contains("earlier draft"));
    assert!(!joined.contains("keep the numbers"));
    assert!(!joined.contains("Act One"));
    assert!(!joined.contains("clerk discovers"));
}

#[test]
fn sample_scene_structure() {
    let est = estimate(SCRIPT, PageSize::Letter).unwrap();
    assert_eq!(est.scenes.len(), 4);
    assert_eq!(est.scenes[0].heading, None);
    assert_eq!(est.scenes[1].number.as_deref(), Some("1"));
    assert_eq!(est.scenes[2].number.as_deref(), Some("2"));
    assert_eq!(est.scenes[3].number.as_deref(), Some("3"));
    assert_eq!(
        est.scenes[3].heading.as_deref(),
        Some("CELLAR UNDER THE FOURTH LANTERN - DAWN")
    );
    assert!(est.title_page);
    assert_eq!(est.scenes[0].start_page, 2);
    // The `===` before scene 3 pads scene 2 to the end of page 3.
    assert_eq!(est.scenes[2].end_page, 3);
    assert_eq!(est.scenes[3].start_page, 4);
    let sum: u32 = est.scenes.iter().map(|s| s.line_count).sum();
    assert_eq!(sum, est.total.lines);
    let eighths: u32 = est.scenes.iter().map(|s| s.eighths).sum();
    assert_eq!(eighths, est.total.eighths);
    assert!(est.scenes.iter().all(|s| s.eighths >= 1));
}

#[test]
fn a4_and_letter_differ_only_where_pages_end() {
    let l = estimate(SCRIPT, PageSize::Letter).unwrap();
    let a = estimate(SCRIPT, PageSize::A4).unwrap();
    assert_eq!(l.scenes.len(), a.scenes.len());
    // Scenes not touching a page end have identical line counts.
    assert_eq!(l.scenes[0].line_count, a.scenes[0].line_count);
    assert_eq!(l.scenes[1].line_count, a.scenes[1].line_count);
    assert_eq!(l.scenes[3].line_count, a.scenes[3].line_count);
    // The page-break-padded scene is longer on A4 (58 vs 55 lines/page).
    assert_eq!(a.scenes[2].line_count - l.scenes[2].line_count, 6);
    assert_ne!(l.total.eighths, a.total.eighths);
}

#[test]
fn custom_profile_is_echoed_in_output() {
    let profile =
        LayoutProfile::from_json(r#"{"page_size":"letter-54","lines_per_page":54}"#).unwrap();
    let est = estimate_with(SCRIPT, &profile).unwrap();
    assert_eq!(est.page_size, "letter-54");
    assert_eq!(est.layout.lines_per_page, 54);
    assert_eq!(est.layout.action_width, 60);
}

#[test]
fn repeated_runs_are_byte_identical() {
    let first = to_json(&estimate(SCRIPT, PageSize::A4).unwrap());
    for _ in 0..5 {
        assert_eq!(to_json(&estimate(SCRIPT, PageSize::A4).unwrap()), first);
    }
}

#[test]
fn crlf_input_gives_same_estimate() {
    let crlf = SCRIPT.replace('\n', "\r\n");
    assert_eq!(to_json(&estimate(&crlf, PageSize::Letter).unwrap()), LETTER);
}
