// SPDX-License-Identifier: MIT OR Apache-2.0

use std::io::Write;
use std::process::{Command, Stdio};

fn cli(args: &[&str], stdin: &[u8]) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_screenplay-length"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    child.wait_with_output().unwrap()
}

fn sample() -> String {
    format!("{}/samples/synthetic.fountain", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn table_output_for_sample_file() {
    let out = cli(&[&sample()], b"");
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.starts_with("page size: letter"));
    assert!(text.contains("EXT. HARBOUR WALL - DUSK"));
    assert!(text.contains("total: 4 page(s)"));
}

#[test]
fn json_output_matches_golden() {
    let out = cli(&[&sample(), "--json", "--a4"], b"");
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        include_str!("../samples/synthetic.a4.json")
    );
}

#[test]
fn stdin_input_and_profile_file() {
    let profile = format!(
        "{}/samples/profile.example.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let out = cli(&["-", "--profile", &profile], b"INT. A - DAY\n\nOne.\n");
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.starts_with("page size: letter-54  (54 lines/page)"));
}

#[test]
fn usage_and_errors() {
    let out = cli(&[], b"");
    assert_eq!(out.status.code(), Some(2));
    let out = cli(&["--help"], b"");
    assert!(out.status.success());
    assert!(String::from_utf8(out.stdout).unwrap().contains("Usage:"));
    let out = cli(&["/nonexistent/path.fountain"], b"");
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8(out.stderr)
        .unwrap()
        .contains("cannot open"));
    let out = cli(&["-", "--bogus"], b"");
    assert_eq!(out.status.code(), Some(1));
    let out = cli(&["-"], b"\xff\xfe");
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8(out.stderr).unwrap().contains("UTF-8"));
}
