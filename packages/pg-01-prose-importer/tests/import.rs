// SPDX-License-Identifier: MIT OR Apache-2.0

use pg_01_prose_importer::{
    import_docx, import_fb2, import_txt, Block, Document, ImportOptions, Span,
};
use std::{
    io::{self, BufReader, Cursor, Read, Write},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

fn fb(body: &str) -> String {
    format!("<FictionBook xmlns=\"http://www.gribuser.ru/xml/fictionbook/2.0\" xmlns:l=\"http://www.w3.org/1999/xlink\">{body}</FictionBook>")
}
fn word(body: &str) -> String {
    format!("<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><w:body>{body}</w:body></w:document>")
}
fn zip(parts: &[(&str, &str)]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, body) in parts {
        writer
            .start_file(
                *name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
            )
            .expect("synthetic ZIP part");
        writer
            .write_all(body.as_bytes())
            .expect("synthetic ZIP text");
    }
    writer.finish().expect("ZIP finish").into_inner()
}
fn read_fb(source: &str) -> Document {
    let mut json = Vec::new();
    let report =
        import_fb2(Cursor::new(source), &mut json, &ImportOptions::default()).expect("FB2 import");
    let document: Document = serde_json::from_slice(&json).expect("complete JSON");
    assert_eq!(report, document.report);
    document
}
fn read_docx(parts: &[(&str, &str)]) -> Document {
    let mut json = Vec::new();
    let report = import_docx(
        Cursor::new(zip(parts)),
        &mut json,
        &ImportOptions::default(),
    )
    .expect("DOCX import");
    let document: Document = serde_json::from_slice(&json).expect("complete JSON");
    assert_eq!(report, document.report);
    document
}
fn sample_docx() -> Document {
    read_docx(&[
        (
            "word/document.xml",
            include_str!("../samples/docx/word/document.xml"),
        ),
        (
            "word/styles.xml",
            include_str!("../samples/docx/word/styles.xml"),
        ),
        (
            "word/numbering.xml",
            include_str!("../samples/docx/word/numbering.xml"),
        ),
        (
            "word/footnotes.xml",
            include_str!("../samples/docx/word/footnotes.xml"),
        ),
        (
            "word/_rels/document.xml.rels",
            include_str!("../samples/docx/word/_rels/document.xml.rels"),
        ),
    ])
}
fn spans(block: &Block) -> &[Span] {
    match block {
        Block::Paragraph { spans } | Block::Heading { spans, .. } => spans,
        _ => panic!("expected text block"),
    }
}
fn text(spans: &[Span]) -> String {
    spans.iter().map(|s| s.text.as_str()).collect()
}
fn losses(document: &Document) -> Vec<&str> {
    document
        .report
        .losses
        .iter()
        .map(|loss| loss.kind.as_str())
        .collect()
}

#[test]
fn txt_preserves_unicode_paragraphs_bom_and_line_breaks() {
    let mut json = Vec::new();
    let source = "\u{feff} first line\r\nСевер 南 عربي\r\n\r\nsecond paragraph\r\n";
    import_txt(Cursor::new(source), &mut json, &ImportOptions::default()).expect("TXT import");
    let document: Document = serde_json::from_slice(&json).expect("JSON");
    assert_eq!(document.chapters.len(), 1);
    assert_eq!(document.chapters[0].blocks.len(), 2);
    assert_eq!(
        text(spans(&document.chapters[0].blocks[0])),
        " first line\nСевер 南 عربي"
    );
    assert!(document.report.losses.is_empty());
}

#[test]
fn txt_empty_document_is_valid_and_invalid_utf8_is_rejected() {
    let mut json = Vec::new();
    import_txt(Cursor::new(" \n\n"), &mut json, &ImportOptions::default()).expect("empty TXT");
    let document: Document = serde_json::from_slice(&json).expect("JSON");
    assert!(document.chapters[0].blocks.is_empty());
    assert_eq!(document.report.words, 0);
    assert!(import_txt(Cursor::new([255]), io::sink(), &ImportOptions::default()).is_err());
}

