// SPDX-License-Identifier: MIT OR Apache-2.0

#[path = "../examples/support/mod.rs"]
mod support;

use pg_02_screenplay_importer::*;
use std::io::Cursor;
use support::{pdf, SamplePage, TextLine};

fn read(bytes: Vec<u8>) -> Screenplay {
    import_pdf(Cursor::new(bytes), &ImportOptions::default()).unwrap()
}

#[test]
fn user_unit_scales_page_and_line_coordinates_consistently() {
    let screenplay = read(pdf(&[SamplePage {
        lines: vec![TextLine::new("INT. SYNTHETIC ROOM - DAY", 72.0, 720.0)],
        user_unit: Some(2.0),
        ..SamplePage::default()
    }]));
    assert_eq!(screenplay.pages[0].user_unit, 2.0);
    assert_eq!(screenplay.pages[0].width, 1224.0);
    assert_eq!(screenplay.pages[0].height, 1584.0);
    assert!((screenplay.lines[0].baseline[0] - 144.0).abs() < 0.001);
    assert!((screenplay.lines[0].baseline[1] - 144.0).abs() < 0.001);
    assert!((screenplay.lines[0].font_size - 24.0).abs() < 0.001);
    assert_eq!(screenplay.lines[0].kind, ElementKind::SceneHeading);
}

#[test]
fn invalid_user_units_are_rejected() {
    for unit in [0.0, -1.0, 75_001.0] {
        assert!(matches!(
            import_pdf(
                Cursor::new(pdf(&[SamplePage {
                    user_unit: Some(unit),
                    ..SamplePage::default()
                }])),
                &ImportOptions::default()
            ),
            Err(Error::InvalidGeometry)
        ));
    }
}

#[test]
fn form_depth_cannot_exceed_the_backend_safe_bound() {
    let options = ImportOptions {
        max_form_depth: 33,
        ..ImportOptions::default()
    };
    assert!(matches!(
        import_pdf(Cursor::new(support::sample()), &options),
        Err(Error::InvalidOptions)
    ));
}

#[test]
fn oversized_single_glyph_mapping_is_charged_to_text_budget() {
    // The builder maps one glyph to one code point; a four-byte scalar still charges four bytes.
    let bytes = pdf(&[SamplePage {
        lines: vec![TextLine::new("🙂", 72.0, 720.0)],
        ..SamplePage::default()
    }]);
    let options = ImportOptions {
        max_text_bytes: 3,
        ..ImportOptions::default()
    };
    assert!(matches!(
        import_pdf(Cursor::new(bytes), &options),
        Err(Error::Limit("decoded text bytes"))
    ));
}

#[test]
fn controls_are_preserved_in_source_but_flagged_unknown() {
    let screenplay = read(pdf(&[SamplePage {
        lines: vec![TextLine::new("Synthetic\u{0001}text", 72.0, 720.0)],
        ..SamplePage::default()
    }]));
    assert!(screenplay.lines[0].raw_text.contains('\u{0001}'));
    assert_eq!(screenplay.lines[0].kind, ElementKind::Unknown);
    assert!(screenplay.doubts[0]
        .reasons
        .contains(&Reason::ControlCharacters));
}

#[test]
fn incomplete_heading_and_ambiguous_caps_have_explicit_reasons() {
    let screenplay = read(pdf(&[SamplePage {
        lines: vec![
            TextLine::new("ИНТ.", 72.0, 720.0),
            TextLine::new("SYNTHETIC UPPERCASE", 72.0, 680.0),
        ],
        ..SamplePage::default()
    }]));
    assert!(screenplay
        .doubts
        .iter()
        .any(|doubt| doubt.reasons.contains(&Reason::IncompleteSceneHeading)));
    assert!(screenplay
        .doubts
        .iter()
        .any(|doubt| doubt.reasons.contains(&Reason::AmbiguousUppercase)));
}

#[test]
fn transitions_reset_speaker_state_before_following_indented_text() {
    let screenplay = read(pdf(&[SamplePage {
        lines: vec![
            TextLine::new("IRIS", 252.0, 720.0),
            TextLine::new("A synthetic response.", 144.0, 702.0),
            TextLine::new("CUT TO:", 432.0, 684.0),
            TextLine::new("An indented synthetic line.", 144.0, 666.0),
        ],
        ..SamplePage::default()
    }]));
    assert_eq!(screenplay.lines[2].kind, ElementKind::Transition);
    assert!(screenplay.blocks[3].speaker.is_none());
    assert!(screenplay
        .doubts
        .iter()
        .any(|doubt| doubt.reasons.contains(&Reason::MissingSpeaker)));
}

#[test]
fn missing_unicode_mapping_is_not_silently_treated_as_empty_text() {
    let screenplay = read(pdf(&[SamplePage {
        lines: vec![TextLine::new("Unmapped", 72.0, 720.0)],
        omit_unicode: true,
        ..SamplePage::default()
    }]));
    assert_eq!(screenplay.lines[0].raw_text, "\u{fffd}".repeat(8));
    assert_eq!(screenplay.lines[0].kind, ElementKind::Unknown);
    assert!(screenplay.doubts[0]
        .reasons
        .contains(&Reason::UnicodeMappingMissing));
}

#[test]
fn degenerate_text_geometry_is_an_error_not_nonfinite_json() {
    let mut line = TextLine::new("Synthetic.", 72.0, 720.0);
    line.size = 0.0;
    assert!(matches!(
        import_pdf(
            Cursor::new(pdf(&[SamplePage {
                lines: vec![line],
                ..SamplePage::default()
            }])),
            &ImportOptions::default()
        ),
        Err(Error::InvalidGeometry)
    ));
}
