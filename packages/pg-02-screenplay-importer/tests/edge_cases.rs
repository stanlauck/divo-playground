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

#[test]
fn incomplete_combined_markers_do_not_replace_the_active_scene() {
    for invalid in [
        "INT./EXTERIOR notes",
        "INT. / EXTERIOR notes",
        "ИНТ./НАТУРА заметки",
    ] {
        let screenplay = read(pdf(&[SamplePage {
            lines: vec![
                TextLine::new("INT. SYNTHETIC ROOM - DAY", 72.0, 720.0),
                TextLine::new(invalid, 72.0, 684.0),
                TextLine::new("A synthetic action.", 72.0, 648.0),
            ],
            ..SamplePage::default()
        }]));
        assert_ne!(screenplay.lines[1].kind, ElementKind::SceneHeading);
        assert_eq!(screenplay.blocks[2].scene.as_deref(), Some("block-1"));
    }
    for valid in [
        "INT./EXT. SAMPLE - DAY",
        "INT. / EXT. SAMPLE - DAY",
        "ИНТ. / НАТ. ПРИМЕР - ДЕНЬ",
    ] {
        let screenplay = read(pdf(&[SamplePage {
            lines: vec![TextLine::new(valid, 72.0, 720.0)],
            ..SamplePage::default()
        }]));
        assert_eq!(screenplay.lines[0].kind, ElementKind::SceneHeading);
    }
}

#[test]
fn uppercase_action_at_left_margin_never_becomes_a_speaker() {
    let screenplay = read(pdf(&[SamplePage {
        lines: vec![
            TextLine::new("IRIS MOVES", 72.0, 720.0),
            TextLine::new("Indented synthetic prose.", 144.0, 702.0),
        ],
        ..SamplePage::default()
    }]));
    assert_eq!(screenplay.lines[0].kind, ElementKind::Unknown);
    assert!(screenplay
        .blocks
        .iter()
        .all(|block| block.speaker.is_none()));
    assert!(screenplay
        .doubts
        .iter()
        .any(|doubt| doubt.reasons.contains(&Reason::AmbiguousUppercase)));
}

#[test]
fn whitespace_rows_are_not_spoken_or_merged_into_dialogue() {
    let screenplay = read(pdf(&[SamplePage {
        lines: vec![
            TextLine::new("IRIS", 252.0, 720.0),
            TextLine::new("   ", 144.0, 702.0),
            TextLine::new("A synthetic response.", 144.0, 684.0),
        ],
        ..SamplePage::default()
    }]));
    assert_eq!(screenplay.lines[1].raw_text, "   ");
    assert_eq!(screenplay.lines[1].kind, ElementKind::Unknown);
    assert_eq!(screenplay.blocks[1].lines, ["line-2"]);
    assert!(screenplay.blocks[1].speaker.is_none());
    assert!(screenplay.blocks[2].speaker.is_none());
}

#[test]
fn whitespace_only_text_layer_is_not_misreported_as_an_unsupported_scan() {
    let screenplay = read(pdf(&[SamplePage {
        lines: vec![TextLine::new("   ", 144.0, 720.0)],
        ..SamplePage::default()
    }]));
    assert_eq!(screenplay.pages[0].status, PageStatus::Text);
    assert_eq!(screenplay.lines[0].kind, ElementKind::Unknown);
    assert!(!screenplay
        .warnings
        .iter()
        .any(|warning| warning.kind == WarningKind::NoExtractableText));
}

#[test]
fn marginal_furniture_between_cue_and_text_clears_speaker_state() {
    let screenplay = read(pdf(&[
        SamplePage {
            lines: vec![
                TextLine::new("IRIS", 252.0, 782.0),
                TextLine::new("SYNTHETIC DRAFT", 200.0, 764.0),
                TextLine::new("A synthetic response.", 144.0, 746.0),
            ],
            ..SamplePage::default()
        },
        SamplePage {
            lines: vec![TextLine::new("SYNTHETIC DRAFT", 200.0, 764.0)],
            ..SamplePage::default()
        },
    ]));
    assert!(screenplay.doubts.iter().any(
        |doubt| doubt.line == "line-2" && doubt.reasons.contains(&Reason::PossibleHeaderFooter)
    ));
    assert!(screenplay.blocks[2].speaker.is_none());
    assert!(screenplay
        .doubts
        .iter()
        .any(|doubt| doubt.line == "line-3" && doubt.reasons.contains(&Reason::MissingSpeaker)));
}

#[test]
fn repeated_marginal_scene_headings_keep_their_scene_roles() {
    let screenplay = read(pdf(&[
        SamplePage {
            lines: vec![TextLine::new("INT. SYNTHETIC ROOM - DAY", 72.0, 772.0)],
            ..SamplePage::default()
        },
        SamplePage {
            lines: vec![TextLine::new("INT. SYNTHETIC ROOM - DAY", 72.0, 772.0)],
            ..SamplePage::default()
        },
    ]));
    assert!(screenplay
        .lines
        .iter()
        .all(|line| line.kind == ElementKind::SceneHeading));
    assert!(!screenplay
        .doubts
        .iter()
        .any(|doubt| doubt.reasons.contains(&Reason::PossibleHeaderFooter)));
}

#[test]
fn malformed_image_resources_still_report_presence_and_image_loss() {
    let objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /XObject << /Bad 5 0 R >> >> /Contents 4 0 R >>".to_vec(),
        support::stream(b"/Bad Do"),
        b"<< /Type /XObject /Subtype /Image /Height 1 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length 0 >>\nstream\n\nendstream".to_vec(),
    ];
    let screenplay = read(support::serialize(&objects));
    assert_eq!(screenplay.pages[0].images, 1);
    assert_eq!(
        screenplay.pages[0].status,
        PageStatus::UnsupportedNoTextLayer
    );
    assert!(screenplay
        .warnings
        .iter()
        .any(|warning| warning.kind == WarningKind::ImageContentNotImported));
}

#[test]
fn spaced_russian_combined_heading_replaces_the_previous_active_scene() {
    let screenplay = read(pdf(&[SamplePage {
        lines: vec![
            TextLine::new("ИНТ. УЧЕБНАЯ КОМНАТА - ДЕНЬ", 72.0, 720.0),
            TextLine::new("Первое учебное действие.", 72.0, 690.0),
            TextLine::new("ИНТ. / НАТ. УЧЕБНЫЙ ДВОР - НОЧЬ", 72.0, 654.0),
            TextLine::new("Второе учебное действие.", 72.0, 624.0),
        ],
        ..SamplePage::default()
    }]));
    assert_eq!(screenplay.lines[2].kind, ElementKind::SceneHeading);
    assert_eq!(screenplay.blocks[1].scene.as_deref(), Some("block-1"));
    assert_eq!(screenplay.blocks[2].kind, ElementKind::SceneHeading);
    assert_eq!(screenplay.blocks[2].scene, None);
    assert_eq!(screenplay.blocks[3].scene.as_deref(), Some("block-3"));
    assert!(screenplay.doubts.is_empty());
}
