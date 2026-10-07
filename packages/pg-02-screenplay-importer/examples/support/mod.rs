// SPDX-License-Identifier: MIT OR Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
};

#[derive(Clone)]
pub struct TextLine {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub size: f64,
    pub matrix: [f64; 4],
    pub render_mode: u8,
}

impl TextLine {
    pub fn new(text: &str, x: f64, y: f64) -> Self {
        Self {
            text: text.into(),
            x,
            y,
            size: 12.0,
            matrix: [1.0, 0.0, 0.0, 1.0],
            render_mode: 0,
        }
    }
}

#[derive(Default)]
pub struct SamplePage {
    pub lines: Vec<TextLine>,
    pub rotation: u16,
    pub crop: Option<[f64; 4]>,
    pub user_unit: Option<f64>,
    pub omit_unicode: bool,
    pub image: bool,
    pub extra_content: String,
    pub no_content: bool,
}

/// A tiny synthetic Type3 font with box glyphs and an explicit Unicode mapping.
/// It contains no copied font program, artwork or story text.
pub fn pdf(pages: &[SamplePage]) -> Vec<u8> {
    let characters: BTreeSet<_> = pages
        .iter()
        .flat_map(|page| page.lines.iter())
        .flat_map(|line| line.text.chars())
        .collect();
    assert!(characters.len() <= 255);
    let codes: BTreeMap<_, _> = characters
        .iter()
        .copied()
        .enumerate()
        .map(|(index, character)| (character, index + 1))
        .collect();
    let mut cmap = String::from("/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Synthetic) /Ordering (Synthetic) /Supplement 0 >> def\n/CMapName /Synthetic def\n/CMapType 2 def\n1 begincodespacerange\n<01> <FF>\nendcodespacerange\n");
    for chunk in characters.iter().copied().collect::<Vec<_>>().chunks(100) {
        writeln!(cmap, "{} beginbfchar", chunk.len()).unwrap();
        for character in chunk {
            let mut buffer = [0u16; 2];
            let unicode: String = character
                .encode_utf16(&mut buffer)
                .iter()
                .map(|unit| format!("{unit:04X}"))
                .collect();
            writeln!(cmap, "<{:02X}> <{unicode}>", codes[character]).unwrap();
        }
        cmap.push_str("endbfchar\n");
    }
    cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    let count = characters.len().max(1);
    let names = (1..=count)
        .map(|code| format!("/g{code}"))
        .collect::<Vec<_>>()
        .join(" ");
    let procs = (1..=count)
        .map(|code| format!("/g{code} 5 0 R"))
        .collect::<Vec<_>>()
        .join(" ");
    let widths = vec!["600"; count].join(" ");
    let kids = (0..pages.len())
        .map(|index| format!("{} 0 R", 6 + index * 2))
        .collect::<Vec<_>>()
        .join(" ");
    let unicode = if pages.iter().any(|page| page.omit_unicode) {
        String::new()
    } else {
        "/ToUnicode 4 0 R".into()
    };
    let mut objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        format!("<< /Type /Pages /Kids [{kids}] /Count {} >>", pages.len()).into_bytes(),
        format!("<< /Type /Font /Subtype /Type3 /FontBBox [0 -200 600 800] /FontMatrix [0.001 0 0 0.001 0 0] /CharProcs << {procs} >> /Encoding << /Type /Encoding /Differences [1 {names}] >> /FirstChar 1 /LastChar {count} /Widths [{widths}] {unicode} /Resources << >> >>").into_bytes(),
        stream(cmap.as_bytes()),
        stream(b"600 0 0 -200 600 800 d1\n0 0 500 700 re f\n"),
    ];
    for (index, page) in pages.iter().enumerate() {
        let crop = page
            .crop
            .map(|crop| format!("/CropBox [{} {} {} {}]", crop[0], crop[1], crop[2], crop[3]))
            .unwrap_or_default();
        let crop = page
            .user_unit
            .map(|unit| format!("{crop} /UserUnit {unit}"))
            .unwrap_or(crop);
        let contents = if page.no_content {
            String::new()
        } else {
            format!("/Contents {} 0 R", 7 + index * 2)
        };
        objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] {crop} /Rotate {} /Resources << /Font << /F1 3 0 R >> >> {contents} >>", page.rotation).into_bytes());
        let mut content = String::new();
        for line in &page.lines {
            let text: String = line
                .text
                .chars()
                .map(|character| format!("{:02X}", codes[&character]))
                .collect();
            writeln!(
                content,
                "BT /F1 {} Tf {} Tr {} {} {} {} {} {} Tm <{text}> Tj ET",
                line.size,
                line.render_mode,
                line.matrix[0],
                line.matrix[1],
                line.matrix[2],
                line.matrix[3],
                line.x,
                line.y
            )
            .unwrap();
        }
        content.push_str(&page.extra_content);
        let mut content = content.into_bytes();
        if page.image {
            content.extend_from_slice(
                b"\nq 100 0 0 100 72 500 cm BI /W 1 /H 1 /CS /RGB /BPC 8 ID \x70\x50\x30 EI Q\n",
            );
        }
        objects.push(stream(&content));
    }
    serialize(&objects)
}

