// SPDX-License-Identifier: MIT OR Apache-2.0

use serde_json::{Value, json};
use std::io::{self, Read};
use timeline_writer::*;

fn list() -> ShotList {
    let mut list = ShotList::new(FrameRate::new(24, 1));
    list.shots.push(Shot::new("synthetic", 24));
    list
}

#[test]
fn doctype_text_in_comments_does_not_hide_the_real_header_declaration() {
    let list = list();
    let xml = to_fcpxml(&list).unwrap();
    let input = xml.replace(
        "<!DOCTYPE fcpxml>",
        "<!-- comment mentions <!DOCTYPE fcpxml> -->\n<!DOCTYPE fcpxml>",
    );
    assert_eq!(from_fcpxml(&input).unwrap(), list);
    let input = xml.replace(
        "<!DOCTYPE fcpxml>",
        "<!-- mentions <!DOCTYPE fcpxml SYSTEM \"https://example.invalid/ignored.dtd\"> -->",
    );
    assert_eq!(from_fcpxml(&input).unwrap(), list);
    let input = format!(
        "\u{feff}{}",
        xml.replace("<!DOCTYPE fcpxml>", "<!-- prelude --><!DOCTYPE\nfcpxml\t>")
    );
    assert_eq!(from_fcpxml(&input).unwrap(), list);
}

#[test]
fn doctype_after_or_inside_root_is_not_repaired_into_valid_xml() {
    let xml = to_fcpxml(&list()).unwrap().replace("<!DOCTYPE fcpxml>", "");
    for input in [
        xml.replace(
            "<fcpxml version=\"1.9\">",
            "<fcpxml version=\"1.9\"><!DOCTYPE fcpxml>",
        ),
        format!("{xml}<!DOCTYPE fcpxml>"),
        xml.replace("<spine>", "<spine><!DOCTYPE fcpxml>"),
        format!("<!DOCTYPEfcpxml>{xml}"),
        format!("<!DOCTYPE\u{000b}fcpxml>{xml}"),
        format!("<!DOCTYPE fcpxml><!DOCTYPE fcpxml>{xml}"),
        format!(
            "<!-- <!DOCTYPE fcpxml> --> <!DOCTYPE fcpxml SYSTEM \"https://example.invalid/never.dtd\">{xml}"
        ),
    ] {
        assert!(from_fcpxml(&input).is_err());
    }
}

#[test]
fn safe_io_kind_codes_preserve_diagnosis_without_raw_causes() {
    for (kind, code) in [
        (io::ErrorKind::AlreadyExists, "already_exists"),
        (io::ErrorKind::NotFound, "not_found"),
        (io::ErrorKind::PermissionDenied, "permission_denied"),
        (io::ErrorKind::BrokenPipe, "broken_pipe"),
        (io::ErrorKind::Other, "io_error"),
    ] {
        let error = Error::from(io::Error::new(kind, "invented-private-path"));
        assert_eq!(error.code, code);
        assert!(!format!("{error} {error:?}").contains("invented-private"));
        assert!(std::error::Error::source(&error).is_none());
    }
}

struct FailingReader;
impl Read for FailingReader {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "invented-private-source",
        ))
    }
}

#[test]
fn input_reader_reports_structural_input_side() {
    let error = read_input(FailingReader, 1024).unwrap_err();
    assert_eq!(error.code, "permission_denied");
    assert_eq!(error.path, "input");
    assert!(!format!("{error} {error:?}").contains("invented-private"));
}

#[test]
fn external_empty_otio_infers_global_rate_without_metadata() {
    for rate in [FrameRate::new(24, 1), FrameRate::new(24000, 1001)] {
        let mut list = ShotList::new(rate);
        list.record_start = Some("01:00:00:00".into());
        let mut external: Value = serde_json::from_str(&to_otio(&list).unwrap()).unwrap();
        external["metadata"] = json!({});
        assert_eq!(from_otio(&external.to_string()).unwrap(), list);
        external["global_start_time"] = json!(null);
        assert!(from_otio(&external.to_string()).is_err());
        external["global_start_time"] =
            json!({"OTIO_SCHEMA":"RationalTime.1","value":0,"rate":23.976});
        assert!(from_otio(&external.to_string()).is_err());
        external["global_start_time"] = json!({"OTIO_SCHEMA":"RationalTime.2","value":0,"rate":24});
        assert!(from_otio(&external.to_string()).is_err());
    }
}
