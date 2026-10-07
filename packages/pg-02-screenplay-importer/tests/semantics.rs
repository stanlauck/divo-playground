// SPDX-License-Identifier: MIT OR Apache-2.0

#[path = "../examples/support/mod.rs"]
mod support;

use pg_02_screenplay_importer::*;
use std::io::Cursor;
use support::{pdf, SamplePage, TextLine};

fn read(bytes: Vec<u8>) -> Screenplay {
    import_pdf(Cursor::new(bytes), &ImportOptions::default()).unwrap()
}
fn single(lines: Vec<TextLine>) -> Screenplay {
    read(pdf(&[SamplePage {
        lines,
        ..SamplePage::default()
    }]))
}

#[test]
fn wrapped_dialogue_and_action_form_blocks_without_losing_source_lines() {
    let screenplay = single(vec![
        TextLine::new("INT. SAMPLE ROOM - DAY", 72.0, 720.0),
        TextLine::new("A blue sample moves.", 72.0, 690.0),
        TextLine::new("A green sample stays.", 72.0, 672.0),
        TextLine::new("IRIS", 252.0, 636.0),
        TextLine::new("One synthetic sentence", 144.0, 618.0),
        TextLine::new("continues on this line.", 144.0, 600.0),
    ]);
    assert_eq!(screenplay.blocks[1].kind, ElementKind::Action);
    assert_eq!(screenplay.blocks[1].lines, ["line-2", "line-3"]);
    assert_eq!(
        screenplay.blocks[3].text,
        "One synthetic sentence\ncontinues on this line."
    );
    assert_eq!(screenplay.blocks[3].lines, ["line-5", "line-6"]);
    assert_eq!(screenplay.blocks[3].speaker.as_deref(), Some("block-3"));
}

#[test]
fn consecutive_speaker_cues_with_uppercase_dialogue_keep_distinct_speakers() {
    let screenplay = single(vec![
        TextLine::new("INT. SAMPLE ROOM - DAY", 72.0, 720.0),
        TextLine::new("IRIS", 252.0, 684.0),
        TextLine::new("SYNTHETIC RESPONSE!", 144.0, 666.0),
        TextLine::new("RAY", 252.0, 648.0),
        TextLine::new("Another synthetic response.", 144.0, 630.0),
    ]);
    assert_eq!(screenplay.lines[1].kind, ElementKind::Character);
    assert_eq!(screenplay.lines[2].kind, ElementKind::Dialogue);
    assert_eq!(screenplay.lines[3].kind, ElementKind::Character);
    assert_eq!(screenplay.lines[4].kind, ElementKind::Dialogue);
    assert_ne!(screenplay.blocks[2].speaker, screenplay.blocks[4].speaker);
}

#[test]
fn detached_dialogue_and_parentheticals_are_never_silently_linked_to_a_speaker() {
    let screenplay = single(vec![
        TextLine::new("An orphan synthetic response.", 144.0, 720.0),
        TextLine::new("(with no speaker)", 180.0, 684.0),
    ]);
    assert!(screenplay
        .blocks
        .iter()
        .all(|block| block.speaker.is_none()));
    assert!(screenplay
        .doubts
        .iter()
        .any(|doubt| doubt.reasons.contains(&Reason::MissingSpeaker)));
    assert!(screenplay
        .doubts
        .iter()
        .any(|doubt| doubt.reasons.contains(&Reason::UnexpectedParenthetical)));
}

#[test]
fn multiline_parenthetical_keeps_its_character_reference() {
    let screenplay = single(vec![
        TextLine::new("INT. SAMPLE ROOM - DAY", 72.0, 720.0),
        TextLine::new("IRIS", 252.0, 684.0),
        TextLine::new("(in a synthetic", 180.0, 666.0),
        TextLine::new("quiet manner)", 180.0, 648.0),
        TextLine::new("A synthetic response.", 144.0, 630.0),
    ]);
    assert_eq!(screenplay.blocks[2].kind, ElementKind::Parenthetical);
    assert_eq!(screenplay.blocks[2].lines.len(), 2);
    assert_eq!(screenplay.blocks[2].speaker.as_deref(), Some("block-2"));
    assert_eq!(screenplay.blocks[3].kind, ElementKind::Dialogue);
}

