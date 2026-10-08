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
fn synthetic_sample_keeps_both_languages_and_all_element_kinds() {
    let screenplay = read(support::sample());
    assert_eq!(screenplay.pages.len(), 2);
    assert_eq!(screenplay.lines.len(), 14);
    assert_eq!(screenplay.lines[0].text, "1 INT. AMBER ROOM - DAY");
    assert_eq!(screenplay.lines[7].text, "2 НАТ. СИНТЕТИЧЕСКИЙ ДВОР - ДЕНЬ");
    assert_eq!(screenplay.lines[11].text, "Это только вымышленный пример.");
    for kind in [
        ElementKind::SceneHeading,
        ElementKind::Action,
        ElementKind::Character,
        ElementKind::Dialogue,
        ElementKind::Parenthetical,
        ElementKind::Transition,
        ElementKind::Unknown,
    ] {
        assert!(
            screenplay.blocks.iter().any(|block| block.kind == kind),
            "{kind:?}"
        );
    }
    assert_eq!(screenplay.blocks[4].speaker.as_deref(), Some("block-3"));
    assert_eq!(screenplay.blocks[4].scene.as_deref(), Some("block-1"));
    assert!(screenplay
        .doubts
        .iter()
        .any(|doubt| doubt.reasons.contains(&Reason::AmbiguousUppercase)));
    assert!(screenplay
        .doubts
        .iter()
        .any(|doubt| doubt.reasons.contains(&Reason::PossibleHeaderFooter)));
}

#[test]
fn glyph_positions_use_top_left_page_points() {
    let screenplay = single(vec![TextLine::new("A synthetic line.", 72.0, 720.0)]);
    let line = &screenplay.lines[0];
    assert!((line.baseline[0] - 72.0).abs() < 0.001);
    assert!((line.baseline[1] - 72.0).abs() < 0.001);
    assert!((line.font_size - 12.0).abs() < 0.001);
    assert!((line.bounds.left - 72.0).abs() < 0.001);
    assert!((line.bounds.right - (72.0 + 17.0 * 7.2)).abs() < 0.001);
    assert!(line.bounds.top < line.baseline[1]);
    assert!(line.bounds.bottom > line.baseline[1]);
}

#[test]
fn blank_and_image_only_pages_are_distinguished() {
    let screenplay = read(pdf(&[
        SamplePage {
            no_content: true,
            ..SamplePage::default()
        },
        SamplePage {
            image: true,
            ..SamplePage::default()
        },
    ]));
    assert_eq!(screenplay.pages[0].status, PageStatus::Blank);
    assert_eq!(
        screenplay.pages[1].status,
        PageStatus::UnsupportedNoTextLayer
    );
    assert!(screenplay.lines.is_empty());
    assert!(screenplay
        .warnings
        .iter()
        .any(|warning| warning.kind == WarningKind::NoExtractableText));
}

#[test]
fn geometric_reading_order_does_not_follow_content_object_order() {
    let screenplay = single(vec![
        TextLine::new("Lower synthetic line.", 72.0, 690.0),
        TextLine::new("Upper synthetic line.", 72.0, 720.0),
    ]);
    assert_eq!(screenplay.lines[0].text, "Upper synthetic line.");
    assert_eq!(screenplay.lines[1].text, "Lower synthetic line.");
    assert!(screenplay.lines[0].draw_order > screenplay.lines[1].draw_order);
}

#[test]
fn crop_translation_is_applied_before_provenance() {
    let screenplay = read(pdf(&[SamplePage {
        lines: vec![TextLine::new("Synthetic.", 72.0, 720.0)],
        crop: Some([20.0, 40.0, 600.0, 780.0]),
        ..SamplePage::default()
    }]));
    assert_eq!(screenplay.pages[0].width, 580.0);
    assert_eq!(screenplay.pages[0].height, 740.0);
    assert!((screenplay.lines[0].baseline[0] - 52.0).abs() < 0.001);
    assert!((screenplay.lines[0].baseline[1] - 60.0).abs() < 0.001);
}

#[test]
fn geometry_ambiguity_keeps_text_and_reports_doubts() {
    let screenplay = single(vec![
        TextLine::new("LEFT", 72.0, 720.0),
        TextLine::new("RIGHT", 350.0, 720.0),
    ]);
    assert_eq!(screenplay.lines.len(), 2);
    assert!(screenplay
        .lines
        .iter()
        .all(|line| line.kind == ElementKind::Unknown));
    assert!(screenplay
        .doubts
        .iter()
        .all(|doubt| doubt.reasons.contains(&Reason::MultipleColumns)));
}