pub fn stream(content: &[u8]) -> Vec<u8> {
    let mut stream = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
    stream.extend_from_slice(content);
    stream.extend_from_slice(b"\nendstream");
    stream
}

pub fn serialize(objects: &[Vec<u8>]) -> Vec<u8> {
    let mut output =
        b"%PDF-1.7\n% SPDX-License-Identifier: MIT OR Apache-2.0\n%synthetic\n".to_vec();
    let mut offsets = vec![0];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(output.len());
        output.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        output.extend_from_slice(object);
        output.extend_from_slice(b"\nendobj\n");
    }
    let xref = output.len();
    output
        .extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes());
    for offset in &offsets[1..] {
        output.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    output.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            offsets.len()
        )
        .as_bytes(),
    );
    output
}

pub fn sample() -> Vec<u8> {
    pdf(&[
        SamplePage {
            lines: vec![
                TextLine::new("1 INT. AMBER ROOM - DAY", 72.0, 720.0),
                TextLine::new("Iris places a blue cube on a table.", 72.0, 690.0),
                TextLine::new("IRIS", 252.0, 654.0),
                TextLine::new("(quietly)", 180.0, 636.0),
                TextLine::new("The sample light is green.", 144.0, 618.0),
                TextLine::new("CUT TO:", 432.0, 570.0),
                TextLine::new("UNCERTAIN MARKER", 72.0, 534.0),
            ],
            ..SamplePage::default()
        },
        SamplePage {
            lines: vec![
                TextLine::new("2 НАТ. СИНТЕТИЧЕСКИЙ ДВОР - ДЕНЬ", 72.0, 720.0),
                TextLine::new("Ирис переставляет учебный куб.", 72.0, 690.0),
                TextLine::new("ИРИС", 252.0, 654.0),
                TextLine::new("(тихо)", 180.0, 636.0),
                TextLine::new("Это только вымышленный пример.", 144.0, 618.0),
                TextLine::new("ЗАТЕМНЕНИЕ.", 432.0, 570.0),
                TextLine::new("2.", 540.0, 30.0),
            ],
            ..SamplePage::default()
        },
    ])
}

#[allow(dead_code)]
pub fn form_pdf(depth: usize, repeats: usize, cycle: bool) -> Vec<u8> {
    assert!(depth > 0);
    let mut objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> /XObject << /Fm 6 0 R >> >> /Contents 5 0 R >>".to_vec(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>".to_vec(),
        stream(b"q 1 0 0 1 72 700 cm /Fm Do Q"),
    ];
    for index in 0..depth {
        let last = index + 1 == depth;
        let content = if last && !cycle {
            "BT /F1 12 Tf 1 0 0 1 0 0 Tm (Synthetic form text.) Tj ET".to_owned()
        } else {
            "/Next Do\n".repeat(repeats)
        };
        let next = if last { 6 } else { 7 + index };
        let resources = if last && !cycle {
            String::new()
        } else {
            format!("/Resources << /XObject << /Next {next} 0 R >> >>")
        };
        objects.push(format!("<< /Type /XObject /Subtype /Form /BBox [0 0 500 500] {resources} /Length {} >>\nstream\n{content}\nendstream", content.len()).into_bytes());
    }
    serialize(&objects)
}
