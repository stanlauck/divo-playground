// SPDX-License-Identifier: MIT OR Apache-2.0

use subtitle_writer::{
    escape_text, format_timecode, write, write_srt, write_ttml, write_vtt, Cue, Document, Error,
    Format,
};

fn doc(cues: Vec<Cue>) -> Document {
    let mut d = Document::new();
    d.cues = cues;
    d
}
fn spoken(id: &str, speaker: &str, start: u64, end: u64, lines: &[&str]) -> Cue {
    let mut cue = Cue::new(id, start, end, lines);
    cue.speaker = Some(speaker.to_owned());
    cue
}

#[test]
fn timecodes_use_two_digit_hours_and_zero_padding() {
    assert_eq!(format_timecode(0, ','), "00:00:00,000");
    assert_eq!(format_timecode(1, '.'), "00:00:00.001");
    assert_eq!(format_timecode(59_999, ','), "00:00:59,999");
    assert_eq!(format_timecode(60_000, '.'), "00:01:00.000");
    assert_eq!(format_timecode(3_599_999, ','), "00:59:59,999");
    assert_eq!(format_timecode(3_600_000, '.'), "01:00:00.000");
    assert_eq!(format_timecode(359_999_999, ','), "99:59:59,999");
}

#[test]
fn srt_numbers_cues_and_prefixes_speaker_on_first_line_only() {
    let d = doc(vec![
        spoken("a", "ANNA", 0, 1500, &["One.", "Two."]),
        Cue::new("b", 2000, 3000, &["Three."]),
    ]);
    assert_eq!(
        write_srt(&d).unwrap(),
        "1\n00:00:00,000 --> 00:00:01,500\nANNA: One.\nTwo.\n\n2\n00:00:02,000 --> 00:00:03,000\nThree.\n"
    );
}

#[test]
fn srt_does_not_escape_markup_characters() {
    let d = doc(vec![spoken("a", "A&B", 0, 1500, &["<i>x</i> & y > z"])]);
    assert!(write_srt(&d).unwrap().contains("A&B: <i>x</i> & y > z\n"));
}

#[test]
fn vtt_starts_with_header_and_uses_voice_tags() {
    let d = doc(vec![
        spoken("intro", "ANNA", 0, 1500, &["One.", "Two."]),
        Cue::new("second", 2000, 3000, &["Three."]),
    ]);
    assert_eq!(
        write_vtt(&d).unwrap(),
        "WEBVTT\n\nintro\n00:00:00.000 --> 00:00:01.500\n<v ANNA>One.\nTwo.\n\nsecond\n00:00:02.000 --> 00:00:03.000\nThree.\n"
    );
}

#[test]
fn vtt_escapes_payload_and_speaker() {
    let d = doc(vec![spoken("a", "A&B", 0, 1500, &["<i>x</i> & y > z"])]);
    let vtt = write_vtt(&d).unwrap();
    assert!(vtt.contains("<v A&amp;B>&lt;i&gt;x&lt;/i&gt; &amp; y &gt; z\n"));
}

#[test]
fn escape_text_handles_only_the_three_markup_characters() {
    assert_eq!(escape_text("a&b<c>d\"e'f"), "a&amp;b&lt;c&gt;d\"e'f");
    assert_eq!(escape_text("плain"), "плain");
}

#[test]
fn ttml_structure_agents_and_line_breaks() {
    let mut d = doc(vec![
        spoken("a", "ANNA", 0, 1500, &["One.", "Two."]),
        Cue::new("b", 2000, 3000, &["Three."]),
        spoken("c", "ANNA", 3000, 4000, &["Four."]),
        spoken("d", "BOB", 4000, 5000, &["Five."]),
    ]);
    d.title = Some("T & <x>".into());
    d.language = Some("de".into());
    let ttml = write_ttml(&d).unwrap();
    assert!(ttml.starts_with(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<tt xmlns=\"http://www.w3.org/ns/ttml\""
    ));
    assert!(ttml.contains("xml:lang=\"de\" ttp:timeBase=\"media\">"));
    assert!(ttml.contains("<ttm:title>T &amp; &lt;x&gt;</ttm:title>"));
    assert_eq!(
        ttml.matches("<ttm:agent ").count(),
        2,
        "one agent per distinct speaker"
    );
    assert!(ttml.contains("xml:id=\"agent1\"><ttm:name type=\"full\">ANNA</ttm:name>"));
    assert!(ttml.contains("xml:id=\"agent2\"><ttm:name type=\"full\">BOB</ttm:name>"));
    assert!(ttml.contains(
        "<p xml:id=\"a\" begin=\"00:00:00.000\" end=\"00:00:01.500\" ttm:agent=\"agent1\">One.<br/>Two.</p>"
    ));
    assert!(ttml.contains("<p xml:id=\"b\" begin=\"00:00:02.000\" end=\"00:00:03.000\">Three.</p>"));
    assert!(ttml.contains("<p xml:id=\"c\" begin=\"00:00:03.000\" end=\"00:00:04.000\" ttm:agent=\"agent1\">Four.</p>"));
    assert!(ttml.contains("<p xml:id=\"d\" begin=\"00:00:04.000\" end=\"00:00:05.000\" ttm:agent=\"agent2\">Five.</p>"));
    assert!(ttml.ends_with("</div>\n  </body>\n</tt>\n"));
}

