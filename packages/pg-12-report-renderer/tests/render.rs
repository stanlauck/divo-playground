// SPDX-License-Identifier: MIT OR Apache-2.0
mod common;
use common::*;
use report_renderer::*;
use serde_json::json;

fn block_report(block: serde_json::Value) -> Report {
    let mut value = serde_json::to_value(minimal()).unwrap();
    value["sections"][0]["blocks"] = json!([block]);
    from_json(&value.to_string()).unwrap()
}
fn md(block: serde_json::Value) -> String {
    render_markdown(&block_report(block), &RenderOptions::default()).text
}
fn typ(block: serde_json::Value) -> String {
    render_typst(&block_report(block), &RenderOptions::default()).text
}

#[test]
fn golden_markdown() {
    assert_eq!(
        render_markdown(&sample(), &RenderOptions::default()).text,
        include_str!("../samples/synthetic-report.md")
    );
}
#[test]
fn golden_optional_markdown() {
    assert_eq!(
        render_markdown(
            &sample(),
            &RenderOptions {
                include_optional: true,
                ..Default::default()
            }
        )
        .text,
        include_str!("../samples/synthetic-report.optional.md")
    );
}
#[test]
fn golden_typst() {
    assert_eq!(
        render_typst(&sample(), &RenderOptions::default()).text,
        include_str!("../samples/synthetic-report.typ")
    );
}

macro_rules! block_test {
    ($name:ident,$input:expr,$md:expr,$typ:expr) => {
        #[test]
        fn $name() {
            let b = $input;
            assert!(md(b.clone()).contains($md));
            assert!(typ(b).contains($typ));
        }
    };
}
block_test!(
    paragraph,
    json!({"type":"paragraph","text":"Text *plain*"}),
    "Text \\*plain\\*",
    "#text(\"Text *plain*\")"
);
block_test!(
    heading,
    json!({"type":"heading","text":"Inner","level":3}),
    "### Inner",
    "#heading(level: 3, outlined: false, text(\"Inner\"))"
);
block_test!(
    bullets,
    json!({"type":"bullets","items":["one","two"]}),
    "- one\n- two\n",
    "#list(text(\"one\"),text(\"two\"),)"
);
block_test!(
    numbered,
    json!({"type":"numbered","items":["one","two"]}),
    "1. one\n2. two\n",
    "#enum(text(\"one\"),text(\"two\"),)"
);
block_test!(
    table,
    json!({"type":"table","columns":["Column"],"rows":[["a|b"]],"caption":"Caption"}),
    "| a\\|b |",
    "text(\"a|b\"),"
);
block_test!(
    key_values,
    json!({"type":"key_values","pairs":[["Key *","Value |"]]}),
    "| Key \\* | Value \\| |",
    "text(\"Key *\"),text(\"Value |\"),"
);
block_test!(
    code,
    json!({"type":"code","text":"#hello()","language":"text"}),
    "```text\n#hello()\n```",
    "#raw(\"#hello()\", block: true, lang: \"text\")"
);
block_test!(
    quote,
    json!({"type":"quote","text":"Quoted","attribution":"Writer"}),
    "> Quoted\n>\n> — Writer",
    "#quote(block: true, text(\"Quoted\"), attribution: text(\"Writer\"))"
);
block_test!(
    callout_note,
    json!({"type":"callout","kind":"note","text":"Literal"}),
    "> **Note:** Literal",
    "fill: rgb(\"e9f1fa\")"
);
block_test!(
    callout_warning,
    json!({"type":"callout","kind":"warning","text":"Literal"}),
    "> **Warning:** Literal",
    "fill: rgb(\"fff3d6\")"
);
block_test!(
    callout_error,
    json!({"type":"callout","kind":"error","text":"Literal"}),
    "> **Error:** Literal",
    "fill: rgb(\"fde8e8\")"
);
block_test!(
    callout_success,
    json!({"type":"callout","kind":"success","text":"Literal"}),
    "> **Success:** Literal",
    "fill: rgb(\"e6f4eb\")"
);
block_test!(
    page_break,
    json!({"type":"page_break"}),
    "<!-- page break -->",
    "#pagebreak()"
);
block_test!(
    image,
    json!({"type":"image","path":"not-present.png","caption":"A *caption*","width_percent":33}),
    "![A \\*caption\\*](not-present.png)",
    "#block(width: 33%, height: 48pt"
);

