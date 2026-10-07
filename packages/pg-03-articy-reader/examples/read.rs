// SPDX-License-Identifier: MIT OR Apache-2.0

use pg_03_articy_reader::{read_export, ImportOptions};
use std::{
    fs::File,
    io::{self, BufReader, BufWriter, Write},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let input = arguments
        .next()
        .ok_or("usage: read <export.json> [--strict]")?;
    let strict = match arguments.next().as_deref() {
        None => false,
        Some(value) if value == "--strict" => true,
        _ => return Err("usage: read <export.json> [--strict]".into()),
    };
    if arguments.next().is_some() {
        return Err("usage: read <export.json> [--strict]".into());
    }
    let graph = read_export(
        BufReader::new(File::open(input)?),
        &ImportOptions {
            strict_references: strict,
            ..ImportOptions::default()
        },
    )?;
    let mut output = BufWriter::new(io::stdout().lock());
    serde_json::to_writer_pretty(&mut output, &graph)?;
    output.write_all(b"\n")?;
    output.flush()?;
    Ok(())
}
