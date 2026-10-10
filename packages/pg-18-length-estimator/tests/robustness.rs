// SPDX-License-Identifier: MIT OR Apache-2.0

use screenplay_length::{estimate, estimate_with, LayoutProfile, PageSize};

const ODD_INPUTS: &[&str] = &[
    "",
    "\n",
    "^",
    "@",
    "@\nx",
    ">",
    "><",
    "> <",
    ".",
    ".A",
    "!",
    "~",
    "#",
    "=",
    "==",
    "===",
    "=== ===",
    "/*",
    "*/",
    "[[",
    "]]",
    "[[ /* ]] */",
    "/* [[ */ ]]",
    "INT.",
    "INT. #",
    "INT. ##",
    "INT. #1#",
    "INT. #a b#",
    "TO:",
    "A TO:",
    "BOB ^\n^",
    "BOB\n(\n)",
    "BOB\n()",
    "Title:",
    "Title:\n   \n",
    "Title: X\nINT. A\n",
    "  \n  \n",
    "\t\t\t",
    "\u{feff}",
    "\u{feff}\u{feff}INT. A",
    "a\r\n\rb\r",
    "ABC\n\u{200b}",
];

#[test]
fn odd_inputs_never_panic() {
    let long_accented = "é".repeat(5000);
    for input in ODD_INPUTS.iter().copied().chain([long_accented.as_str()]) {
        let l = estimate(input, PageSize::Letter).unwrap();
        let a = estimate(input, PageSize::A4).unwrap();
        assert!(l.total.pages <= 2, "{input:?}");
        assert!(a.total.pages <= 2, "{input:?}");
    }
}

#[test]
fn pathological_shapes_never_panic() {
    let giant_word = "x".repeat(100_000);
    let many_headings = "INT. A\n\n".repeat(5_000);
    let many_cues = "BOB\nHi.\n\n".repeat(5_000);
    let many_breaks = "===\n".repeat(5_000);
    let many_dual = "BOB\nHi.\n\nANNA ^\nHo.\n\n".repeat(2_000);
    for input in [
        giant_word.as_str(),
        many_headings.as_str(),
        many_cues.as_str(),
        many_breaks.as_str(),
        many_dual.as_str(),
    ] {
        let e = estimate(input, PageSize::Letter).unwrap();
        let sum: u32 = e.scenes.iter().map(|s| s.line_count).sum();
        assert_eq!(sum, e.total.lines);
    }
    let e = estimate(&many_breaks, PageSize::Letter).unwrap();
    // Consecutive page breaks are idempotent: nothing to break.
    assert_eq!(e.total.pages, 0);
    let e = estimate(&many_headings, PageSize::Letter).unwrap();
    assert_eq!(e.scenes.len(), 5_000);
}

#[test]
fn tiny_profile_degenerate_pages_never_panic() {
    let profile = LayoutProfile::from_json(
        r#"{"page_size":"tiny","lines_per_page":8,"character_width":1,"dialogue_width":1,
            "heading_keep_lines":6,"dialogue_split_min_lines":5}"#,
    )
    .unwrap();
    let text = "INT. A VERY LONG HEADING THAT WRAPS ONTO MANY LINES BECAUSE THE WIDTH IS SMALL\n\n\
                CHARACTER NAME\nSome dialogue that is fairly long.\n\n\
                OTHER ^\nShort.\n";
    let profile = LayoutProfile {
        scene_heading_width: 3,
        ..profile
    };
    let e = estimate_with(text, &profile).unwrap();
    assert!(e.total.pages >= 1);
    assert_eq!(e.scenes.len(), 1);
}