#[test]
fn markdown_table_escapes_every_inline_special_character() {
    for c in "\\*_`[]<>|~&#".chars() {
        let cell = format!("{c}before{c}after{c}");
        let rendered = md(json!({"type":"table","columns":[cell],"rows":[[cell]]}));
        let expected = format!("\\{c}before\\{c}after\\{c}");
        assert!(rendered.contains(&expected), "{c}: {rendered}");
    }
}
#[test]
fn markdown_escapes_block_starters_only_at_line_start() {
    for prefix in ["- x", "+ x", "> x", "= x", "1. x", "12) x"] {
        let r = md(json!({"type":"paragraph","text":prefix}));
        let (lead, rest) = prefix.split_at(prefix.find(['-', '+', '>', '=', '.', ')']).unwrap());
        assert!(r.contains(&format!("{lead}\\{rest}")), "{prefix}: {r}");
    }
    // Readable prose keeps its punctuation when it cannot open a construct.
    let r = md(
        json!({"type":"paragraph","text":"Shot 2026-10-10: ready, take 1. Plan (B) \"ok\"; a + b = c > d"}),
    );
    assert!(
        r.contains("Shot 2026-10-10: ready, take 1. Plan (B) \"ok\"; a + b = c \\> d"),
        "{r}"
    );
}
#[test]
fn typst_table_preserves_every_special_character_as_string_data() {
    for c in "#*_@<>$`~//\\\"".chars() {
        let cell = format!("{c}before{c}after{c}");
        let rendered = typ(json!({"type":"table","columns":[cell],"rows":[[cell]]}));
        let escaped = cell.replace('\\', "\\\\").replace('"', "\\\"");
        assert!(
            rendered.contains(&format!("text(\"{escaped}\")")),
            "{c}: {rendered}"
        );
    }
}
#[test]
fn deterministic_escaping_mixture_property() {
    let alphabet: Vec<_> = "*_#|\\<>[]@~$`//\"'&-+!=:;(){}\n\r\t Фонарь"
        .chars()
        .collect();
    let mut seed = 7u64;
    for _ in 0..256 {
        let mut text = String::new();
        for _ in 0..64 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            text.push(alphabet[(seed >> 32) as usize % alphabet.len()]);
        }
        let r = block_report(json!({"type":"table","columns":[text],"rows":[[text]]}));
        let a = render_markdown(&r, &Default::default());
        let b = render_typst(&r, &Default::default());
        assert!(!a.text.contains('\r'));
        assert!(!b.text.contains('\r'));
        assert_eq!(a, render_markdown(&r, &Default::default()));
        assert_eq!(b, render_typst(&r, &Default::default()));
        assert_eq!(a.text.lines().filter(|s| s.starts_with('|')).count(), 3);
    }
}
#[test]
fn markdown_multiline_cannot_inject_blocks() {
    let r = md(
        json!({"type":"paragraph","text":"    indented\n# heading\n- bullet\n1. item\n<script>\n[link](url)\n\n```"}),
    );
    assert!(r.contains("&#32;&#32;&#32;&#32;indented<br>\\# heading<br>\\- bullet<br>1\\. item"));
    assert!(!r.contains("\n<script>"));
    assert!(!r.contains("\n```"));
}
#[test]
fn multiline_table_cells_stay_in_one_row() {
    let r = md(json!({"type":"table","columns":["a\nb"],"rows":[["x\r\ny\rz"]]}));
    assert!(r.contains("| a<br>b |"));
    assert!(r.contains("| x<br>y<br>z |"));
}
#[test]
fn literal_backslashes_cannot_escape_renderer_markup() {
    let r = md(json!({"type":"paragraph","text":"\\*\\[\\<"}));
    assert!(r.contains("\\\\\\*\\\\\\[\\\\\\<"));
}
#[test]
fn markdown_dynamic_code_fences() {
    let r = md(json!({"type":"code","text":"```\n````\n~~~\n#literal","language":"text"}));
    assert!(r.contains("`````text\n```\n````\n~~~\n#literal\n`````\n"));
}
#[test]
fn empty_lists_and_pairs_are_supported() {
    for block in [
        json!({"type":"bullets","items":[]}),
        json!({"type":"numbered","items":[]}),
        json!({"type":"key_values","pairs":[]}),
    ] {
        assert!(md(block.clone()).starts_with("# Example"));
        assert!(typ(block).contains("#set page"));
    }
}
#[test]
fn optional_quote_and_code_fields() {
    assert!(md(json!({"type":"quote","text":"a"})).contains("> a\n\n"));
    assert!(typ(json!({"type":"quote","text":"a"})).contains("#quote(block: true, text(\"a\"))"));
    assert!(md(json!({"type":"code","text":""})).contains("```\n\n```"));
    assert!(typ(json!({"type":"code","text":""})).contains("#raw(\"\", block: true)"));
}
#[test]
fn markdown_images_do_not_read_or_warn() {
    let result = render_markdown(&sample(), &Default::default());
    assert!(result.warnings.is_empty());
    assert!(result.text.contains("![Invented lantern reference"));
}
#[test]
fn markdown_image_destinations_are_percent_encoded() {
    let text = md(json!({"type":"image","path":"images/фонарь (a)*%.png","caption":"[literal]"}));
    assert!(text.contains(
        "![\\[literal\\]](images/%D1%84%D0%BE%D0%BD%D0%B0%D1%80%D1%8C%20%28a%29%2A%25.png)"
    ));
}
#[test]
fn missing_image_warns_and_has_placeholder() {
    let temp = Temp::new();
    let r = block_report(json!({"type":"image","path":"missing.png"}));
    let rendered = render_typst(
        &r,
        &RenderOptions {
            image_root: Some(temp.0.clone()),
            ..Default::default()
        },
    );
    assert_eq!(
        rendered.warnings,
        vec![Warning::MissingImage {
            path: "missing.png".into()
        }]
    );
    assert!(rendered.text.contains("Image unavailable: missing.png"));
    assert!(!rendered.text.contains("#image("));
}
#[test]
fn existing_image_is_passed_through_without_reading_content() {
    let temp = Temp::new();
    std::fs::write(temp.0.join("a.png"), b"not decoded by renderer").unwrap();
    let r = block_report(json!({"type":"image","path":"a.png","width_percent":50}));
    let rendered = render_typst(
        &r,
        &RenderOptions {
            image_root: Some(temp.0.clone()),
            ..Default::default()
        },
    );
    assert!(rendered.warnings.is_empty());
    assert!(rendered.text.contains("#image(\"a.png\", width: 50%)"));
}
#[cfg(unix)]
#[test]
fn image_symlink_cannot_escape_root() {
    let temp = Temp::new();
    let external = Temp::new();
    std::fs::write(external.0.join("a.png"), b"x").unwrap();
    std::os::unix::fs::symlink(external.0.join("a.png"), temp.0.join("a.png")).unwrap();
    let r = block_report(json!({"type":"image","path":"a.png"}));
    let rendered = render_typst(
        &r,
        &RenderOptions {
            image_root: Some(temp.0.clone()),
            ..Default::default()
        },
    );
    assert_eq!(rendered.warnings.len(), 1);
}
#[test]
fn typst_strings_cannot_inject_source() {
    let r = typ(
        json!({"type":"paragraph","text":"\"); #read(\"secret\"); //\n#image(\"https://example.test\")\n\\"}),
    );
    assert!(r.contains(
        "#text(\"\\\"); #read(\\\"secret\\\"); //\\n#image(\\\"https://example.test\\\")\\n\\\\\")"
    ));
}
#[test]
fn typst_control_character_escaping() {
    let r = typ(json!({"type":"paragraph","text":"a\u{1}b\t\n\r"}));
    assert!(r.contains("a\\u{1}b\\t\\n\\n"));
}
#[test]
fn toc_has_stable_anchors_and_escapes_titles() {
    let mut r = sample();
    r.sections[0].title = "Duplicate *title*".into();
    r.sections[1].title = "Duplicate *title*".into();
    r.sections[0].id = "<unsafe>".into();
    let rendered = render_markdown(
        &r,
        &RenderOptions {
            toc: true,
            ..Default::default()
        },
    );
    assert!(rendered
        .text
        .contains("- [Duplicate \\*title\\*](#section-1)"));
    assert!(rendered
        .text
        .contains("- [Duplicate \\*title\\*](#section-3)"));
    assert!(rendered.text.contains("<a id=\"section-1\"></a>"));
    assert!(!rendered.text.contains("<unsafe>"));
    let t = render_typst(
        &r,
        &RenderOptions {
            toc: true,
            ..Default::default()
        },
    );
    assert!(t.text.contains("#outline(title: [Contents])"));
}
#[test]
fn toc_level_four_first_is_not_a_code_block() {
    let mut r = minimal();
    r.sections[0].level = 4;
    let text = render_markdown(
        &r,
        &RenderOptions {
            toc: true,
            ..Default::default()
        },
    )
    .text;
    assert!(text.contains("\n- [One](#section-1)\n"));
}
#[test]
fn letter_and_a4() {
    assert!(render_typst(&minimal(), &Default::default())
        .text
        .contains("paper: \"a4\""));
    assert!(render_typst(
        &minimal(),
        &RenderOptions {
            page: PageSize::Letter,
            ..Default::default()
        }
    )
    .text
    .contains("paper: \"us-letter\""));
}
#[test]
fn metadata_is_escaped_and_dates_are_not_generated() {
    let mut r = minimal();
    r.title = "*Title* #".into();
    r.author = Some("A <B>".into());
    let m = render_markdown(&r, &Default::default()).text;
    assert!(m.starts_with("# \\*Title\\* \\#"));
    let t = render_typst(&r, &Default::default()).text;
    assert!(t.contains("author: \"A <B>\", date: none"));
    assert!(!m.contains("2026"));
    r.date = Some("2026-01-02".into());
    assert!(render_typst(&r, &Default::default())
        .text
        .contains("datetime(year: 2026, month: 1, day: 2)"));
}
#[test]
fn cyrillic_and_language_are_preserved() {
    let t = render_typst(&sample(), &Default::default()).text;
    assert!(t.contains("#set text(lang: \"ru\")"));
    assert!(t.contains("Все данные вымышлены"));
    assert!(render_markdown(&sample(), &Default::default())
        .text
        .contains("Все данные вымышлены"));
}
#[test]
fn invalid_programmatic_report_returns_warning_without_text() {
    let mut r = minimal();
    r.sections[0].level = 0;
    for rendered in [
        render_markdown(&r, &Default::default()),
        render_typst(&r, &Default::default()),
    ] {
        assert!(rendered.text.is_empty());
        assert!(matches!(
            &rendered.warnings[..],
            [Warning::InvalidReport { .. }]
        ));
    }
}

