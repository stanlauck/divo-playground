// SPDX-License-Identifier: MIT OR Apache-2.0

use pg_13_gexf_writer::Graph;
use std::io;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let graph: Graph = match std::env::args_os().nth(1) {
        Some(path) => serde_json::from_reader(std::fs::File::open(path)?)?,
        None => serde_json::from_str(include_str!("../samples/typed_dynamic_graph.json"))?,
    };
    graph.write_to(io::stdout().lock())?;
    Ok(())
}
