// SPDX-License-Identifier: MIT OR Apache-2.0
use ink_reader::{read_compiled, ReadOptions};
use std::{
    env,
    fs::File,
    io::{self, Read, Write},
};
fn main() {
    let mut args = env::args().skip(1);
    let Some(input) = args.next() else {
        eprintln!(
            "usage: ink2graph <story.ink.json> [-o graph.json] [--report report.json] [--strict]"
        );
        std::process::exit(2)
    };
    let mut out = None;
    let mut report = None;
    let mut strict = false;
    while let Some(a) = args.next() {
        match a.as_str() {
            "-o" => out = args.next(),
            "--report" => report = args.next(),
            "--strict" => strict = true,
            _ => {
                eprintln!("unknown argument: {a}");
                std::process::exit(2)
            }
        }
    }
    let mut f = File::open(&input).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    let mut bytes = Vec::new();
    f.read_to_end(&mut bytes).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    let o = ReadOptions {
        package_name: input
            .rsplit('/')
            .next()
            .unwrap_or(&input)
            .trim_end_matches(".json")
            .into(),
        ..ReadOptions::default()
    };
    let (graph, rep) = read_compiled(io::Cursor::new(bytes), &o).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    let encoded = serde_json::to_string_pretty(&graph).unwrap();
    if let Some(path) = out {
        File::create(path)
            .and_then(|mut f| f.write_all(encoded.as_bytes()))
            .unwrap();
    } else {
        println!("{encoded}");
    }
    if let Some(path) = report {
        let s = serde_json::to_string_pretty(&rep).unwrap();
        File::create(path)
            .and_then(|mut f| f.write_all(s.as_bytes()))
            .unwrap();
    }
    if strict && (!rep.unsupported.is_empty() || !rep.warnings.is_empty()) {
        std::process::exit(1)
    }
}
