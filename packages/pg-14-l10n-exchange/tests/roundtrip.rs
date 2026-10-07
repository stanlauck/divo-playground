// SPDX-License-Identifier: MIT OR Apache-2.0

use l10n_exchange::{
    Entry, Error, State, StringTable, from_csv, from_json, from_xliff, to_csv, to_json, to_xliff,
};

const SAMPLE: &str = include_str!("../samples/strings.json");

fn sample() -> StringTable {
    from_json(SAMPLE).expect("sample parses")
}

fn tricky() -> StringTable {
    let mut t = StringTable::new("en", Some("ar".into()));
    let mut a = Entry::new("a.1", "  leading and trailing spaces  ");
    a.target = Some("مرحبا\r\nبالعالم".into());
    a.character = Some("Nadia, \"the Clockmaker\"".into());
    a.context = Some("tab\there; comma, quote \" and <tag> & amp".into());
    a.state = Some(State::Translated);
    let mut b = Entry::new("b:2", "Combining: e\u{301} 👩🏽‍🚀 漢字");
    b.note = Some("multi\nline\nnote".into());
    b.max_length = Some(0);
    t.strings = vec![a, b];
    t
}

#[test]
fn json_roundtrip() {
    let t = sample();
    assert_eq!(from_json(&to_json(&t).unwrap()).unwrap(), t);
}

#[test]
fn xliff_roundtrip_sample_and_tricky() {
    for t in [sample(), tricky()] {
        let xml = to_xliff(&t).unwrap();
        assert_eq!(from_xliff(&xml).unwrap(), t, "xliff:\n{xml}");
    }
}

#[test]
fn csv_roundtrip_sample_and_tricky() {
    for t in [sample(), tricky()] {
        let csv = to_csv(&t).unwrap();
        let back = from_csv(csv.as_bytes(), &t.source_lang, t.target_lang.as_deref()).unwrap();
        assert_eq!(back, t, "csv:\n{csv}");
    }
}

#[test]
fn json_to_xliff_to_csv_to_json() {
    let t = sample();
    let x = from_xliff(&to_xliff(&t).unwrap()).unwrap();
    let c = from_csv(to_csv(&x).unwrap().as_bytes(), "en", Some("de")).unwrap();
    assert_eq!(from_json(&to_json(&c).unwrap()).unwrap(), t);
}

#[test]
fn committed_samples_match_writer_output() {
    let t = sample();
    assert_eq!(to_json(&t).unwrap(), SAMPLE);
    assert_eq!(
        to_xliff(&t).unwrap(),
        include_str!("../samples/strings.xlf")
    );
    assert_eq!(to_csv(&t).unwrap(), include_str!("../samples/strings.csv"));
}

