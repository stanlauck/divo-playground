// SPDX-License-Identifier: MIT OR Apache-2.0
mod common;
use common::*;
use std::{
    fs,
    process::{Command, Output},
};

fn cli(args: &[&str], temp: &Temp) -> Output {
    Command::new(env!("CARGO_BIN_EXE_report-renderer"))
        .args(args)
        .current_dir(&temp.0)
        .output()
        .unwrap()
}
fn fixture(temp: &Temp) {
    fs::write(temp.0.join("in.json"), SAMPLE).unwrap();
}
#[test]
fn help() {
    let t = Temp::new();
    let r = cli(&["--help"], &t);
    assert!(r.status.success());
    assert!(String::from_utf8_lossy(&r.stdout).contains("report-renderer <in.json>"));
}
#[test]
fn markdown_cli_matches_golden() {
    let t = Temp::new();
    fixture(&t);
    let r = cli(&["in.json", "--md", "out.md"], &t);
    assert!(r.status.success(), "{:?}", r.stderr);
    assert!(r.stderr.is_empty());
    assert_eq!(
        fs::read_to_string(t.0.join("out.md")).unwrap(),
        include_str!("../samples/synthetic-report.md")
    );
}
#[test]
fn optional_cli_matches_golden() {
    let t = Temp::new();
    fixture(&t);
    let r = cli(&["in.json", "--md", "out.md", "--include-optional"], &t);
    assert!(r.status.success());
    assert_eq!(
        fs::read_to_string(t.0.join("out.md")).unwrap(),
        include_str!("../samples/synthetic-report.optional.md")
    );
}
#[test]
fn typst_cli_matches_golden_and_warns() {
    let t = Temp::new();
    fixture(&t);
    let r = cli(&["in.json", "--typ", "out.typ"], &t);
    assert!(r.status.success());
    assert!(String::from_utf8_lossy(&r.stderr).contains("missing-lantern.png"));
    assert_eq!(
        fs::read_to_string(t.0.join("out.typ")).unwrap(),
        include_str!("../samples/synthetic-report.typ")
    );
}
#[test]
fn cli_selection_toc_and_letter() {
    let t = Temp::new();
    fixture(&t);
    let r = cli(
        &[
            "in.json",
            "--typ",
            "out.typ",
            "--include",
            "appendix",
            "--exclude",
            "visuals,resources",
            "--toc",
            "--page",
            "letter",
        ],
        &t,
    );
    assert!(r.status.success());
    let s = fs::read_to_string(t.0.join("out.typ")).unwrap();
    assert!(s.contains("Optional appendix"));
    assert!(!s.contains("Resource detail"));
    assert!(!s.contains("Visual plan"));
    assert!(s.contains("#outline"));
    assert!(s.contains("us-letter"));
    assert!(r.stderr.is_empty());
}
#[test]
fn cli_unknown_id_warns_but_succeeds() {
    let t = Temp::new();
    fixture(&t);
    let r = cli(&["in.json", "--md", "out.md", "--include", "unknown"], &t);
    assert!(r.status.success());
    assert!(String::from_utf8_lossy(&r.stderr).contains("unknown section"));
}
#[test]
fn repeated_include_exclude_flags_accumulate() {
    let t = Temp::new();
    fixture(&t);
    let r = cli(
        &[
            "in.json",
            "--md",
            "out.md",
            "--include",
            "appendix",
            "--include",
            "overview",
            "--exclude",
            "visuals",
            "--exclude",
            "resources",
        ],
        &t,
    );
    assert!(r.status.success());
    let text = fs::read_to_string(t.0.join("out.md")).unwrap();
    assert!(text.contains("Optional appendix"));
    assert!(text.contains("Overview"));
    assert!(!text.contains("Visual plan"));
    assert!(!text.contains("Resource detail"));
}
#[test]
fn malformed_input_exits_one_without_output() {
    let t = Temp::new();
    fs::write(t.0.join("in.json"), "{").unwrap();
    let r = cli(&["in.json", "--md", "out.md"], &t);
    assert_eq!(r.status.code(), Some(1));
    assert!(!t.0.join("out.md").exists());
}
#[test]
fn missing_input_exits_one() {
    let t = Temp::new();
    let r = cli(&["absent.json", "--md", "out.md"], &t);
    assert_eq!(r.status.code(), Some(1));
}
#[test]
fn output_cannot_overwrite_input() {
    let t = Temp::new();
    fixture(&t);
    let r = cli(&["in.json", "--md", "in.json"], &t);
    assert_eq!(r.status.code(), Some(1));
    assert_eq!(fs::read_to_string(t.0.join("in.json")).unwrap(), SAMPLE);
}
#[test]
fn argument_errors_exit_one() {
    let t = Temp::new();
    fixture(&t);
    for args in [
        vec![],
        vec!["in.json"],
        vec!["in.json", "--md"],
        vec!["in.json", "--md", "a", "--typ", "b"],
        vec!["in.json", "--md", "a", "--page", "legal"],
        vec!["in.json", "--md", "a", "--include", "a,,b"],
        vec!["in.json", "--md", "a", "--exclude", ""],
        vec!["in.json", "--md", "a", "--typst", "typst"],
        vec!["in.json", "--md", "a", "--wat"],
        vec!["in.json", "--include", "--md", "a"],
        vec!["in.json", "--pdf", "a", "--typst", "t", "--typst", "t"],
    ] {
        let r = cli(&args, &t);
        assert_eq!(r.status.code(), Some(1), "{args:?}");
    }
}
#[test]
fn missing_compiler_exits_one_and_cleans_staging() {
    let t = Temp::new();
    fixture(&t);
    let r = cli(
        &["in.json", "--pdf", "out.pdf", "--typst", "./absent-typst"],
        &t,
    );
    assert_eq!(r.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&r.stderr).contains("Typst executable not found"));
    assert!(!t.0.join("out.pdf").exists());
    assert!(fs::read_dir(&t.0).unwrap().all(|e| !e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".report-renderer-")));
}
#[cfg(unix)]
#[test]
fn pdf_cli_stages_images_within_root_and_cleans_up() {
    use std::os::unix::fs::PermissionsExt;
    let t = Temp::new();
    fs::create_dir(t.0.join("input")).unwrap();
    fs::create_dir(t.0.join("output")).unwrap();
    fs::write(t.0.join("input/lamp.svg"), b"synthetic image bytes").unwrap();
    let mut report = serde_json::to_value(minimal()).unwrap();
    report["sections"][0]["blocks"] = serde_json::json!([{"type":"image","path":"lamp.svg"}]);
    fs::write(t.0.join("input/in.json"), report.to_string()).unwrap();
    let script = t.0.join("compiler");
    fs::write(&script,"#!/bin/sh\n# SPDX-License-Identifier: MIT OR Apache-2.0\n[ \"$1\" = compile ] || exit 2\n[ \"$2\" = --root ] || exit 3\n[ -f \"$3/assets/lamp.svg\" ] || exit 4\n[ -f \"$4\" ] || exit 5\nprintf '%%PDF synthetic' > \"$5\"\n").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
    let r = cli(
        &[
            "input/in.json",
            "--pdf",
            "output/out.pdf",
            "--typst",
            "./compiler",
        ],
        &t,
    );
    assert!(r.status.success(), "{:?}", r.stderr);
    assert!(r.stderr.is_empty());
    assert!(fs::read(t.0.join("output/out.pdf"))
        .unwrap()
        .starts_with(b"%PDF"));
    assert_eq!(fs::read_dir(t.0.join("output")).unwrap().count(), 1);
}
