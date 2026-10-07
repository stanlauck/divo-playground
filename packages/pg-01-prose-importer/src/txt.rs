// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::{
    emit::Engine, xml::Limited, Block, Error, ImportOptions, ImportReport, Result, SourceFormat,
    Span,
};
use std::io::{BufRead, BufReader, Read, Write};

/// Import UTF-8 TXT. Blank lines separate paragraphs; line breaks within a
/// paragraph are preserved. A leading UTF-8 BOM is removed.
pub fn import_txt<R: BufRead, W: Write>(
    reader: R,
    writer: W,
    options: &ImportOptions,
) -> Result<ImportReport> {
    let mut engine = Engine::new(writer, SourceFormat::Txt, options)?;
    let mut reader = BufReader::new(Limited::new(reader, options.max_input_bytes));
    let mut paragraph = String::new();
    let mut first = true;
    loop {
        let mut bytes = Vec::new();
        let count = reader
            .by_ref()
            .take((options.max_block_bytes as u64).saturating_add(1))
            .read_until(b'\n', &mut bytes)?;
        if count == 0 {
            break;
        }
        if bytes.len() > options.max_block_bytes {
            return Err(Error::Limit("TXT line bytes"));
        }
        let mut line =
            std::str::from_utf8(&bytes).map_err(|_| Error::Invalid("TXT must be UTF-8"))?;
        if first {
            line = line.strip_prefix('\u{feff}').unwrap_or(line);
            first = false;
        }
        line = line.strip_suffix('\n').unwrap_or(line);
        line = line.strip_suffix('\r').unwrap_or(line);
        if line.trim().is_empty() {
            flush(&mut paragraph, &mut engine)?;
        } else {
            if !paragraph.is_empty() {
                paragraph.push('\n');
            }
            paragraph.push_str(line);
            engine.observe(paragraph.len())?;
        }
    }
    flush(&mut paragraph, &mut engine)?;
    engine.finish()
}

fn flush<W: Write>(paragraph: &mut String, engine: &mut Engine<W>) -> Result<()> {
    if !paragraph.is_empty() {
        engine.block(Block::Paragraph {
            spans: vec![Span {
                text: std::mem::take(paragraph),
                ..Span::default()
            }],
        })?;
    }
    Ok(())
}
