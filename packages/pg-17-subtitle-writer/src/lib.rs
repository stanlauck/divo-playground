// SPDX-License-Identifier: MIT OR Apache-2.0

//! Neutral dialogue timing JSON v1 → SRT / WebVTT / TTML, plus readability
//! checks (reading speed, line length, lines per cue, duration) and structural
//! checks (ordering, overlap, unique ids, empty lines) reported as JSON.
//!
//! ```
//! use subtitle_writer::{check, parse, write, Checks, Format};
//!
//! let document = parse(r#"{
//!   "version": 1,
//!   "cues": [
//!     {"id": "c1", "speaker": "ANNA", "lines": ["Hello."], "start_ms": 0, "end_ms": 1500}
//!   ]
//! }"#)?;
//! let srt = write(&document, Format::Srt)?;
//! assert_eq!(srt, "1\n00:00:00,000 --> 00:00:01,500\nANNA: Hello.\n");
//! let violations = check(&document, &Checks::default())?;
//! assert!(violations.is_empty());
//! # Ok::<(), subtitle_writer::Error>(())
//! ```
//!
//! See `README.md` for the full field, check and format reference.

mod checks;
mod error;
mod model;
mod writers;

pub use checks::{
    check, reading_speed, report_json, Checks, Metric, Rule, Violation, DEFAULT_MAX_CPS,
    DEFAULT_MAX_DURATION_MS, DEFAULT_MAX_LINES, DEFAULT_MAX_LINE_LENGTH, DEFAULT_MIN_DURATION_MS,
};
pub use error::{Error, Result};
pub use model::{
    parse, read_input, Cue, Document, MAX_CUES, MAX_DEPTH, MAX_INPUT_BYTES, MAX_LINES_PER_CUE,
    MAX_LINE_CHARS, MAX_NAME_CHARS, MAX_TIME_MS, MAX_TITLE_CHARS, SCHEMA_VERSION,
};
pub use writers::{escape_text, format_timecode, write, write_srt, write_ttml, write_vtt, Format};
