// SPDX-License-Identifier: MIT OR Apache-2.0

mod support;

use std::{
    fs::OpenOptions,
    io::{BufWriter, Write},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let path = arguments.next().ok_or("usage: make_sample OUTPUT.pdf")?;
    if arguments.next().is_some() {
        return Err("usage: make_sample OUTPUT.pdf".into());
    }
    let file = OpenOptions::new().write(true).create_new(true).open(path)?;
    let mut output = BufWriter::new(file);
    output.write_all(&support::sample())?;
    output.flush()?;
    Ok(())
}