#[test]
fn fb2_keeps_all_rich_blocks_and_resolves_footnotes() {
    let document = read_fb(include_str!("../samples/synthetic.fb2"));
    let blocks = &document.chapters[0].blocks;
    assert!(matches!(&blocks[0], Block::Heading { level: 1, .. }));
    assert!(blocks.iter().any(|b| matches!(b, Block::Epigraph { .. })));
    assert!(blocks.iter().any(|b| matches!(b, Block::Quote { .. })));
    assert!(blocks.iter().any(|b| matches!(b, Block::Table { .. })));
    let paragraph = blocks
        .iter()
        .find(|b| matches!(b, Block::Paragraph { spans } if spans.iter().any(|s| s.bold)))
        .expect("rich paragraph");
    let rich = spans(paragraph);
    assert!(rich
        .iter()
        .any(|s| s.bold && s.italic && s.text.contains("and italic")));
    assert!(rich.iter().any(|s| s.strike));
    assert!(rich.iter().any(|s| s.superscript));
    assert!(rich.iter().any(|s| s.subscript));
    assert!(rich
        .iter()
        .any(|s| s.url.as_deref() == Some("https://example.invalid/synthetic")));
    assert!(rich
        .iter()
        .any(|s| s.note_id.as_deref() == Some("note-one")));
    let poem = blocks
        .iter()
        .find(|b| matches!(b, Block::Poem { .. }))
        .expect("poem");
    let Block::Poem {
        stanzas,
        attribution,
        ..
    } = poem
    else {
        unreachable!()
    };
    assert_eq!(stanzas.len(), 2);
    assert_eq!(stanzas[0].lines.len(), 2);
    assert_eq!(text(&stanzas[0].lines[0]), "Amber light");
    assert_eq!(text(&stanzas[1].lines[1]), "A separate line");
    assert_eq!(text(attribution), "Synthetic poet");
    assert!(document
        .chapters
        .iter()
        .flat_map(|c| &c.blocks)
        .any(|b| matches!(b, Block::Footnote { id, .. } if id == "note-one")));
    assert!(document.report.unresolved_footnotes.is_empty());
    assert!(losses(&document).contains(&"fb2.images"));
    assert!(losses(&document).contains(&"fb2.binary_resource"));
    assert!(losses(&document).contains(&"fb2.description"));
}

#[test]
fn fb2_nested_sections_keep_reading_order_and_parent_continuations() {
    let document = read_fb(&fb("<body><section id=\"a\"><title><p>Parent</p></title><p>before</p><section id=\"b\"><title><p>Child</p></title><p>inside</p></section><p>after</p></section></body>"));
    assert_eq!(document.chapters.len(), 3);
    assert_eq!(document.chapters[0].level, 1);
    assert_eq!(document.chapters[1].level, 2);
    assert_eq!(document.chapters[2].level, 1);
    assert_eq!(
        document.chapters[2].continuation_of.as_deref(),
        Some("chapter-1")
    );
    assert_eq!(text(spans(&document.chapters[2].blocks[0])), "after");
}

#[test]
fn fb2_keeps_six_heading_levels_without_empty_continuations() {
    let mut body = "<body>".to_string();
    for level in 1..=6 {
        body.push_str(&format!("<section><title><p>Heading {level}</p></title>"));
    }
    body.push_str("<p>deepest text</p>");
    body.push_str(&"</section>".repeat(6));
    body.push_str("</body>");
    let document = read_fb(&fb(&body));
    assert_eq!(document.chapters.len(), 6);
    for (index, chapter) in document.chapters.iter().enumerate() {
        assert_eq!(chapter.level, index as u32 + 1);
        assert!(
            matches!(&chapter.blocks[0], Block::Heading { level, .. } if *level == chapter.level)
        );
    }
}

#[test]
fn fb2_entities_cdata_and_format_boundaries_do_not_change_words() {
    let document = read_fb(&fb("<body><section><p>A&amp;B <![CDATA[<plain>]]> co<strong>oper</strong>ate &#x1F642;</p></section></body>"));
    let value = text(spans(&document.chapters[0].blocks[0]));
    assert_eq!(value, "A&B <plain> cooperate 🙂");
    assert_eq!(document.report.words, 4);
}

#[test]
fn fb2_poem_keeps_stanza_titles_epigraphs_and_blank_verse_lines() {
    let document = read_fb(&fb("<body><section><poem><title><p>Verse</p></title><epigraph><p>Before verse</p></epigraph><stanza><title><p>Part one</p></title><v>first</v><v/><v><emphasis>third</emphasis></v></stanza><date>synthetic date</date></poem></section></body>"));
    let Block::Poem {
        epigraphs, stanzas, ..
    } = &document.chapters[0].blocks[0]
    else {
        panic!("poem")
    };
    assert_eq!(epigraphs.len(), 1);
    assert_eq!(text(&stanzas[0].title), "Part one");
    assert_eq!(stanzas[0].lines.len(), 3);
    assert!(stanzas[0].lines[1].is_empty());
    assert!(stanzas[0].lines[2][0].italic);
    assert!(losses(&document).contains(&"fb2.poem.date"));
}