#[test]
fn ttml_escapes_text_and_defaults_language_to_und() {
    let d = doc(vec![spoken("a", "A&B", 0, 1500, &["<i>x</i> & y > z"])]);
    let ttml = write_ttml(&d).unwrap();
    assert!(ttml.contains("xml:lang=\"und\""));
    assert!(ttml.contains("<ttm:name type=\"full\">A&amp;B</ttm:name>"));
    assert!(ttml.contains(">&lt;i&gt;x&lt;/i&gt; &amp; y &gt; z</p>"));
    assert!(!ttml.contains("<ttm:title>"));
}

#[test]
fn ttml_falls_back_to_generated_ids_when_cue_ids_are_not_ncnames_or_not_unique() {
    let d = doc(vec![
        Cue::new("1", 0, 1000, &["x"]),
        Cue::new("ok", 1000, 2000, &["y"]),
    ]);
    let ttml = write_ttml(&d).unwrap();
    assert!(ttml.contains("<p xml:id=\"c1\" begin"));
    assert!(ttml.contains("<p xml:id=\"c2\" begin"));
    let d = doc(vec![
        Cue::new("dup", 0, 1000, &["x"]),
        Cue::new("dup", 1000, 2000, &["y"]),
    ]);
    let ttml = write_ttml(&d).unwrap();
    assert!(ttml.contains("<p xml:id=\"c1\" begin") && ttml.contains("<p xml:id=\"c2\" begin"));
    let d = doc(vec![Cue::new("has space", 0, 1000, &["x"])]);
    assert!(write_ttml(&d).unwrap().contains("<p xml:id=\"c1\" begin"));
    let d = doc(vec![Cue::new("Ok_1.2-3", 0, 1000, &["x"])]);
    assert!(write_ttml(&d)
        .unwrap()
        .contains("<p xml:id=\"Ok_1.2-3\" begin"));
}

#[test]
fn every_format_is_lf_only_without_bom_and_vtt_starts_with_webvtt() {
    let d = doc(vec![spoken("a", "ANNA", 0, 1500, &["One.", "Two."])]);
    for format in [Format::Srt, Format::Vtt, Format::Ttml] {
        let text = write(&d, format).unwrap();
        assert!(!text.contains('\r'));
        assert!(!text.starts_with('\u{feff}'));
        assert!(text.ends_with('\n'));
        assert_eq!(write(&d, format).unwrap(), text, "deterministic");
    }
    assert!(write(&d, Format::Vtt).unwrap().starts_with("WEBVTT\n"));
}

#[test]
fn empty_document_writes_minimal_outputs() {
    let d = Document::new();
    assert_eq!(write_srt(&d).unwrap(), "");
    assert_eq!(write_vtt(&d).unwrap(), "WEBVTT\n");
    let ttml = write_ttml(&d).unwrap();
    assert!(ttml.contains("<div>\n    </div>"));
}

#[test]
fn writers_refuse_cues_without_a_presentation_interval_or_with_empty_lines() {
    for format in [Format::Srt, Format::Vtt, Format::Ttml] {
        let d = doc(vec![Cue::new("a", 1000, 1000, &["x"])]);
        assert!(matches!(
            write(&d, format),
            Err(Error::Invalid { path, .. }) if path == "cues[0].end_ms"
        ));
        let d = doc(vec![Cue::new("a", 0, 1000, &["x", "   "])]);
        assert!(matches!(
            write(&d, format),
            Err(Error::Invalid { path, .. }) if path == "cues[0].lines[1]"
        ));
        let mut d = doc(vec![Cue::new("a", 0, 1000, &["x"])]);
        d.version = 2;
        assert!(matches!(write(&d, format), Err(Error::Invalid { path, .. }) if path == "version"));
    }
}

#[test]
fn format_names_parse_case_insensitively() {
    assert_eq!(Format::parse("SRT"), Some(Format::Srt));
    assert_eq!(Format::parse("vtt"), Some(Format::Vtt));
    assert_eq!(Format::parse("WebVTT"), Some(Format::Vtt));
    assert_eq!(Format::parse("ttml"), Some(Format::Ttml));
    assert_eq!(Format::parse("ass"), None);
    assert_eq!(Format::Ttml.extension(), "ttml");
}