#[test]
fn page_boundaries_do_not_invent_dialogue_continuation() {
    let screenplay = read(pdf(&[
        SamplePage {
            lines: vec![
                TextLine::new("IRIS", 252.0, 100.0),
                TextLine::new("Synthetic response.", 144.0, 82.0),
            ],
            ..SamplePage::default()
        },
        SamplePage {
            lines: vec![TextLine::new("Possible continuation.", 144.0, 720.0)],
            ..SamplePage::default()
        },
    ]));
    assert!(screenplay.blocks.last().unwrap().speaker.is_none());
    assert!(screenplay
        .doubts
        .last()
        .unwrap()
        .reasons
        .contains(&Reason::DialogueAcrossPage));
}

#[test]
fn repeated_headers_are_retained_as_unknown_not_dropped_or_character_cues() {
    let screenplay = read(pdf(&[
        SamplePage {
            lines: vec![
                TextLine::new("SYNTHETIC HEADER", 200.0, 772.0),
                TextLine::new("INT. SAMPLE - DAY", 72.0, 720.0),
            ],
            ..SamplePage::default()
        },
        SamplePage {
            lines: vec![
                TextLine::new("SYNTHETIC HEADER", 200.0, 772.0),
                TextLine::new("A new synthetic action.", 72.0, 720.0),
            ],
            ..SamplePage::default()
        },
    ]));
    assert_eq!(screenplay.lines.len(), 4);
    assert_eq!(screenplay.lines[0].kind, ElementKind::Unknown);
    assert_eq!(screenplay.lines[2].kind, ElementKind::Unknown);
    assert_eq!(
        screenplay
            .doubts
            .iter()
            .filter(|doubt| doubt.reasons.contains(&Reason::PossibleHeaderFooter))
            .count(),
        2
    );
}

#[test]
fn scene_marker_boundaries_do_not_misread_an_action_prefix() {
    let screenplay = single(vec![
        TextLine::new("INT.PRODUCTION is only a synthetic label.", 72.0, 720.0),
        TextLine::new("НАТ.ПРИМЕР — это не заголовок сцены.", 72.0, 684.0),
        TextLine::new("12А ИНТ./НАТ. УЧЕБНЫЙ ДВОР - ДЕНЬ", 72.0, 648.0),
        TextLine::new("INT/EXT. SYNTHETIC ROOM - NIGHT", 72.0, 612.0),
    ]);
    assert_eq!(screenplay.lines[0].kind, ElementKind::Action);
    assert_eq!(screenplay.lines[1].kind, ElementKind::Action);
    assert_eq!(screenplay.lines[2].kind, ElementKind::SceneHeading);
    assert_eq!(screenplay.lines[3].kind, ElementKind::SceneHeading);
}

#[test]
fn title_case_and_lonely_character_candidates_are_marked_uncertain() {
    let screenplay = single(vec![
        TextLine::new("Iris", 252.0, 720.0),
        TextLine::new("A synthetic response.", 144.0, 702.0),
        TextLine::new("LONELY CUE", 252.0, 650.0),
    ]);
    assert_eq!(screenplay.lines[0].kind, ElementKind::Character);
    assert_eq!(screenplay.lines[2].kind, ElementKind::Character);
    assert!(
        screenplay
            .doubts
            .iter()
            .filter(|doubt| doubt.reasons.contains(&Reason::UncertainCharacterCue))
            .count()
            >= 2
    );
}

#[test]
fn text_layer_on_an_image_page_is_imported_but_image_content_is_reported() {
    let screenplay = read(pdf(&[SamplePage {
        lines: vec![TextLine::new("A synthetic text layer.", 72.0, 720.0)],
        image: true,
        ..SamplePage::default()
    }]));
    assert_eq!(screenplay.pages[0].status, PageStatus::Text);
    assert_eq!(screenplay.pages[0].images, 1);
    assert_eq!(screenplay.lines[0].text, "A synthetic text layer.");
    assert!(screenplay
        .warnings
        .iter()
        .any(|warning| warning.kind == WarningKind::ImageContentNotImported));
}

#[test]
fn empty_content_streams_are_blank_not_unsupported_scans() {
    let screenplay = read(pdf(&[SamplePage::default()]));
    assert_eq!(screenplay.pages[0].status, PageStatus::Blank);
    assert!(!screenplay
        .warnings
        .iter()
        .any(|warning| warning.kind == WarningKind::NoExtractableText));
}