#[test]
fn fb2_reports_unresolved_notes_and_rejects_duplicate_note_ids() {
    let document = read_fb(&fb(
        "<body><section><p><a type=\"note\" l:href=\"#missing\">1</a></p></section></body>",
    ));
    assert_eq!(document.report.unresolved_footnotes, ["missing"]);
    let source = fb("<body name=\"notes\"><section id=\"same\"><p>one</p></section><section id=\"same\"><p>two</p></section></body>");
    assert!(import_fb2(Cursor::new(source), io::sink(), &ImportOptions::default()).is_err());
}

#[test]
fn fb2_rejects_doctype_malformed_xml_illegal_characters_and_wrong_namespace() {
    for source in [
        "<!DOCTYPE FictionBook [<!ENTITY leak SYSTEM 'file:///synthetic'>]><FictionBook/>"
            .to_string(),
        fb("<body><section><p>unfinished"),
        fb("<body><p>bad&#x1;</p></body>"),
        "<FictionBook xmlns=\"urn:wrong\"><body/></FictionBook>".to_string(),
        fb("<body><p>&missing;</p></body>"),
        fb("<body/>") + &fb("<body/>"),
    ] {
        assert!(import_fb2(Cursor::new(source), io::sink(), &ImportOptions::default()).is_err());
    }
}

#[test]
fn fb2_declared_windows_1251_is_decoded() {
    let mut source = b"<?xml version=\"1.0\" encoding=\"windows-1251\"?><FictionBook xmlns=\"http://www.gribuser.ru/xml/fictionbook/2.0\"><body><section><p>".to_vec();
    source.extend_from_slice(&[0xD2, 0xE5, 0xF1, 0xF2]); // synthetic word: Тест
    source.extend_from_slice(b"</p></section></body></FictionBook>");
    let mut json = Vec::new();
    import_fb2(Cursor::new(source), &mut json, &ImportOptions::default()).expect("Windows-1251");
    let document: Document = serde_json::from_slice(&json).expect("JSON");
    assert_eq!(text(spans(&document.chapters[0].blocks[0])), "Тест");
}

