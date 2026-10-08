// SPDX-License-Identifier: MIT OR Apache-2.0

use pg_02_screenplay_importer::{import_pdf, ImportOptions};
use std::{
    fs::File,
    io::{self, BufReader, BufWriter, Write},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let path = arguments.next().ok_or("usage: import INPUT.pdf")?;
    if arguments.next().is_some() {
        return Err("usage: import INPUT.pdf".into());
    }
    let screenplay = import_pdf(BufReader::new(File::open(path)?), &ImportOptions::default())?;
    let mut output = BufWriter::new(io::stdout().lock());
    serde_json::to_writer_pretty(&mut output, &screenplay)?;
    output.write_all(b"\n")?;
    output.flush()?;
    Ok(())
}