#[test]
fn fill_and_stroke_does_not_double_text_and_invisible_text_is_retained() {
    let mut fill_stroke = TextLine::new("Synthetic outline.", 72.0, 720.0);
    fill_stroke.render_mode = 2;
    let mut invisible = TextLine::new("Invisible synthetic layer.", 72.0, 684.0);
    invisible.render_mode = 3;
    let screenplay = single(vec![fill_stroke, invisible]);
    assert_eq!(screenplay.lines[0].text, "Synthetic outline.");
    assert_eq!(screenplay.lines[1].text, "Invisible synthetic layer.");
}

#[test]
fn whitespace_and_inferred_word_spaces_have_separate_provenance() {
    let screenplay = single(vec![
        TextLine::new("  Synthetic text.  ", 72.0, 720.0),
        TextLine::new("Blue", 72.0, 684.0),
        TextLine::new("cube", 115.2, 684.0),
    ]);
    assert_eq!(screenplay.lines[0].raw_text, "  Synthetic text.  ");
    assert_eq!(screenplay.blocks[0].text, "Synthetic text.");
    assert_eq!(screenplay.lines[1].raw_text, "Bluecube");
    assert_eq!(screenplay.lines[1].text, "Blue cube");
    assert_eq!(screenplay.lines[1].inferred_spaces, 1);
}

#[test]
fn unicode_mappings_are_not_ascii_normalized() {
    let screenplay = single(vec![TextLine::new(
        "Учебный текст 南 עברית e\u{301} 🙂",
        72.0,
        720.0,
    )]);
    assert_eq!(
        screenplay.lines[0].text,
        "Учебный текст 南 עברית e\u{301} 🙂"
    );
}

#[test]
fn rotations_retain_dimensions_and_flag_nonhorizontal_layout() {
    let screenplay = read(pdf(&[SamplePage {
        lines: vec![TextLine::new("Synthetic.", 72.0, 720.0)],
        rotation: 90,
        ..SamplePage::default()
    }]));
    assert_eq!(screenplay.pages[0].rotation, 90);
    assert_eq!(screenplay.pages[0].width, 792.0);
    assert_eq!(screenplay.pages[0].height, 612.0);
    assert!(screenplay
        .doubts
        .iter()
        .any(|doubt| doubt.reasons.contains(&Reason::UnsupportedTextOrientation)));
}

#[test]
fn nested_form_text_and_inherited_fonts_are_extracted() {
    let screenplay = read(support::form_pdf(3, 1, false));
    assert_eq!(screenplay.lines[0].text, "Synthetic form text.");
    assert!((screenplay.lines[0].baseline[0] - 72.0).abs() < 0.001);
    assert!((screenplay.lines[0].baseline[1] - 92.0).abs() < 0.001);
}

#[test]
fn form_cycles_depth_and_expansion_are_bounded_before_interpretation() {
    assert!(matches!(
        import_pdf(
            Cursor::new(support::form_pdf(2, 1, true)),
            &ImportOptions::default()
        ),
        Err(Error::InvalidPdf)
    ));
    let options = ImportOptions {
        max_form_depth: 1,
        ..ImportOptions::default()
    };
    assert!(matches!(
        import_pdf(Cursor::new(support::form_pdf(3, 1, false)), &options),
        Err(Error::Limit("Form depth"))
    ));
    let options = ImportOptions {
        max_operations: 100,
        ..ImportOptions::default()
    };
    assert!(matches!(
        import_pdf(Cursor::new(support::form_pdf(10, 2, false)), &options),
        Err(Error::Limit("expanded content operations"))
    ));
}

#[test]
fn missing_fonts_and_unknown_operators_are_reported_not_silently_ignored() {
    let screenplay = read(pdf(&[SamplePage {
        extra_content:
            "BT /Missing 12 Tf 1 0 0 1 72 720 Tm (Synthetic.) Tj ET\nsynthetic_unsupported\n".into(),
        ..SamplePage::default()
    }]));
    assert!(screenplay
        .warnings
        .iter()
        .any(|warning| warning.kind == WarningKind::BackendInvalidResource));
    assert!(screenplay
        .warnings
        .iter()
        .any(|warning| warning.kind == WarningKind::BackendUnknownOperator));
    // The backend can supply a fallback font. Its guessed text must remain uncertain.
    assert!(screenplay
        .lines
        .iter()
        .all(|line| line.kind == ElementKind::Unknown));
    assert!(screenplay
        .doubts
        .iter()
        .any(|doubt| doubt.reasons.contains(&Reason::BackendUncertainText)));
}

