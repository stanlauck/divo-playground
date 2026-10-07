// SPDX-License-Identifier: MIT OR Apache-2.0

use pg_01_prose_importer::{import_docx, import_fb2, import_txt, ImportOptions};
use std::{
    fs::File,
    io::{self, BufReader, BufWriter, Write},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let format = args
        .next()
        .ok_or("usage: import <fb2|docx|txt> <input file>")?;
    let path = args
        .next()
        .ok_or("usage: import <fb2|docx|txt> <input file>")?;
    let input = File::open(path)?;
    let mut output = BufWriter::new(io::stdout().lock());
    let options = ImportOptions::default();
    match format.to_str() {
        Some("fb2") => {
            import_fb2(BufReader::new(input), &mut output, &options)?;
        }
        Some("docx") => {
            import_docx(input, &mut output, &options)?;
        }
        Some("txt") => {
            import_txt(BufReader::new(input), &mut output, &options)?;
        }
        _ => return Err("format must be fb2, docx or txt".into()),
    }
    output.flush()?;
    Ok(())
}
