// SPDX-License-Identifier: MIT OR Apache-2.0

//! Parse Yarn Spinner 2 `.yarn` scripts into a neutral dialogue graph.

#![doc = include_str!("../README.md")]

mod error;
mod lex;
mod model;
mod parse;

pub use error::{Error, Result};
pub use model::*;
pub use parse::{parse, parse_sources, ParseOptions, Source};

use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Reads `.yarn` files from disk and parses them into one graph.
///
/// Each file is recorded under the path exactly as given, so reports never
/// contain a resolved absolute path. A UTF-8 byte-order mark is ignored.
pub fn read_files(paths: &[impl AsRef<Path>], options: &ParseOptions) -> Result<DialogueGraph> {
    let mut sources = Vec::with_capacity(paths.len());
    for path in paths {
        let path = path.as_ref();
        let name = path.to_string_lossy().into_owned();
        let text = read_bounded(path, &name, options)?;
        sources.push(Source { name, text });
    }
    parse_sources(&sources, options)
}

fn read_bounded(path: &Path, name: &str, options: &ParseOptions) -> Result<String> {
    let unreadable = |_| Error::Io {
        source: name.to_string(),
    };
    let size = std::fs::metadata(path).map_err(unreadable)?.len();
    let limit = u64::try_from(options.max_input_bytes).unwrap_or(u64::MAX);
    if size > limit {
        return Err(Error::TooLarge {
            source: name.to_string(),
            bytes: size,
            limit,
        });
    }
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|mut file| file.read_to_end(&mut bytes))
        .map_err(unreadable)?;
    String::from_utf8(bytes).map_err(|_| Error::NotUtf8 {
        source: name.to_string(),
    })
}

/// Serializes a graph as pretty-printed JSON with LF endings and a trailing
/// newline, so golden files compare identically on every platform.
pub fn to_json(graph: &DialogueGraph) -> Result<String> {
    let mut text = serde_json::to_string_pretty(graph).map_err(|_| Error::Encode)?;
    text.push('\n');
    Ok(text)
}