fn selected(options: RenderOptions) -> String {
    render_markdown(&sample(), &options).text
}
#[test]
fn default_optional_selection() {
    let text = selected(Default::default());
    assert!(text.contains("Overview"));
    assert!(text.contains("Resource detail"));
    assert!(!text.contains("Optional appendix"));
}
#[test]
fn include_optional_selects_all_enabled_sections() {
    assert!(selected(RenderOptions {
        include_optional: true,
        ..Default::default()
    })
    .contains("Optional appendix"));
}
#[test]
fn explicit_include_enables_optional_but_keeps_regular_sections() {
    let text = selected(RenderOptions {
        include: Some(vec!["appendix".into()]),
        ..Default::default()
    });
    assert!(text.contains("Optional appendix"));
    assert!(text.contains("Overview"));
}
#[test]
fn empty_include_does_not_filter_regular_sections() {
    assert_eq!(
        selected(Default::default()),
        selected(RenderOptions {
            include: Some(vec![]),
            ..Default::default()
        })
    );
}
#[test]
fn exclude_wins_over_explicit_include() {
    assert!(!selected(RenderOptions {
        include: Some(vec!["appendix".into()]),
        exclude: vec!["appendix".into()],
        include_optional: true,
        ..Default::default()
    })
    .contains("Optional appendix"));
}
#[test]
fn excluding_parent_excludes_nested_section() {
    let text = selected(RenderOptions {
        exclude: vec!["overview".into()],
        include: Some(vec!["resources".into()]),
        ..Default::default()
    });
    assert!(!text.contains("Overview"));
    assert!(!text.contains("Resource detail"));
    assert!(text.contains("Visual plan"));
}
#[test]
fn excluding_child_keeps_parent() {
    let text = selected(RenderOptions {
        exclude: vec!["resources".into()],
        ..Default::default()
    });
    assert!(text.contains("Overview"));
    assert!(!text.contains("Resource detail"));
}
#[test]
fn disabled_sections_cannot_be_included() {
    let mut r = sample();
    r.sections[2].enabled = false;
    for include_optional in [false, true] {
        let o = RenderOptions {
            include: Some(vec!["appendix".into()]),
            include_optional,
            ..Default::default()
        };
        assert!(!render_markdown(&r, &o).text.contains("Optional appendix"));
    }
}
#[test]
fn disabled_parent_suppresses_descendants() {
    let mut r = sample();
    r.sections[0].enabled = false;
    assert!(!render_markdown(&r, &Default::default())
        .text
        .contains("Resource detail"));
}
#[test]
fn optional_parent_suppresses_explicit_child_until_parent_included() {
    let mut r = sample();
    r.sections[0].optional = true;
    let mut o = RenderOptions {
        include: Some(vec!["resources".into()]),
        ..Default::default()
    };
    assert!(!render_markdown(&r, &o).text.contains("Resource detail"));
    o.include.as_mut().unwrap().push("overview".into());
    assert!(render_markdown(&r, &o).text.contains("Resource detail"));
}
#[test]
fn selection_boolean_matrix() {
    for enabled in [false, true] {
        for optional in [false, true] {
            for include_optional in [false, true] {
                for included in [false, true] {
                    for excluded in [false, true] {
                        let mut r = minimal();
                        r.sections[0].enabled = enabled;
                        r.sections[0].optional = optional;
                        let o = RenderOptions {
                            include: included.then(|| vec!["one".into()]),
                            exclude: if excluded { vec!["one".into()] } else { vec![] },
                            include_optional,
                            ..Default::default()
                        };
                        let expected =
                            enabled && !excluded && (!optional || include_optional || included);
                        assert_eq!(render_markdown(&r, &o).text.contains("# One\n"), expected);
                        assert_eq!(
                            render_typst(&r, &o)
                                .text
                                .contains("#heading(level: 1, text(\"One\"))"),
                            expected
                        );
                    }
                }
            }
        }
    }
}
#[test]
fn unknown_ids_warn_once_per_option_in_request_order() {
    let o = RenderOptions {
        include: Some(vec![
            "resources".into(),
            "missing-b".into(),
            "missing-a".into(),
            "missing-b".into(),
        ]),
        exclude: vec!["missing-a".into()],
        ..Default::default()
    };
    let m = render_markdown(&sample(), &o);
    assert_eq!(
        m.warnings,
        vec![
            Warning::UnknownSection {
                id: "missing-b".into(),
                option: "include"
            },
            Warning::UnknownSection {
                id: "missing-a".into(),
                option: "include"
            },
            Warning::UnknownSection {
                id: "missing-a".into(),
                option: "exclude"
            }
        ]
    );
    assert_eq!(&render_typst(&sample(), &o).warnings[..3], m.warnings);
}
#[test]
fn excluded_images_do_not_warn() {
    let r = render_typst(
        &sample(),
        &RenderOptions {
            exclude: vec!["visuals".into()],
            ..Default::default()
        },
    );
    assert!(r.warnings.is_empty());
}
