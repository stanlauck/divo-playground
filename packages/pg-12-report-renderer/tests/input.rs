// SPDX-License-Identifier: MIT OR Apache-2.0
mod common;
use common::*;
use report_renderer::*;
use serde_json::{json, Value};

fn base() -> Value {
    serde_json::to_value(minimal()).unwrap()
}
fn check(value: Value) -> Result<Report, Error> {
    from_json(&value.to_string())
}
fn with_block(block: Value) -> Value {
    let mut v = base();
    v["sections"][0]["blocks"] = json!([block]);
    v
}

#[test]
fn defaults() {
    let r = from_json(
        r#"{"version":1,"title":"Example","sections":[{"id":"one","title":"One","blocks":[]}]}"#,
    )
    .unwrap();
    let s = &r.sections[0];
    assert_eq!(s.level, 1);
    assert!(!s.optional);
    assert!(s.enabled);
    assert!(s.sections.is_empty());
}
#[test]
fn sample_roundtrip() {
    let a = sample();
    let b = from_json(&serde_json::to_string(&a).unwrap()).unwrap();
    assert_eq!(
        render_markdown(&a, &RenderOptions::default()),
        render_markdown(&b, &RenderOptions::default())
    );
}
#[test]
fn invalid_utf8() {
    assert!(matches!(read_report(&b"\xff"[..]), Err(Error::Invalid(_))));
}
#[test]
fn malformed_json() {
    assert!(matches!(from_json("{"), Err(Error::Json(_))));
}
#[test]
fn duplicate_json_key() {
    assert!(from_json(r#"{"version":1,"title":"a","title":"b","sections":[]}"#).is_err());
}
#[test]
fn trailing_json() {
    assert!(from_json(r#"{"version":1,"title":"a","sections":[]} []"#).is_err());
}
#[test]
fn missing_required_fields() {
    for key in ["version", "title", "sections"] {
        let mut v = base();
        v.as_object_mut().unwrap().remove(key);
        assert!(check(v).is_err(), "{key}");
    }
}
#[test]
fn unknown_fields() {
    let mut v = base();
    v["extra"] = json!(1);
    assert!(check(v).is_err());
    let mut v = base();
    v["sections"][0]["extra"] = json!(1);
    assert!(check(v).is_err());
    assert!(check(with_block(json!({"type":"paragraph","text":"a","extra":1}))).is_err());
}
#[test]
fn unknown_block() {
    assert!(check(with_block(json!({"type":"script","text":"a"}))).is_err());
}
#[test]
fn unknown_callout_kind() {
    assert!(check(with_block(
        json!({"type":"callout","kind":"info","text":"a"})
    ))
    .is_err());
}
#[test]
fn empty_title_and_id() {
    for text in ["", " \t\n"] {
        let mut v = base();
        v["title"] = json!(text);
        assert!(check(v).is_err());
        let mut v = base();
        v["sections"][0]["id"] = json!(text);
        assert!(check(v).is_err());
        let mut v = base();
        v["sections"][0]["title"] = json!(text);
        assert!(check(v).is_err());
    }
}
#[test]
fn duplicate_section_id_in_descendant() {
    let mut r = minimal();
    let duplicate = r.sections[0].clone();
    r.sections[0].sections.push(duplicate);
    assert!(matches!(validate(&r), Err(Error::Invalid(_))));
}
#[test]
fn unsupported_version() {
    let mut v = base();
    v["version"] = json!(2);
    assert!(check(v).is_err());
}
#[test]
fn invalid_section_level() {
    for n in [0, 5, 256] {
        let mut v = base();
        v["sections"][0]["level"] = json!(n);
        assert!(check(v).is_err());
    }
}
#[test]
fn invalid_block_heading_level() {
    for n in [0, 5] {
        assert!(check(with_block(json!({"type":"heading","level":n,"text":"x"}))).is_err());
    }
}
#[test]
fn empty_table_columns() {
    assert!(check(with_block(json!({"type":"table","columns":[],"rows":[]}))).is_err());
}
#[test]
fn ragged_table() {
    for row in [json!([]), json!(["a", "b"])] {
        assert!(check(with_block(
            json!({"type":"table","columns":["a"],"rows":[row]})
        ))
        .is_err());
    }
}
#[test]
fn pairs_require_two_strings() {
    for pair in [json!(["a"]), json!(["a", "b", "c"]), json!(["a", 4])] {
        assert!(check(with_block(json!({"type":"key_values","pairs":[pair]}))).is_err());
    }
}
#[test]
fn code_language_tokens() {
    for lang in ["", "a b", "a\nb", "<evil>", "rust`"] {
        assert!(check(with_block(
            json!({"type":"code","text":"x","language":lang})
        ))
        .is_err());
    }
    for lang in ["c++", "c-sharp", "plain_text"] {
        assert!(check(with_block(
            json!({"type":"code","text":"x","language":lang})
        ))
        .is_ok());
    }
}
#[test]
fn date_validation() {
    for date in ["2026-10-10", "2024-02-29", "2000-02-29"] {
        let mut v = base();
        v["date"] = json!(date);
        assert!(check(v).is_ok(), "{date}");
    }
    for date in [
        "2026-02-29",
        "1900-02-29",
        "0000-01-01",
        "2026-13-01",
        "2026-01-00",
        "2026-04-31",
        "2026-10-10T12:00:00Z",
        "текст",
    ] {
        let mut v = base();
        v["date"] = json!(date);
        assert!(check(v).is_err(), "{date}");
    }
}
#[test]
fn bcp47_tags() {
    for lang in [
        "en",
        "ru-RU",
        "zh-Hans-CN",
        "de-CH-1901",
        "sl-rozaj-biske",
        "en-US-u-ca-gregory",
        "x-example",
        "i-klingon",
        "en-a-ab-x-test",
    ] {
        let mut v = base();
        v["language"] = json!(lang);
        assert!(check(v).is_ok(), "{lang}");
    }
    for lang in [
        "",
        "en_US",
        "a",
        "x",
        "en-u",
        "en--US",
        "123",
        "en-a-foo-a-bar",
        "sl-rozaj-rozaj",
        "en-abcdefghij",
        "en-!",
        "en-1",
    ] {
        let mut v = base();
        v["language"] = json!(lang);
        assert!(check(v).is_err(), "{lang}");
    }
}
#[test]
fn optional_null_metadata() {
    let mut v = base();
    for key in ["subtitle", "author", "date", "language", "summary"] {
        v[key] = Value::Null;
    }
    assert!(check(v).is_ok());
}
#[test]
fn image_paths_reject_traversal_absolute_urls() {
    for path in [
        "",
        "../a.png",
        "a/../b.png",
        "/a.png",
        "//server/a",
        "C:/a.png",
        "C:\\a.png",
        "a\\..\\b",
        "https://example.test/a",
        "data:image/png,x",
        "file:a",
        "a//b",
        "./a",
        "a/./b",
        "a/",
        "a?b",
        "a#b",
        "a\nb",
        "a\0b",
        "a\u{85}b",
    ] {
        assert!(
            check(with_block(json!({"type":"image","path":path}))).is_err(),
            "{path:?}"
        );
    }
}
#[test]
fn image_paths_accept_relative_unicode_and_punctuation() {
    for path in [
        "a.png",
        "assets/фонарь.png",
        "a b_(c)*.png",
        "a%20b.png",
        "..named.png",
    ] {
        assert!(
            check(with_block(json!({"type":"image","path":path}))).is_ok(),
            "{path}"
        );
    }
}
#[test]
fn image_width_validation() {
    for width in [0.0, -1.0, 100.1] {
        assert!(check(with_block(
            json!({"type":"image","path":"a","width_percent":width})
        ))
        .is_err());
    }
    for width in [0.1, 50.0, 100.0] {
        assert!(check(with_block(
            json!({"type":"image","path":"a","width_percent":width})
        ))
        .is_ok());
    }
}
#[test]
fn nonfinite_programmatic_width() {
    let mut r = minimal();
    r.sections[0].blocks.push(Block::Image {
        path: "a".into(),
        caption: None,
        width_percent: Some(f64::NAN),
    });
    assert!(validate(&r).is_err());
}
#[test]
fn input_byte_limit_boundary() {
    let mut text = serde_json::to_string(&minimal()).unwrap();
    text.push_str(&" ".repeat(MAX_INPUT_BYTES - text.len()));
    assert!(from_json(&text).is_ok());
    text.push(' ');
    assert!(matches!(from_json(&text), Err(Error::Limit(_))));
    assert!(matches!(read_report(text.as_bytes()), Err(Error::Limit(_))));
}
#[test]
fn reader_is_bounded() {
    struct Endless(usize);
    impl std::io::Read for Endless {
        fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
            b.fill(b' ');
            self.0 += b.len();
            Ok(b.len())
        }
    }
    let mut r = Endless(0);
    assert!(matches!(read_report(&mut r), Err(Error::Limit(_))));
    assert_eq!(r.0, MAX_INPUT_BYTES + 1);
}
#[test]
fn section_limit_boundary() {
    let mut r = minimal();
    let s = r.sections[0].clone();
    r.sections = (0..MAX_SECTIONS)
        .map(|i| {
            let mut s = s.clone();
            s.id = i.to_string();
            s
        })
        .collect();
    assert!(validate(&r).is_ok());
    let mut s = s;
    s.id = "extra".into();
    r.sections.push(s);
    assert!(matches!(validate(&r), Err(Error::Limit(_))));
}
#[test]
fn block_limit_is_global_including_disabled_sections() {
    let mut r = minimal();
    r.sections[0].blocks = vec![Block::PageBreak {}; MAX_BLOCKS];
    assert!(validate(&r).is_ok());
    let mut s = r.sections[0].clone();
    s.id = "extra".into();
    s.enabled = false;
    s.blocks = vec![Block::PageBreak {}];
    r.sections.push(s);
    assert!(matches!(validate(&r), Err(Error::Limit(_))));
}
#[test]
fn depth_limit_boundary() {
    let mut r = minimal();
    let mut root = r.sections.pop().unwrap();
    for i in 1..MAX_DEPTH {
        let mut parent = root.clone();
        parent.id = i.to_string();
        parent.sections = vec![root];
        root = parent;
    }
    r.sections = vec![root];
    assert!(validate(&r).is_ok());
    assert!(from_json(&serde_json::to_string(&r).unwrap()).is_ok());
    let mut parent = r.sections[0].clone();
    parent.id = "extra".into();
    parent.sections = std::mem::take(&mut r.sections);
    r.sections = vec![parent];
    assert!(matches!(validate(&r), Err(Error::Limit(_))));
}
#[test]
fn schema_is_valid_json_and_declares_all_blocks() {
    let schema: Value = serde_json::from_str(include_str!("../schema.json")).unwrap();
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(
        schema["$defs"]["block"]["oneOf"].as_array().unwrap().len(),
        11
    );
}