#[test]
fn docx_sample_preserves_runs_lists_table_and_footnote() {
    let document = sample_docx();
    assert_eq!(document.chapters[0].level, 1);
    let blocks = &document.chapters[0].blocks;
    let paragraph = spans(&blocks[1]);
    assert!(paragraph.iter().any(|s| s.bold && s.italic));
    assert!(paragraph.iter().any(|s| s.strike));
    assert!(paragraph.iter().any(|s| s.superscript));
    assert!(paragraph.iter().any(|s| s.subscript));
    assert!(paragraph
        .iter()
        .any(|s| s.url.as_deref() == Some("https://example.invalid/synthetic")));
    assert!(paragraph
        .iter()
        .any(|s| s.note_id.as_deref() == Some("docx-footnote-1")));
    let lists: Vec<_> = blocks
        .iter()
        .filter_map(|b| {
            if let Block::List {
                start,
                ordered,
                items,
                ..
            } = b
            {
                Some((*start, *ordered, items[0].level))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(lists, [(1, true, 0), (1, false, 1), (2, true, 0)]);
    let Block::Table { rows } = blocks
        .iter()
        .find(|b| matches!(b, Block::Table { .. }))
        .expect("table")
    else {
        unreachable!()
    };
    assert_eq!(rows[0].cells[0].colspan, 2);
    assert!(rows[0].cells[0].header);
    assert_eq!(rows[1].cells.len(), 2);
    assert!(blocks.iter().any(|b| matches!(b, Block::Quote { .. })));
    assert!(document
        .chapters
        .iter()
        .flat_map(|c| &c.blocks)
        .any(|b| matches!(b, Block::Footnote { id, .. } if id == "docx-footnote-1")));
    assert!(document.report.unresolved_footnotes.is_empty());
    assert!(losses(&document).contains(&"docx.colors"));
    assert!(losses(&document).contains(&"docx.font_styles"));
    assert!(losses(&document).contains(&"docx.page_layout"));
}

#[test]
fn docx_inherited_styles_direct_overrides_and_heading_levels_are_preserved() {
    let styles = "<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:style w:styleId=\"Base\"><w:rPr><w:b/><w:i/></w:rPr></w:style><w:style w:styleId=\"Chapter\"><w:basedOn w:val=\"Base\"/><w:pPr><w:outlineLvl w:val=\"5\"/></w:pPr></w:style></w:styles>";
    let document_xml = word("<w:p><w:pPr><w:pStyle w:val=\"Chapter\"/></w:pPr><w:r><w:rPr><w:b w:val=\"false\"/></w:rPr><w:t>Heading six</w:t></w:r></w:p>");
    let document = read_docx(&[
        ("word/document.xml", &document_xml),
        ("word/styles.xml", styles),
    ]);
    assert_eq!(document.chapters[0].level, 6);
    let mark = &spans(&document.chapters[0].blocks[0])[0];
    assert!(!mark.bold);
    assert!(mark.italic);
}

#[test]
fn docx_style_cycles_and_missing_list_definitions_are_rejected() {
    let styles = "<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:style w:styleId=\"A\"><w:basedOn w:val=\"B\"/></w:style><w:style w:styleId=\"B\"><w:basedOn w:val=\"A\"/></w:style></w:styles>";
    let document =
        word("<w:p><w:pPr><w:pStyle w:val=\"A\"/></w:pPr><w:r><w:t>text</w:t></w:r></w:p>");
    assert!(import_docx(
        Cursor::new(zip(&[
            ("word/document.xml", &document),
            ("word/styles.xml", styles)
        ])),
        io::sink(),
        &ImportOptions::default()
    )
    .is_err());
    let document = word("<w:p><w:pPr><w:numPr><w:numId w:val=\"99\"/></w:numPr></w:pPr><w:r><w:t>text</w:t></w:r></w:p>");
    assert!(import_docx(
        Cursor::new(zip(&[("word/document.xml", &document)])),
        io::sink(),
        &ImportOptions::default()
    )
    .is_err());
}

#[test]
fn docx_tabs_breaks_hyperlinks_and_explicit_spaces_are_preserved() {
    let document_xml = word("<w:p><w:r><w:t xml:space=\"preserve\"> a </w:t><w:tab/><w:br/><w:t>b&amp;c</w:t></w:r><w:hyperlink w:anchor=\"mark\"><w:r><w:t>link</w:t></w:r></w:hyperlink></w:p>");
    let document = read_docx(&[("word/document.xml", &document_xml)]);
    let mark = spans(&document.chapters[0].blocks[0]);
    assert_eq!(text(mark), " a \t\nb&clink");
    assert_eq!(mark.last().expect("link").url.as_deref(), Some("#mark"));
}

#[test]
fn docx_vertical_merges_keep_text_rows_and_grid_columns() {
    let document_xml = word("<w:tbl><w:tr><w:tc><w:tcPr><w:vMerge w:val=\"restart\"/></w:tcPr><w:p><w:r><w:t>origin</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>right one</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p/></w:tc><w:tc><w:p><w:r><w:t>right two</w:t></w:r></w:p></w:tc></w:tr></w:tbl>");
    let document = read_docx(&[("word/document.xml", &document_xml)]);
    let Block::Table { rows } = &document.chapters[0].blocks[0] else {
        panic!("table")
    };
    assert_eq!(rows[0].cells[0].rowspan, 2);
    assert_eq!(rows[1].cells[0].column, 1);
    assert_eq!(text(spans(&rows[1].cells[0].blocks[0])), "right two");
}

#[test]
fn docx_footnote_hyperlinks_use_their_own_relationship_part() {
    let document_xml =
        word("<w:p><w:r><w:t>text</w:t><w:footnoteReference w:id=\"3\"/></w:r></w:p>");
    let notes = "<w:footnotes xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><w:footnote w:id=\"3\"><w:p><w:hyperlink r:id=\"link\"><w:r><w:t>note link</w:t></w:r></w:hyperlink></w:p></w:footnote></w:footnotes>";
    let rels = "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"link\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink\" Target=\"https://example.invalid/note\" TargetMode=\"External\"/></Relationships>";
    let document = read_docx(&[
        ("word/document.xml", &document_xml),
        ("word/footnotes.xml", notes),
        ("word/_rels/footnotes.xml.rels", rels),
    ]);
    let Block::Footnote { blocks, .. } = &document.chapters[1].blocks[0] else {
        panic!("footnote")
    };
    assert_eq!(
        spans(&blocks[0])[0].url.as_deref(),
        Some("https://example.invalid/note")
    );
}

#[test]
fn docx_skipped_images_colors_underlines_and_headers_are_reported() {
    let document_xml = word("<w:p><w:r><w:rPr><w:u/><w:color w:val=\"ABCDEF\"/><w:rFonts w:ascii=\"SyntheticFont\"/></w:rPr><w:t>kept</w:t><w:drawing/></w:r></w:p>");
    let document = read_docx(&[
        ("word/document.xml", &document_xml),
        ("word/media/synthetic.bin", "synthetic bytes"),
        ("word/header1.xml", "<synthetic/>"),
    ]);
    assert_eq!(text(spans(&document.chapters[0].blocks[0])), "kept");
    for kind in [
        "docx.images",
        "docx.colors",
        "docx.format.u",
        "docx.font_styles",
        "docx.headers_footers",
    ] {
        assert!(
            losses(&document).contains(&kind),
            "missing loss category {kind}"
        );
    }
}

#[test]
fn docx_accepts_strict_ooxml_namespace_and_rejects_wrong_roots() {
    let document = word("<w:p><w:r><w:t>strict text</w:t></w:r></w:p>").replace(
        "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
        "http://purl.oclc.org/ooxml/wordprocessingml/main",
    );
    assert_eq!(
        text(spans(
            &read_docx(&[("word/document.xml", &document)]).chapters[0].blocks[0]
        )),
        "strict text"
    );
    for source in ["<synthetic/>", "<w:document xmlns:w=\"urn:wrong\"/>"] {
        assert!(import_docx(
            Cursor::new(zip(&[("word/document.xml", source)])),
            io::sink(),
            &ImportOptions::default()
        )
        .is_err());
    }
    assert!(import_docx(
        Cursor::new(b"not a zip"),
        io::sink(),
        &ImportOptions::default()
    )
    .is_err());
    assert!(import_docx(
        Cursor::new(zip(&[("unrelated.xml", "<x/>")])),
        io::sink(),
        &ImportOptions::default()
    )
    .is_err());
}

#[test]
fn block_input_metadata_and_depth_limits_are_enforced() {
    let mut options = ImportOptions {
        max_block_bytes: 32,
        ..ImportOptions::default()
    };
    assert!(import_txt(Cursor::new("x".repeat(100)), io::sink(), &options).is_err());
    assert!(import_fb2(
        Cursor::new(fb("<body><section><p>text</p></section></body>")),
        io::sink(),
        &options
    )
    .is_err());
    options = ImportOptions {
        max_input_bytes: 3,
        ..ImportOptions::default()
    };
    assert!(import_txt(Cursor::new("too long"), io::sink(), &options).is_err());
    assert!(import_docx(
        Cursor::new(zip(&[("word/document.xml", &word("<w:p/>"))])),
        io::sink(),
        &options
    )
    .is_err());
    options = ImportOptions {
        max_depth: 2,
        ..ImportOptions::default()
    };
    assert!(import_fb2(
        Cursor::new(fb("<body><section><p>deep</p></section></body>")),
        io::sink(),
        &options
    )
    .is_err());
    options = ImportOptions {
        max_metadata_bytes: 16,
        ..ImportOptions::default()
    };
    assert!(import_docx(
        Cursor::new(zip(&[
            ("word/document.xml", &word("<w:p/>")),
            (
                "word/styles.xml",
                include_str!("../samples/docx/word/styles.xml")
            )
        ])),
        io::sink(),
        &options
    )
    .is_err());
}

#[test]
fn writer_failures_propagate_without_claiming_success() {
    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "synthetic failure",
            ))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    assert!(import_txt(Cursor::new("text"), Broken, &ImportOptions::default()).is_err());
    assert!(import_fb2(
        Cursor::new(fb("<body/>")),
        Broken,
        &ImportOptions::default()
    )
    .is_err());
}

#[test]
fn streaming_emits_blocks_before_the_whole_input_is_consumed() {
    let bytes = Arc::new(AtomicUsize::new(0));
    struct Output(Arc<AtomicUsize>);
    impl Write for Output {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            self.0.fetch_add(data.len(), Ordering::Relaxed);
            Ok(data.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    struct Input {
        cursor: Cursor<Vec<u8>>,
        output: Arc<AtomicUsize>,
        length: usize,
    }
    impl Read for Input {
        fn read(&mut self, data: &mut [u8]) -> io::Result<usize> {
            if self.cursor.position() > (self.length / 2) as u64 {
                assert!(
                    self.output.load(Ordering::Relaxed) > 1000,
                    "blocks must be emitted before EOF"
                );
            }
            let amount = data.len().min(256);
            self.cursor.read(&mut data[..amount])
        }
    }
    let text = "amber cloud circle leaf stone\n\n"
        .repeat(2000)
        .into_bytes();
    let input = Input {
        length: text.len(),
        cursor: Cursor::new(text),
        output: bytes.clone(),
    };
    let report = import_txt(
        BufReader::new(input),
        Output(bytes),
        &ImportOptions::default(),
    )
    .expect("streaming");
    assert_eq!(report.words, 10_000);
    assert!(report.peak_block_bytes < 1024);
}

#[test]
fn synthetic_200000_word_import_is_exact_and_block_buffer_size_stays_small() {
    let paragraph = "amber cloud circle leaf stone";
    let source = fb(&format!(
        "<body><section>{}</section></body>",
        format!("<p>{paragraph}</p>").repeat(40_000)
    ));
    let report = import_fb2(Cursor::new(source), io::sink(), &ImportOptions::default())
        .expect("200k import");
    assert_eq!(report.words, 200_000);
    assert_eq!(report.blocks, 40_000);
    assert!(report.peak_block_bytes < 1024);
}

#[test]
fn output_model_round_trips_with_formatting_and_nested_blocks() {
    for document in [
        read_fb(include_str!("../samples/synthetic.fb2")),
        sample_docx(),
    ] {
        let json = serde_json::to_string(&document).expect("JSON");
        assert_eq!(
            serde_json::from_str::<Document>(&json).expect("round trip"),
            document
        );
    }
}

#[test]
fn fb2_rowspans_reserve_grid_columns_in_following_rows() {
    let document = read_fb(&fb("<body><section><table><tr><td rowspan=\"2\" colspan=\"2\">merged</td><td>right</td></tr><tr><td>below right</td></tr></table></section></body>"));
    let Block::Table { rows } = &document.chapters[0].blocks[0] else {
        panic!("table")
    };
    assert_eq!(rows[0].cells[0].rowspan, 2);
    assert_eq!(rows[1].cells[0].column, 2);
}

#[test]
fn foreign_namespaces_cannot_spoof_inline_formatting_or_attributes() {
    let document = read_fb(&fb("<body><section><p xmlns:e=\"urn:synthetic\"><e:strong>not bold</e:strong><a e:href=\"https://example.invalid/wrong\" l:href=\"https://example.invalid/right\">link</a></p></section></body>"));
    let rich = spans(&document.chapters[0].blocks[0]);
    assert!(!rich[0].bold);
    assert_eq!(
        rich[1].url.as_deref(),
        Some("https://example.invalid/right")
    );
    let source = word("<w:p xmlns:e=\"urn:synthetic\"><w:r><w:rPr><e:b/><w:i/></w:rPr><w:t>not bold</w:t></w:r></w:p>");
    let document = read_docx(&[("word/document.xml", &source)]);
    assert!(!spans(&document.chapters[0].blocks[0])[0].bold);
    assert!(spans(&document.chapters[0].blocks[0])[0].italic);
}

#[test]
fn illegal_attribute_characters_and_unbound_prefixes_are_rejected() {
    for source in [
        fb("<body><section id=\"bad&#x1;\"><p>text</p></section></body>"),
        fb("<body><section><p><unknown:strong>text</unknown:strong></p></section></body>"),
    ] {
        assert!(import_fb2(Cursor::new(source), io::sink(), &ImportOptions::default()).is_err());
    }
}

#[test]
fn xml_token_limit_prevents_unbounded_skipped_resource_buffers() {
    let source = fb(&format!("<body/><binary>{}</binary>", "x".repeat(64_000)));
    let options = ImportOptions {
        max_block_bytes: 1024,
        max_metadata_bytes: 1024,
        ..ImportOptions::default()
    };
    assert!(import_fb2(Cursor::new(source), io::sink(), &options).is_err());
}

#[test]
fn report_metadata_entries_and_bytes_are_bounded() {
    let source = fb("<body><section><p><a type=\"note\" l:href=\"#one\">1</a><a type=\"note\" l:href=\"#two\">2</a></p></section></body>");
    let options = ImportOptions {
        max_metadata_entries: 1,
        ..ImportOptions::default()
    };
    assert!(import_fb2(Cursor::new(&source), io::sink(), &options).is_err());
    let options = ImportOptions {
        max_metadata_bytes: 4,
        ..ImportOptions::default()
    };
    assert!(import_fb2(Cursor::new(source), io::sink(), &options).is_err());
}

#[test]
fn all_zero_limits_are_rejected() {
    let defaults = ImportOptions::default();
    for options in [
        ImportOptions {
            max_input_bytes: 0,
            ..defaults.clone()
        },
        ImportOptions {
            max_block_bytes: 0,
            ..defaults.clone()
        },
        ImportOptions {
            max_metadata_bytes: 0,
            ..defaults.clone()
        },
        ImportOptions {
            max_depth: 0,
            ..defaults.clone()
        },
        ImportOptions {
            max_zip_entries: 0,
            ..defaults.clone()
        },
        ImportOptions {
            max_metadata_entries: 0,
            ..defaults.clone()
        },
    ] {
        assert!(import_txt(Cursor::new("text"), io::sink(), &options).is_err());
    }
}

#[test]
fn docx_zip_entry_limit_duplicate_notes_and_truncated_xml_are_rejected() {
    let source = word("<w:p/>");
    let archive = zip(&[("word/document.xml", &source), ("extra.xml", "<x/>")]);
    let options = ImportOptions {
        max_zip_entries: 1,
        ..ImportOptions::default()
    };
    assert!(import_docx(Cursor::new(archive), io::sink(), &options).is_err());
    let notes = "<w:footnotes xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:footnote w:id=\"1\"><w:p/></w:footnote><w:footnote w:id=\"1\"><w:p/></w:footnote></w:footnotes>";
    assert!(import_docx(
        Cursor::new(zip(&[
            ("word/document.xml", &source),
            ("word/footnotes.xml", notes)
        ])),
        io::sink(),
        &ImportOptions::default()
    )
    .is_err());
    assert!(import_docx(
        Cursor::new(zip(&[("word/document.xml", &word("<w:p>broken"))])),
        io::sink(),
        &ImportOptions::default()
    )
    .is_err());
}

#[test]
fn readers_that_fail_mid_import_propagate_errors() {
    struct Broken {
        remaining: usize,
    }
    impl Read for Broken {
        fn read(&mut self, data: &mut [u8]) -> io::Result<usize> {
            if self.remaining == 0 {
                return Err(io::Error::other("synthetic read failure"));
            }
            let count = data.len().min(self.remaining);
            data[..count].fill(b'a');
            self.remaining -= count;
            Ok(count)
        }
    }
    assert!(import_txt(
        BufReader::new(Broken { remaining: 16 }),
        io::sink(),
        &ImportOptions::default()
    )
    .is_err());
}

#[test]
fn docx_document_defaults_and_default_paragraph_styles_are_used() {
    let styles = "<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:docDefaults><w:rPrDefault><w:rPr><w:i/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type=\"paragraph\" w:default=\"true\" w:styleId=\"Normal\"><w:rPr><w:b/></w:rPr></w:style></w:styles>";
    let source = word("<w:p><w:r><w:t>default text</w:t></w:r></w:p>");
    let document = read_docx(&[("word/document.xml", &source), ("word/styles.xml", styles)]);
    assert!(spans(&document.chapters[0].blocks[0])[0].bold);
    assert!(spans(&document.chapters[0].blocks[0])[0].italic);
}

#[test]
fn docx_style_toggles_combine_but_direct_false_is_an_absolute_override() {
    let styles = "<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:style w:styleId=\"Base\"><w:rPr><w:b/><w:i/></w:rPr></w:style><w:style w:styleId=\"Child\"><w:basedOn w:val=\"Base\"/><w:rPr><w:b/></w:rPr></w:style><w:style w:styleId=\"Character\"><w:rPr><w:i/></w:rPr></w:style></w:styles>";
    let source = word("<w:p><w:pPr><w:pStyle w:val=\"Child\"/></w:pPr><w:r><w:t>plain</w:t></w:r><w:r><w:rPr><w:rStyle w:val=\"Character\"/><w:b/><w:i w:val=\"0\"/></w:rPr><w:t>direct bold</w:t></w:r></w:p>");
    let document = read_docx(&[("word/document.xml", &source), ("word/styles.xml", styles)]);
    let rich = spans(&document.chapters[0].blocks[0]);
    assert!(!rich[0].bold);
    assert!(rich[0].italic);
    assert!(rich[1].bold);
    assert!(!rich[1].italic);
}

#[test]
fn docx_numbering_start_overrides_and_restart_levels_are_retained() {
    let numbering = "<w:numbering xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:abstractNum w:abstractNumId=\"0\"><w:lvl w:ilvl=\"0\"><w:start w:val=\"1\"/><w:numFmt w:val=\"lowerRoman\"/><w:lvlText w:val=\"%1)\"/></w:lvl><w:lvl w:ilvl=\"1\"><w:start w:val=\"3\"/><w:numFmt w:val=\"decimal\"/></w:lvl></w:abstractNum><w:num w:numId=\"5\"><w:abstractNumId w:val=\"0\"/><w:lvlOverride w:ilvl=\"0\"><w:startOverride w:val=\"7\"/></w:lvlOverride></w:num></w:numbering>";
    let mut body = String::new();
    for level in [0, 1, 1, 0, 1] {
        body.push_str(&format!("<w:p><w:pPr><w:numPr><w:ilvl w:val=\"{level}\"/><w:numId w:val=\"5\"/></w:numPr></w:pPr><w:r><w:t>item</w:t></w:r></w:p>"));
    }
    let source = word(&body);
    let document = read_docx(&[
        ("word/document.xml", &source),
        ("word/numbering.xml", numbering),
    ]);
    let starts: Vec<_> = document.chapters[0]
        .blocks
        .iter()
        .map(|block| {
            if let Block::List { start, .. } = block {
                *start
            } else {
                panic!("list")
            }
        })
        .collect();
    assert_eq!(starts, [7, 3, 4, 8, 3]);
    let Block::List {
        number_format,
        marker,
        ..
    } = &document.chapters[0].blocks[0]
    else {
        unreachable!()
    };
    assert_eq!(number_format, "lowerRoman");
    assert_eq!(marker.as_deref(), Some("%1)"));
}

#[test]
fn docx_expanded_numbering_cannot_exceed_metadata_budget() {
    let marker = "x".repeat(1000);
    let mut numbering = format!("<w:numbering xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:abstractNum w:abstractNumId=\"0\"><w:lvl w:ilvl=\"0\"><w:lvlText w:val=\"{marker}\"/></w:lvl></w:abstractNum>");
    for id in 0..10 {
        numbering.push_str(&format!(
            "<w:num w:numId=\"{id}\"><w:abstractNumId w:val=\"0\"/></w:num>"
        ));
    }
    numbering.push_str("</w:numbering>");
    let options = ImportOptions {
        max_metadata_bytes: 8192,
        ..ImportOptions::default()
    };
    let error = import_docx(
        Cursor::new(zip(&[
            ("word/document.xml", &word("<w:p/>")),
            ("word/numbering.xml", &numbering),
        ])),
        io::sink(),
        &options,
    )
    .expect_err("expanded numbering limit");
    assert!(matches!(
        error,
        pg_01_prose_importer::Error::Limit("expanded numbering bytes")
    ));
}

#[test]
fn fb2_and_docx_reject_roots_without_bodies() {
    assert!(import_fb2(Cursor::new(fb("")), io::sink(), &ImportOptions::default()).is_err());
    let source =
        "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"/>";
    assert!(import_docx(
        Cursor::new(zip(&[("word/document.xml", source)])),
        io::sink(),
        &ImportOptions::default()
    )
    .is_err());
}

#[test]
fn fb2_streams_paragraphs_before_reading_the_rest_of_the_document() {
    let emitted = Arc::new(AtomicUsize::new(0));
    struct Output(Arc<AtomicUsize>);
    impl Write for Output {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.fetch_add(bytes.len(), Ordering::Relaxed);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    struct Input {
        cursor: Cursor<Vec<u8>>,
        emitted: Arc<AtomicUsize>,
        length: usize,
    }
    impl Read for Input {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            if self.cursor.position() > (self.length / 2) as u64 {
                assert!(self.emitted.load(Ordering::Relaxed) > 1000);
            }
            let count = bytes.len().min(256);
            self.cursor.read(&mut bytes[..count])
        }
    }
    let bytes = fb(&format!(
        "<body><section>{}</section></body>",
        "<p>amber cloud circle leaf stone</p>".repeat(2000)
    ))
    .into_bytes();
    let input = Input {
        length: bytes.len(),
        cursor: Cursor::new(bytes),
        emitted: emitted.clone(),
    };
    let report = import_fb2(
        BufReader::new(input),
        Output(emitted),
        &ImportOptions::default(),
    )
    .expect("streaming FB2");
    assert_eq!(report.words, 10_000);
    assert!(report.peak_block_bytes < 1024);
}

#[test]
fn docx_compressed_parts_cannot_bypass_uncompressed_input_limits() {
    let source = word(&format!(
        "<w:p><w:r><w:t>{}</w:t></w:r></w:p>",
        "x".repeat(16_000)
    ));
    let bytes = zip(&[("word/document.xml", &source)]);
    assert!(bytes.len() < 4096);
    let options = ImportOptions {
        max_input_bytes: 4096,
        ..ImportOptions::default()
    };
    assert!(matches!(
        import_docx(Cursor::new(bytes), io::sink(), &options),
        Err(pg_01_prose_importer::Error::Limit("DOCX document bytes"))
    ));
}

#[test]
fn large_hyperlink_targets_cannot_amplify_small_blocks_without_a_bound() {
    let target = format!("https://example.invalid/{}", "x".repeat(16_000));
    let options = ImportOptions {
        max_block_bytes: 64_000,
        ..ImportOptions::default()
    };
    let source = fb(&format!(
        "<body><section><p><a l:href=\"{target}\">{}</a></p></section></body>",
        "<strong>x</strong><emphasis>y</emphasis>".repeat(20)
    ));
    assert!(matches!(
        import_fb2(Cursor::new(source), io::sink(), &options),
        Err(pg_01_prose_importer::Error::Limit("expanded link bytes"))
    ));
    let source = word(&format!(
        "<w:p><w:hyperlink r:id=\"link\">{}</w:hyperlink></w:p>",
        "<w:r><w:t>x</w:t></w:r>".repeat(20)
    ));
    let links = format!("<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"link\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink\" Target=\"{target}\" TargetMode=\"External\"/></Relationships>");
    assert!(matches!(
        import_docx(
            Cursor::new(zip(&[
                ("word/document.xml", &source),
                ("word/_rels/document.xml.rels", &links)
            ])),
            io::sink(),
            &options
        ),
        Err(pg_01_prose_importer::Error::Limit("expanded link bytes"))
    ));
}
