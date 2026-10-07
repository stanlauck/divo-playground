// SPDX-License-Identifier: MIT OR Apache-2.0

use pg_01_prose_importer::{import_docx, import_fb2, import_txt, ImportOptions};
use std::{
    hint::black_box,
    io::{self, Cursor, Write},
    time::Instant,
};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

fn main() {
    let repeats: usize = std::env::var("PG01_BENCH_RUNS")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|n| *n > 0)
        .unwrap_or(3);
    let paragraph = "amber cloud circle leaf stone";
    let txt = format!("{paragraph}\n\n").repeat(40_000);
    let fb2 = format!(
        "<FictionBook xmlns=\"http://www.gribuser.ru/xml/fictionbook/2.0\"><body><section>{}</section></body></FictionBook>",
        format!("<p>{paragraph}</p>").repeat(40_000),
    );
    let xml = format!(
        "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>{}</w:body></w:document>",
        format!("<w:p><w:r><w:t>{paragraph}</w:t></w:r></w:p>").repeat(40_000),
    );
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file(
        "word/document.xml",
        SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
    )
    .expect("ZIP entry");
    zip.write_all(xml.as_bytes()).expect("ZIP input");
    let docx = zip.finish().expect("ZIP finish").into_inner();
    for format in ["txt", "fb2", "docx"] {
        let start = Instant::now();
        let mut peak = 0;
        for _ in 0..repeats {
            let report = match format {
                "txt" => import_txt(
                    Cursor::new(black_box(txt.as_bytes())),
                    io::sink(),
                    &ImportOptions::default(),
                ),
                "fb2" => import_fb2(
                    Cursor::new(black_box(fb2.as_bytes())),
                    io::sink(),
                    &ImportOptions::default(),
                ),
                _ => import_docx(
                    Cursor::new(black_box(docx.as_slice())),
                    io::sink(),
                    &ImportOptions::default(),
                ),
            }
            .expect("synthetic benchmark import");
            assert_eq!(report.words, 200_000);
            assert_eq!(report.blocks, 40_000);
            peak = peak.max(report.peak_block_bytes);
        }
        println!(
            "{format}: 200000 words, {repeats} runs, mean_ms={:.3}, peak_block_bytes={peak}",
            start.elapsed().as_secs_f64() * 1000.0 / repeats as f64
        );
    }
}
