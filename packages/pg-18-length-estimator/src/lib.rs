// SPDX-License-Identifier: MIT OR Apache-2.0

//! Deterministic screenplay length estimator: Fountain text to per-scene
//! length in eighths of a page for Courier 12 pt on US Letter or A4.
//!
//! ```
//! use screenplay_length::{estimate, PageSize};
//!
//! let text = "INT. KITCHEN - DAY\n\nA kettle whistles.\n\nMARA\nTea?\n";
//! let est = estimate(text, PageSize::Letter).unwrap();
//! assert_eq!(est.scenes.len(), 1);
//! assert_eq!(est.scenes[0].eighths, 1);
//! assert_eq!(est.total.pages, 1);
//! ```

mod error;
mod layout;
mod paginate;
mod parser;

pub use error::{Error, Result};
pub use layout::{wrap, LayoutProfile, PageGeometry, PageSize};
pub use paginate::{
    eighths_to_pages, lines_to_eighths, paginate, Estimate, SceneLength, Totals, SCHEMA_VERSION,
};
pub use parser::{
    parse, DialogueBlock, DialogueLine, Document, Element, TitleEntry, MAX_INPUT_BYTES,
    MAX_INPUT_LINES,
};

/// Estimates scene lengths with the default profile of `page_size`.
pub fn estimate(text: &str, page_size: PageSize) -> Result<Estimate> {
    estimate_with(text, &page_size.profile())
}

/// Estimates scene lengths with an explicit layout profile.
pub fn estimate_with(text: &str, profile: &LayoutProfile) -> Result<Estimate> {
    profile.validate()?;
    let doc = parse(text)?;
    Ok(paginate(&doc, profile))
}

/// Serializes an estimate as pretty JSON with a trailing newline. The output
/// is deterministic: field order is fixed by the structs.
pub fn to_json(estimate: &Estimate) -> String {
    let mut text = serde_json::to_string_pretty(estimate).unwrap_or_default();
    text.push('\n');
    text
}

/// Renders the human-readable table printed by the CLI.
pub fn to_table(estimate: &Estimate) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "page size: {}  ({} lines/page)\n",
        estimate.page_size, estimate.layout.lines_per_page
    ));
    if estimate.title_page {
        out.push_str("title page: yes (page 1)\n");
    }
    out.push_str("  #  no.    pages  eighths  lines  start-end  heading\n");
    for s in &estimate.scenes {
        out.push_str(&format!(
            "{:>3}  {:<6} {:<6} {:>7}  {:>5}  {:>4}-{:<4}  {}\n",
            s.index + 1,
            s.number.as_deref().unwrap_or("-"),
            s.pages,
            s.eighths,
            s.line_count,
            s.start_page,
            s.end_page,
            s.heading.as_deref().unwrap_or("(before first heading)")
        ));
    }
    out.push_str(&format!(
        "total: {} page(s), {} eighths ({}), {} body lines\n",
        estimate.total.pages,
        estimate.total.eighths,
        eighths_to_pages(estimate.total.eighths),
        estimate.total.lines
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCRIPT: &str =
        "Title: T\n\nINT. A - DAY\n\nOne.\n\nBOB\nHi.\n\nEXT. B - NIGHT #2#\n\nTwo.\n";

    #[test]
    fn estimate_is_deterministic() {
        let a = to_json(&estimate(SCRIPT, PageSize::Letter).unwrap());
        let b = to_json(&estimate(SCRIPT, PageSize::Letter).unwrap());
        assert_eq!(a, b);
        assert!(a.ends_with('\n'));
        assert!(!a.contains('\r'));
    }

    #[test]
    fn estimate_with_rejects_invalid_profile() {
        let mut profile = PageSize::Letter.profile();
        profile.lines_per_page = 0;
        assert!(matches!(
            estimate_with("INT. A\n", &profile),
            Err(Error::InvalidProfile(_))
        ));
    }

    #[test]
    fn json_has_documented_top_level_fields() {
        let json = to_json(&estimate(SCRIPT, PageSize::A4).unwrap());
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["version"], 1);
        assert_eq!(value["page_size"], "a4");
        assert_eq!(value["layout"]["lines_per_page"], 58);
        assert_eq!(value["title_page"], true);
        assert_eq!(value["scenes"][1]["number"], "2");
        assert_eq!(value["scenes"][0]["number"], serde_json::Value::Null);
        assert!(value["total"]["pages"].is_u64());
    }

    #[test]
    fn table_lists_every_scene() {
        let table = to_table(&estimate(SCRIPT, PageSize::Letter).unwrap());
        assert!(table.contains("INT. A - DAY"));
        assert!(table.contains("EXT. B - NIGHT"));
        assert!(table.contains("title page: yes"));
        assert!(table.contains("total: 2 page(s)"));
    }
}
