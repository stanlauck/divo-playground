// SPDX-License-Identifier: MIT OR Apache-2.0

//! Streaming prose import into a format-independent rich-block JSON model.
//!
//! XML is read incrementally; at most one complex block plus bounded package
//! metadata is materialized. DOCX requires a seekable ZIP input. Failed imports
//! can leave a partial JSON stream, which callers must discard.

#![doc = include_str!("../README.md")]

mod docx;
mod emit;
mod error;
mod fb2;
mod model;
mod txt;
mod xml;

pub use docx::import_docx;
pub use error::{Error, Result};
pub use fb2::import_fb2;
pub use model::{
    Block, Chapter, Document, ImportOptions, ImportReport, ListItem, Loss, SourceFormat, Span,
    Stanza, TableCell, TableRow,
};
pub use txt::import_txt;
