// SPDX-License-Identifier: MIT OR Apache-2.0
use pg_05_breakdown_export::*;
use serde_json::Value;
use std::io::{self, Read};

fn fixture() -> Breakdown {
    from_json(include_bytes!("../samples/synthetic.json").as_slice()).unwrap()
}
fn changed(f: impl FnOnce(&mut Value)) -> Result<Breakdown> {
    let mut value: Value =
        serde_json::from_slice(include_bytes!("../samples/synthetic.json")).unwrap();
    f(&mut value);
    from_json(serde_json::to_vec(&value).unwrap().as_slice())
}
#[test]
fn defaults_and_detached_typed_model_roundtrip() {
    let value = fixture();
    assert_eq!(value.scenes[0].elements[0].quantity, 1);
    assert_eq!(value.scenes[2].synopsis, "");
    assert_eq!(value.scenes[2].notes, "");
    assert!(value.scenes[2].elements.is_empty());
    assert_eq!(
        from_json(to_json(&value).unwrap().as_bytes()).unwrap(),
        value
    );
}
#[test]
fn rejects_unknown_fields_at_every_record() {
    for path in [
        "",
        "/elements/0",
        "/scenes/0",
        "/scenes/0/elements/0",
        "/shooting_days/0",
    ] {
        let error = changed(|v| {
            v.pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("private field".into(), Value::Bool(true));
        })
        .unwrap_err();
        assert_eq!(error.code, "shape");
        assert!(!error.to_string().contains("private"));
    }
}
#[test]
fn rejects_duplicate_json_keys_including_escaped_names() {
    for json in [
        r#"{"version":1,"version":1,"elements":[],"scenes":[]}"#,
        r#"{"version":1,"elements":[],"scenes":[],"elem\u0065nts":[]}"#,
        r#"{"version":1,"elements":[{"id":"a","id":"b"}],"scenes":[]}"#,
        r#"{"version":1,"elements":[],"scenes":[],"unknown":{"secret":1,"secret":2}}"#,
    ] {
        assert_eq!(from_json(json.as_bytes()).unwrap_err().code, "json");
    }
}
#[test]
fn json_syntax_utf8_and_trailing_data_fail_without_input_disclosure() {
    for bytes in [
        b"{private input}".as_slice(),
        b"\xff".as_slice(),
        b"{} {}".as_slice(),
        b"{\"x\":NaN}".as_slice(),
        b"{\"x\":1e999}".as_slice(),
        b"{\"x\":\"\\ud800\"}".as_slice(),
    ] {
        assert_eq!(from_json(bytes).unwrap_err().code, "json");
        assert!(!from_json(bytes)
            .unwrap_err()
            .to_string()
            .contains("private"));
    }
    let mut bom = vec![0xef, 0xbb, 0xbf];
    bom.extend(include_bytes!("../samples/synthetic.json"));
    assert_eq!(from_json(bom.as_slice()).unwrap(), fixture());
}
#[test]
fn numeric_fields_do_not_coerce_float_string_negative_or_overflow() {
    for value in [
        Value::from(-1),
        Value::from(1.5),
        Value::from("8"),
        Value::from(u64::MAX),
        Value::Null,
    ] {
        assert!(changed(|v| v["scenes"][0]["pages_eighths"] = value).is_err());
    }
    assert!(changed(|v| v["version"] = Value::from(2)).is_err());
    assert!(changed(|v| v["elements"][0]["category"] = Value::from("unknown")).is_err());
    assert!(changed(|v| v["scenes"][0]["int_ext"] = Value::from("INT")).is_err());
    assert!(changed(|v| v["scenes"][0]["time_of_day"] = Value::from("whenever")).is_err());
}
#[test]
fn ids_and_scene_numbers_are_unique_without_disclosing_the_id() {
    for (path, value) in [
        ("/elements/1/id", "cast-nila"),
        ("/scenes/0/id", "cast-nila"),
        ("/shooting_days/0/id", "workshop"),
        ("/scenes/1/number", "10A"),
    ] {
        assert!(changed(|v| *v.pointer_mut(path).unwrap() = Value::from(value)).is_err());
    }
    for id in [
        "",
        " space",
        "../private",
        "a/b",
        "a\\b",
        "🙃",
        "a\n",
        "=SUM(A1)",
    ] {
        let error = changed(|v| v["scenes"][0]["id"] = Value::from(id)).unwrap_err();
        assert!(!error.to_string().contains("private"));
    }
    let error = changed(|v| v["scenes"][0]["number"] = Value::from(" ")).unwrap_err();
    assert_eq!(error.code, "text");
}
#[test]
fn duplicate_element_labels_cannot_silently_merge_in_mms() {
    assert_eq!(
        changed(|v| {
            v["elements"][1]["category"] = v["elements"][0]["category"].clone();
            v["elements"][1]["name"] = v["elements"][0]["name"].clone();
        })
        .unwrap_err()
        .code,
        "duplicate_element_label"
    );
    let mut value = fixture();
    value.elements[1].name = value.elements[0].name.clone();
    value.validate().unwrap(); // Same label in a distinct category is fine.
}
#[test]
fn references_quantities_and_single_assignment_validate() {
    assert_eq!(
        changed(|v| v["scenes"][0]["elements"][0]["element_id"] = Value::from("private-id"))
            .unwrap_err()
            .code,
        "element_reference"
    );
    assert_eq!(
        changed(|v| v["scenes"][0]["elements"][1] = v["scenes"][0]["elements"][0].clone())
            .unwrap_err()
            .code,
        "duplicate_reference"
    );
    for quantity in [0, 1_000_001] {
        assert_eq!(
            changed(|v| v["scenes"][0]["elements"][0]["quantity"] = Value::from(quantity))
                .unwrap_err()
                .code,
            "quantity"
        );
    }
    assert_eq!(
        changed(|v| v["shooting_days"][0]["scenes"][0] = Value::from("missing"))
            .unwrap_err()
            .code,
        "scene_reference"
    );
    assert_eq!(
        changed(|v| v["shooting_days"][1]["scenes"] = serde_json::json!(["yard"]))
            .unwrap_err()
            .code,
        "duplicate_assignment"
    );
    assert_eq!(
        changed(|v| v["shooting_days"][0]["scenes"] = serde_json::json!(["yard", "yard"]))
            .unwrap_err()
            .code,
        "duplicate_assignment"
    );
}
#[test]
fn xml_character_and_single_line_rules_are_shared_by_both_writers() {
    for text in ["\0", "\u{b}", "\u{1f}", "\u{fffe}", "\u{ffff}"] {
        let mut value = fixture();
        value.scenes[0].notes = text.into();
        assert_eq!(to_fdx(&value).unwrap_err().code, "xml_character");
        assert_eq!(to_csv(&value).unwrap_err().code, "xml_character");
    }
    for text in ["", "\t", "\n", "foo\rbar"] {
        assert!(changed(|v| v["scenes"][0]["set"] = Value::from(text)).is_err());
    }
    let mut value = fixture();
    value.scenes[0].notes = "tab\tline\r\nРусский العربية עברית 👩‍🚀".into();
    value.validate().unwrap();
}
#[test]
fn dates_validate_without_reordering_input_days() {
    for date in ["2024-02-29", "2000-02-29", "9999-12-31", "0001-01-01"] {
        changed(|v| v["shooting_days"][0]["date"] = Value::from(date)).unwrap();
    }
    for date in [
        "2026-02-29",
        "1900-02-29",
        "0000-01-01",
        "2026-13-01",
        "2026-04-31",
        "26-10-09",
        "２０２６-10-09",
        "2026-10-09T00:00:00Z",
    ] {
        assert!(changed(|v| v["shooting_days"][0]["date"] = Value::from(date)).is_err());
    }
    let value = fixture();
    assert!(value.shooting_days[0].date > value.shooting_days[1].date);
}
#[test]
fn typed_api_limits_do_not_depend_on_json_byte_limit() {
    let mut value = fixture();
    value.scenes[0].notes = "a".repeat(MAX_TEXT_BYTES + 1);
    assert_eq!(value.validate().unwrap_err().code, "text_limit");
    let mut value = fixture();
    value.scenes[0].pages_eighths = MAX_PAGE_EIGHTHS + 1;
    assert_eq!(value.validate().unwrap_err().code, "page_limit");
    let mut value = fixture();
    value.scenes = vec![value.scenes[0].clone(); MAX_RECORDS + 1];
    assert_eq!(value.validate().unwrap_err().code, "record_limit");
    let mut value = fixture();
    value.scenes[0].elements = vec![value.scenes[0].elements[0].clone(); MAX_REFERENCES + 1];
    assert_eq!(value.validate().unwrap_err().code, "reference_limit");
    let mut value = fixture();
    value.elements[0].id = "a".repeat(129);
    assert_eq!(value.validate().unwrap_err().code, "text_limit");
}
#[test]
fn empty_document_is_valid_and_exports_empty_tables() {
    let value = from_json(b"{\"version\":1,\"elements\":[],\"scenes\":[]}".as_slice()).unwrap();
    assert_eq!(value.shooting_days.len(), 0);
    assert!(to_fdx(&value).unwrap().contains("<Content>\n  </Content>"));
    assert_eq!(to_csv(&value).unwrap().lines().count(), 1);
}
#[test]
fn reader_size_depth_and_io_errors_are_bounded() {
    let error = from_json(io::repeat(b' ').take(MAX_INPUT_BYTES as u64 + 2)).unwrap_err();
    assert_eq!(error.code, "input_limit");
    let nested = format!("{}0{}", "[".repeat(40), "]".repeat(40));
    assert_eq!(from_json(nested.as_bytes()).unwrap_err().code, "json");
    let many = format!("[{}0]", "0,".repeat(500_000));
    assert_eq!(from_json(many.as_bytes()).unwrap_err().code, "json");
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("private filename"))
        }
    }
    assert_eq!(from_json(Broken).unwrap_err().to_string(), "read at $");
}
#[test]
fn compact_json_roundtrip_does_not_inflate_many_accepted_default_references() {
    let elements: Vec<_> = (0..20)
        .map(|i| {
            serde_json::json!({
                "id":format!("e{i}"),"name":format!("Item {i}"),"category":"props"
            })
        })
        .collect();
    let references: Vec<_> = (0..20)
        .map(|i| serde_json::json!({"element_id":format!("e{i}")}))
        .collect();
    let scenes: Vec<_> = (0..7_500)
        .map(|i| {
            serde_json::json!({
                "id":format!("s{i}"),"number":i.to_string(),"int_ext":"interior",
                "set":"Room","time_of_day":"day","pages_eighths":1,"elements":references
            })
        })
        .collect();
    let source =
        serde_json::to_vec(&serde_json::json!({"version":1,"elements":elements,"scenes":scenes}))
            .unwrap();
    assert!(source.len() < MAX_INPUT_BYTES);
    let value = from_json(source.as_slice()).unwrap();
    assert!(serde_json::to_vec_pretty(&value).unwrap().len() > MAX_INPUT_BYTES);
    let compact = to_json(&value).unwrap();
    assert!(compact.len() <= source.len());
    assert_eq!(from_json(compact.as_bytes()).unwrap(), value);
}
#[test]
fn unicode_line_separators_reject_in_all_single_line_fields() {
    for separator in ['\u{2028}', '\u{2029}'] {
        for path in [
            "/title",
            "/elements/0/name",
            "/scenes/0/number",
            "/scenes/0/set",
            "/scenes/0/script_day",
            "/scenes/0/unit",
            "/shooting_days/0/label",
        ] {
            let error =
                changed(|v| *v.pointer_mut(path).unwrap() = Value::from(format!("a{separator}b")))
                    .unwrap_err();
            assert_eq!(error.code, "text");
        }
        let mut value = fixture();
        value.scenes[0].synopsis = format!("a{separator}b");
        value.scenes[0].notes = format!("c{separator}d");
        value.validate().unwrap();
    }
}
