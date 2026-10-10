// SPDX-License-Identifier: MIT OR Apache-2.0
#![doc = include_str!("../README.md")]

mod model;
mod pdf;
mod render;

pub use model::{
    from_json, read_report, validate, Block, CalloutKind, Error, Report, Section, MAX_BLOCKS,
    MAX_DEPTH, MAX_INPUT_BYTES, MAX_SECTIONS,
};
pub use pdf::compile_pdf;
pub use render::{render_markdown, render_typst, PageSize, RenderOptions, Rendered, Warning};
