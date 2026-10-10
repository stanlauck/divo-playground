// SPDX-License-Identifier: MIT OR Apache-2.0
use std::fs;
use std::path::PathBuf;
use storyboard_sheets::*;

fn board(n: usize) -> Board {
    Board {
        version: 1,
        title: Some("T".into()),
        frame_rate: Some(FrameRate::Integer(24)),
        aspect: Aspect::Wide,
        shots: (0..n)
            .map(|i| Shot {
                id: format!("s{i}"),
                name: Some(format!("N{i}")),
                duration_frames: Some(48),
                scene: Some("scene".into()),
                description: Some("desc".into()),
                dialogue: Some("line".into()),
                camera: Some("WS".into()),
                frame: None,
                notes: None,
            })
            .collect(),
    }
}
fn opts() -> Options {
    Options {
        assets_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")),
        ..Options::default()
    }
}

#[test]
fn grid_dimensions_and_pages() {
    assert_eq!(Grid::TwoByThree.dimensions(), (2, 3));
    assert_eq!(Grid::ThreeByFour.dimensions(), (3, 4));
    for g in [Grid::TwoByThree, Grid::ThreeByFour] {
        assert_eq!(g.page_count(0), 1);
        assert_eq!(g.page_count(1), 1);
        assert_eq!(g.page_count(g.cells_per_page()), 1);
        assert_eq!(g.page_count(g.cells_per_page() + 1), 2);
    }
}
#[test]
fn parse_optional_fields_and_unknowns() {
    let b = from_json(br#"{"version":1,"wat":true,"shots":[{"id":"x","extra":{"a":1}}]}"#).unwrap();
    assert_eq!(b.shots[0].id, "x");
    assert_eq!(b.frame_rate, None);
}
#[test]
fn rejects_version_ids_and_duplicates() {
    assert!(from_json(br#"{"version":2,"shots":[]}"#).is_err());
    assert!(from_json(br#"{"version":1,"shots":[{"id":"x"},{"id":"x"}]}"#).is_err());
    assert!(from_json(br#"{"version":1,"shots":[{"id":" x"}]}"#).is_err());
}
#[test]
fn rejects_all_bad_paths() {
    for p in [
        "../x.png",
        "/x.png",
        "https://x/a.png",
        "a\\b.png",
        "C:x.png",
        "a//b.png",
        "./x.png",
        "x.txt",
        "x.webp",
        "",
    ] {
        assert!(validate_image_path(p).is_err(), "{p}");
    }
    for p in ["x.PNG", "frames/a.svg", "a.jpg"] {
        assert!(validate_image_path(p).is_ok(), "{p}");
    }
}
#[test]
fn rates_and_timecodes() {
    for (r, n) in [
        (FrameRate::Integer(24), "00:00:01:00"),
        (FrameRate::Integer(25), "00:00:01:00"),
        (FrameRate::Integer(30), "00:00:01:00"),
        (
            FrameRate::Rational {
                num: 24000,
                den: 1001,
            },
            "00:00:01:00",
        ),
        (
            FrameRate::Rational {
                num: 23976,
                den: 1000,
            },
            "00:00:01:00",
        ),
    ] {
        assert_eq!(format_timecode(r.nominal().unwrap(), r).unwrap(), n);
    }
    assert_eq!(
        format_timecode(86_400, FrameRate::Integer(24)).unwrap(),
        "01:00:00:00"
    );
    assert!(FrameRate::Integer(23).nominal().is_err());
}
#[test]
fn escaping_covers_typst_specials() {
    let e = escape_typst("\\ \" # * _ [ ] $ @ < >\n\t");
    assert!(e.starts_with('"') && e.ends_with('"'));
    assert!(!e.contains("\\\n"));
}
#[test]
fn layout_options_are_validated() {
    let mut o = opts();
    assert!(o.layout(Aspect::Vertical).unwrap().frame_height > 0.0);
    o.max_caption_chars = 0;
    assert!(o.layout(Aspect::Wide).is_err());
    let mut o = opts();
    o.caption_fields.push(CaptionField::Name);
    assert!(o.layout(Aspect::Wide).is_err());
}
#[test]
fn deterministic_source_and_warnings() {
    let a = render_typst(&board(7), &opts()).unwrap();
    let b = render_typst(&board(7), &opts()).unwrap();
    assert_eq!(a.source, b.source);
    assert_eq!(a.warnings, b.warnings);
    assert_eq!(a.warnings.len(), 7);
    assert!(a.source.contains("no frame"));
}
#[test]
fn timecodes_and_dialogue_are_captions() {
    let mut b = board(1);
    let o = Options {
        show_timecodes: true,
        ..opts()
    };
    let r = render_typst(&b, &o).unwrap();
    assert!(r.source.contains("00:00:02:00"));
    assert!(r.source.contains("italic"));
    b.shots[0].duration_frames = None;
    let r = render_typst(&b, &o).unwrap();
    assert!(!r.source.contains("00:00:02:00"));
}
#[test]
fn title_page_and_pages() {
    let o = Options {
        title_page: true,
        start_page_number: 4,
        ..opts()
    };
    let r = render_typst(&board(13), &o).unwrap();
    assert!(r.source.contains("13 shots"));
    assert!(r.source.contains("of 7"));
}
#[test]
fn assets_missing_outside_and_present() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("samples");
    let mut b = board(3);
    b.shots[0].frame = Some(Frame {
        path: "frames/S01.svg".into(),
        alt: None,
    });
    b.shots[1].frame = Some(Frame {
        path: "frames/outside.png".into(),
        alt: None,
    });
    b.shots[2].frame = Some(Frame {
        path: "frames/no.png".into(),
        alt: None,
    });
    let o = Options {
        assets_root: root,
        ..Options::default()
    };
    let r = render_typst(&b, &o).unwrap();
    assert_eq!(r.assets, vec![PathBuf::from("frames/S01.svg")]);
    assert_eq!(r.warnings.len(), 2);
}
#[test]
fn limits() {
    let huge = vec![b' '; MAX_INPUT_BYTES + 1];
    assert!(matches!(from_json(huge), Err(Error::Limit(_))));
    let b = Board {
        version: 1,
        title: None,
        frame_rate: None,
        aspect: Aspect::Wide,
        shots: (0..MAX_SHOTS + 1)
            .map(|i| Shot {
                id: i.to_string(),
                name: None,
                duration_frames: None,
                scene: None,
                description: None,
                dialogue: None,
                camera: None,
                frame: None,
                notes: None,
            })
            .collect(),
    };
    assert!(matches!(b.validate(), Err(Error::Limit(_))));
}
#[test]
fn optional_typst_pdf() {
    if std::process::Command::new("typst")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("samples");
    let o = Options {
        assets_root: root.clone(),
        ..Options::default()
    };
    let r = render_typst(&board(1), &o).unwrap();
    let out = std::env::temp_dir().join(format!("pg22-{}.pdf", std::process::id()));
    let _ = fs::remove_file(&out);
    compile_pdf(&r, &out, &root, None).unwrap();
    let bytes = fs::read(&out).unwrap();
    assert!(bytes.starts_with(b"%PDF"));
    let _ = fs::remove_file(out);
}

macro_rules! one_case {
    ($name:ident, $body:expr) => {
        #[test]
        fn $name() {
            $body;
        }
    };
}
one_case!(
    page_count_2x3_zero,
    assert_eq!(Grid::TwoByThree.page_count(0), 1)
);
one_case!(
    page_count_2x3_one,
    assert_eq!(Grid::TwoByThree.page_count(1), 1)
);
one_case!(
    page_count_2x3_six,
    assert_eq!(Grid::TwoByThree.page_count(6), 1)
);
one_case!(
    page_count_2x3_seven,
    assert_eq!(Grid::TwoByThree.page_count(7), 2)
);
one_case!(
    page_count_2x3_twelve,
    assert_eq!(Grid::TwoByThree.page_count(12), 2)
);
one_case!(
    page_count_2x3_thirteen,
    assert_eq!(Grid::TwoByThree.page_count(13), 3)
);
one_case!(
    page_count_3x4_zero,
    assert_eq!(Grid::ThreeByFour.page_count(0), 1)
);
one_case!(
    page_count_3x4_one,
    assert_eq!(Grid::ThreeByFour.page_count(1), 1)
);
one_case!(
    page_count_3x4_six,
    assert_eq!(Grid::ThreeByFour.page_count(6), 1)
);
one_case!(
    page_count_3x4_seven,
    assert_eq!(Grid::ThreeByFour.page_count(7), 1)
);
one_case!(
    page_count_3x4_twelve,
    assert_eq!(Grid::ThreeByFour.page_count(12), 1)
);
one_case!(
    page_count_3x4_thirteen,
    assert_eq!(Grid::ThreeByFour.page_count(13), 2)
);
one_case!(
    path_reject_parent,
    assert!(validate_image_path("../frame.png").is_err())
);
one_case!(
    path_reject_absolute,
    assert!(validate_image_path("/frame.png").is_err())
);
one_case!(
    path_reject_url,
    assert!(validate_image_path("https://example.test/frame.png").is_err())
);
one_case!(
    path_reject_backslash,
    assert!(validate_image_path("dir\\frame.png").is_err())
);
one_case!(
    path_reject_extension,
    assert!(validate_image_path("frame.gif").is_err())
);
one_case!(
    path_accept_svg,
    assert!(validate_image_path("dir/frame.svg").is_ok())
);
one_case!(
    path_accept_jpg,
    assert!(validate_image_path("frame.jpg").is_ok())
);
one_case!(
    path_accept_png_case,
    assert!(validate_image_path("frame.PnG").is_ok())
);
one_case!(
    aspect_wide,
    assert!((Aspect::Wide.ratio() - 16.0 / 9.0).abs() < 0.001)
);
one_case!(
    aspect_classic,
    assert!((Aspect::Classic.ratio() - 4.0 / 3.0).abs() < 0.001)
);
one_case!(
    aspect_cinema,
    assert!((Aspect::Cinema.ratio() - 2.39).abs() < 0.001)
);
one_case!(aspect_square, assert_eq!(Aspect::Square.ratio(), 1.0));
one_case!(
    aspect_vertical,
    assert!((Aspect::Vertical.ratio() - 9.0 / 16.0).abs() < 0.001)
);
