// SPDX-License-Identifier: MIT OR Apache-2.0

use std::path::Path;
use std::process::ExitCode;

use gltf_mockup::{WriteOptions, from_json, to_glb, to_gltf};

const USAGE: &str = "usage: gltf-mockup <input.json> <output.gltf|output.glb> [--no-markers] [--marker-depth <metres>]";

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("gltf-mockup: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    let mut options = WriteOptions::default();
    let mut paths = Vec::new();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            "--no-markers" => options.markers = false,
            "--marker-depth" => {
                let value = args.next().ok_or("--marker-depth needs a value")?;
                options.marker_depth_m = value
                    .parse()
                    .map_err(|_| format!("--marker-depth: not a number: {value}"))?;
            }
            _ if arg.starts_with("--") => return Err(format!("unknown option {arg}\n{USAGE}")),
            _ => paths.push(arg),
        }
    }
    let [input, output] = <[String; 2]>::try_from(paths).map_err(|_| USAGE.to_owned())?;

    let json = std::fs::read_to_string(&input).map_err(|e| format!("{input}: {e}"))?;
    let scene = from_json(&json).map_err(|e| format!("{input}: {e}"))?;
    let extension = Path::new(&output)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    let bytes = match extension.as_deref() {
        Some("gltf") => to_gltf(&scene, &options).map(String::into_bytes),
        Some("glb") => to_glb(&scene, &options),
        _ => return Err(format!("{output}: output must end in .gltf or .glb")),
    }
    .map_err(|e| e.to_string())?;
    std::fs::write(&output, bytes).map_err(|e| format!("{output}: {e}"))
}