#[test]
fn xliff_output_shape() {
    let xml = to_xliff(&sample()).unwrap();
    assert!(xml.contains(r#"version="2.1" srcLang="en" trgLang="de""#));
    assert!(xml.contains(r#"<note category="character">Keeper Odile</note>"#));
    assert!(xml.contains(r#"<note category="context">Main menu button</note>"#));
    assert!(xml.contains(r#"<note category="comment">Quotes are part"#));
    assert!(xml.contains(r#"<unit id="ui.button.continue" slr:sizeRestriction="12">"#));
    assert!(xml.contains(r#"<slr:profiles generalProfile="xliff:codepoints"/>"#));
    assert!(xml.contains(r#"<segment state="final">"#));
    assert!(xml.contains("<source>Saved &lt;slot 3&gt;</source>"));
    // Parses as plain XML too.
    roxmltree::Document::parse(&xml).unwrap();
}

#[test]
fn xliff_without_limits_has_no_slr_namespace() {
    let mut t = sample();
    for e in &mut t.strings {
        e.max_length = None;
    }
    assert!(!to_xliff(&t).unwrap().contains("slr"));
}

#[test]
fn csv_output_shape() {
    let csv = to_csv(&sample()).unwrap();
    let mut lines = csv.split("\r\n");
    assert_eq!(
        lines.next().unwrap(),
        "id,source,target,context,character,note,max_length,state"
    );
    assert_eq!(
        lines.nth(3).unwrap(),
        "ui.button.continue,Continue,Weiter,Main menu button,,,12,final"
    );
}

#[test]
fn reads_foreign_xliff_with_groups_segments_and_ignorables() {
    let xml = r#"<?xml version="1.0"?>
<xliff xmlns="urn:oasis:names:tc:xliff:document:2.0" version="2.0" srcLang="en" trgLang="fr">
  <file id="one">
    <group id="g">
      <unit id="u1">
        <notes>
          <note category="character">Pilot</note>
          <note>first</note>
          <note category="other">second</note>
        </notes>
        <segment state="final"><source>Hello.</source><target>Bonjour.</target></segment>
        <ignorable><source> </source></ignorable>
        <segment state="translated"><source>Bye.</source><target>Salut.</target></segment>
      </unit>
    </group>
  </file>
  <file id="two">
    <unit id="u2"><segment><source>No <!-- c -->target</source></segment></unit>
  </file>
</xliff>"#;
    let t = from_xliff(xml).unwrap();
    assert_eq!(t.target_lang.as_deref(), Some("fr"));
    let u1 = &t.strings[0];
    assert_eq!(u1.source, "Hello. Bye.");
    assert_eq!(u1.target.as_deref(), Some("Bonjour. Salut."));
    assert_eq!(u1.character.as_deref(), Some("Pilot"));
    assert_eq!(u1.note.as_deref(), Some("first\nsecond"));
    assert_eq!(u1.state, Some(State::Translated));
    let u2 = &t.strings[1];
    assert_eq!(u2.source, "No target");
    assert_eq!(u2.target, None);
    assert_eq!(u2.state, None);
}

#[test]
fn rejects_xliff_inline_markup_unknown_profile_and_v1() {
    let inline = r#"<xliff xmlns="urn:oasis:names:tc:xliff:document:2.0" version="2.1" srcLang="en">
      <file id="f"><unit id="u"><segment><source>Hi <ph id="1"/></source></segment></unit></file></xliff>"#;
    assert!(matches!(from_xliff(inline), Err(Error::Unsupported(_))));

    let profile = r#"<xliff xmlns="urn:oasis:names:tc:xliff:document:2.0"
      xmlns:slr="urn:oasis:names:tc:xliff:sizerestriction:2.0" version="2.1" srcLang="en">
      <file id="f"><slr:profiles generalProfile="xliff:utf8"/>
      <unit id="u"><segment><source>Hi</source></segment></unit></file></xliff>"#;
    assert!(matches!(from_xliff(profile), Err(Error::Unsupported(_))));

    let v1 = r#"<xliff xmlns="urn:oasis:names:tc:xliff:document:1.2" version="1.2"></xliff>"#;
    assert!(matches!(from_xliff(v1), Err(Error::Invalid(_))));

    let dtd = r#"<!DOCTYPE x [<!ENTITY e "boom">]><xliff/>"#;
    assert!(matches!(from_xliff(dtd), Err(Error::Xml(_))));
}

#[test]
fn csv_accepts_bom_any_column_order_and_minimal_columns() {
    let input = "\u{feff}source,id,state\r\nHello,greet,reviewed\n\"a, \"\"b\"\"\",x\n";
    let t = from_csv(input.as_bytes(), "en", None).unwrap();
    assert_eq!(t.strings[0].id, "greet");
    assert_eq!(t.strings[0].state, Some(State::Reviewed));
    assert_eq!(t.strings[1].source, "a, \"b\"");
    assert_eq!(t.strings[1].state, None);
}

#[test]
fn csv_rejects_bad_input() {
    let cases = [
        "id,source,colour\na,b,c\n",
        "id\na\n",
        "id,source,max_length\na,b,ten\n",
        "id,source,state\na,b,done\n",
        "id,source\na,b\na,c\n",
        "id,source,id\na,b,c\n",
        "id,source\na,b,extra\n",
    ];
    for input in cases {
        assert!(from_csv(input.as_bytes(), "en", None).is_err(), "{input:?}");
    }
}

#[test]
fn validation_rules() {
    let mut t = sample();
    t.target_lang = None;
    assert!(t.validate().is_err(), "targets need target_lang");

    let mut t = sample();
    t.strings[0].id = "has space".into();
    assert!(t.validate().is_err());

    let mut t = sample();
    t.strings[0].source = "bell\u{7}".into();
    assert!(t.validate().is_err());

    let mut t = sample();
    t.source_lang = "en_US".into();
    assert!(t.validate().is_err());

    let mut t = sample();
    t.version = 2;
    assert!(to_json(&t).is_err());

    assert!(from_json(r#"{"version":1,"source_lang":"en","strings":[],"extra":1}"#).is_err());
}

#[test]
fn empty_strings_mean_absent() {
    let t = from_json(
        r#"{"version":1,"source_lang":"en","target_lang":"",
            "strings":[{"id":"a","source":"x","context":"","character":""}]}"#,
    )
    .unwrap();
    assert_eq!(t.target_lang, None);
    assert_eq!(t.strings[0].context, None);
    assert_eq!(t.strings[0].character, None);
}

#[test]
fn length_violations_count_code_points() {
    let mut t = sample();
    // 12 code points exactly: allowed.
    t.strings[3].target = Some("Fortfahren!!".into());
    assert!(t.length_violations().is_empty());
    t.strings[3].target = Some("Fortfahren!!!".into());
    let v = t.length_violations();
    assert_eq!(v.len(), 1);
    assert_eq!(
        (v[0].id.as_str(), v[0].max_length, v[0].actual),
        ("ui.button.continue", 12, 13)
    );
}
