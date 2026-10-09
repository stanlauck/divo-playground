// SPDX-License-Identifier: MIT OR Apache-2.0

use edl_exchange::{from_json, to_edl};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for name in ["synthetic", "drop-frame"] {
        let input = std::fs::read_to_string(root.join(format!("samples/{name}.json")))?;
        std::fs::write(
            root.join(format!("samples/{name}.edl")),
            to_edl(&from_json(&input)?)?,
        )?;
    }
    Ok(())
}
