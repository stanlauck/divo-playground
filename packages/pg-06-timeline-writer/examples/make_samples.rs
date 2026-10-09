// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fs;
use std::path::Path;
use timeline_writer::{Result, from_json, to_fcpxml, to_otio};

fn main() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let list = from_json(include_str!("../samples/synthetic.json"))?;
    fs::write(root.join("samples/synthetic.otio"), to_otio(&list)?)?;
    fs::write(root.join("samples/synthetic.fcpxml"), to_fcpxml(&list)?)?;
    Ok(())
}
