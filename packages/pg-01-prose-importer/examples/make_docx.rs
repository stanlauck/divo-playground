// SPDX-License-Identifier: MIT OR Apache-2.0

use std::{fs::OpenOptions, io::Write};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("usage: make_docx <new output.docx>")?;
    let output = OpenOptions::new().write(true).create_new(true).open(path)?;
    let mut zip = ZipWriter::new(output);
    let content_types = r#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
<Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/>
<Override PartName="/word/footnotes.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml"/>
</Types>"#;
    let root_rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="document" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let document_rels = include_str!("../samples/docx/word/_rels/document.xml.rels").replace(
        "</Relationships>",
        r#"<Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
<Relationship Id="numbering" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/>
<Relationship Id="footnotes" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/>
</Relationships>"#,
    );
    for (name, contents) in [
        ("[Content_Types].xml", content_types),
        ("_rels/.rels", root_rels),
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
        ("word/_rels/document.xml.rels", document_rels.as_str()),
    ] {
        zip.start_file(
            name,
            SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
        )?;
        zip.write_all(contents.as_bytes())?;
    }
    zip.finish()?;
    Ok(())
}
