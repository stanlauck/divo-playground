// SPDX-License-Identifier: MIT OR Apache-2.0

mod metadata;
mod reader;
mod reels;
mod writer;

pub use reader::from_edl;
pub use writer::to_edl;

/// A percent-encoded UTF-8 ID, on the event it belongs to.
pub(crate) const SHOT_ID_PREFIX: &str = "* SHOT ID: ";
pub(crate) const TIMELINE_PREFIX: &str = "* PG16 TIMELINE: ";
pub(crate) const SHOT_PREFIX: &str = "* PG16 SHOT: ";
pub(crate) const GAP_COMMENT: &str = "* PG16 GAP: 1";
pub(crate) const MAX_LINE: usize = 128 * 1024;
pub(crate) const MAX_LINES: usize = 20_000;