#[test]
fn every_source_line_has_exactly_one_block_and_doubts_reference_existing_objects() {
    let screenplay = read(support::sample());
    for line in &screenplay.lines {
        assert_eq!(
            screenplay
                .blocks
                .iter()
                .filter(|block| block.lines.contains(&line.id))
                .count(),
            1
        );
        assert!(screenplay.pages[line.page - 1].lines.contains(&line.id));
    }
    for doubt in &screenplay.doubts {
        assert!(screenplay.lines.iter().any(|line| line.id == doubt.line));
        assert!(screenplay
            .blocks
            .iter()
            .any(|block| block.id == doubt.block && block.lines.contains(&doubt.line)));
    }
}

#[test]
fn output_round_trip_and_generated_ids_are_deterministic() {
    let bytes = support::sample();
    let first = read(bytes.clone());
    let second = read(bytes);
    assert_eq!(first, second);
    let json = serde_json::to_vec(&first).unwrap();
    assert_eq!(serde_json::from_slice::<Screenplay>(&json).unwrap(), first);
}

#[test]
fn input_exact_byte_limit_is_inclusive_and_reader_failures_propagate() {
    use std::io::{self, Read};
    let bytes = support::sample();
    let options = ImportOptions {
        max_input_bytes: bytes.len(),
        ..ImportOptions::default()
    };
    assert!(import_pdf(Cursor::new(&bytes), &options).is_ok());
    let options = ImportOptions {
        max_input_bytes: bytes.len() - 1,
        ..ImportOptions::default()
    };
    assert!(matches!(
        import_pdf(Cursor::new(bytes), &options),
        Err(Error::Limit("input bytes"))
    ));
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("synthetic IO failure"))
        }
    }
    assert!(matches!(
        import_pdf(Broken, &ImportOptions::default()),
        Err(Error::Io(_))
    ));
    assert!(matches!(
        import_pdf(Cursor::new(b"not a PDF"), &ImportOptions::default()),
        Err(Error::InvalidPdf)
    ));
}

#[test]
fn all_configurable_zero_limits_are_rejected() {
    let fields: &[fn(&mut ImportOptions)] = &[
        |options| options.max_input_bytes = 0,
        |options| options.max_objects = 0,
        |options| options.max_pages = 0,
        |options| options.max_decoded_page_bytes = 0,
        |options| options.max_decoded_content_bytes = 0,
        |options| options.max_operations = 0,
        |options| options.max_form_depth = 0,
        |options| options.max_glyphs = 0,
        |options| options.max_text_bytes = 0,
        |options| options.max_lines = 0,
        |options| options.max_blocks = 0,
        |options| options.max_doubts = 0,
        |options| options.max_warnings = 0,
    ];
    for change in fields {
        let mut options = ImportOptions::default();
        change(&mut options);
        assert!(matches!(
            import_pdf(Cursor::new(support::sample()), &options),
            Err(Error::InvalidOptions)
        ));
    }
}

#[test]
fn output_and_content_limits_are_enforced_without_partial_results() {
    let fields: &[fn(&mut ImportOptions)] = &[
        |options| options.max_objects = 1,
        |options| options.max_pages = 1,
        |options| options.max_decoded_page_bytes = 1,
        |options| options.max_decoded_content_bytes = 1,
        |options| options.max_operations = 1,
        |options| options.max_glyphs = 1,
        |options| options.max_text_bytes = 1,
        |options| options.max_lines = 1,
        |options| options.max_blocks = 1,
        |options| options.max_doubts = 1,
    ];
    for change in fields {
        let mut options = ImportOptions::default();
        change(&mut options);
        assert!(matches!(
            import_pdf(Cursor::new(support::sample()), &options),
            Err(Error::Limit(_))
        ));
    }
    let options = ImportOptions {
        max_warnings: 1,
        ..ImportOptions::default()
    };
    assert!(matches!(
        import_pdf(
            Cursor::new(pdf(&[SamplePage {
                image: true,
                ..SamplePage::default()
            }])),
            &options
        ),
        Err(Error::Limit("warnings"))
    ));
}
