// SPDX-License-Identifier: MIT OR Apache-2.0

//! Golden-file and CLI tests. All samples are invented.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use subtitle_writer::{check, parse, report_json, write, Checks, Format};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
fn sample(name: &str) -> String {
    std::fs::read_to_string(root().join("samples").join(name)).unwrap()
}
fn cli(args: &[&str], stdin: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_subtitle-writer"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    child.wait_with_output().unwrap()
}
fn out(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}
fn err(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "pg17-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn path(&self, name: &str) -> String {
        self.0.join(name).to_str().unwrap().to_owned()
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn sample_path() -> String {
    root()
        .join("samples/synthetic.json")
        .to_str()
        .unwrap()
        .to_owned()
}

#[test]
fn golden_outputs_match_the_library() {
    let document = parse(&sample("synthetic.json")).unwrap();
    assert_eq!(
        write(&document, Format::Srt).unwrap(),
        sample("synthetic.srt")
    );
    assert_eq!(
        write(&document, Format::Vtt).unwrap(),
        sample("synthetic.vtt")
    );
    assert_eq!(
        write(&document, Format::Ttml).unwrap(),
        sample("synthetic.ttml")
    );
    let violations = check(&document, &Checks::default()).unwrap();
    assert_eq!(report_json(&violations), sample("synthetic.report.json"));
}

#[test]
fn golden_sample_exercises_speakers_cyrillic_and_markup() {
    let json = sample("synthetic.json");
    assert!(json.contains("ОЛЬГА") && json.contains('<') && json.contains('&'));
    let srt = sample("synthetic.srt");
    assert!(srt.contains("MIRA: Did you hear that?"));
    assert!(srt.contains("<dropping> fast") && srt.contains("twice & wait"));
    let vtt = sample("synthetic.vtt");
    assert!(vtt.starts_with("WEBVTT\n"));
    assert!(vtt.contains("<v ОЛЬГА>Тише."));
    assert!(vtt.contains("&lt;dropping&gt;") && vtt.contains("&amp; wait"));
    let ttml = sample("synthetic.ttml");
    assert!(ttml.contains("ttp:timeBase=\"media\""));
    assert!(ttml.contains("<ttm:name type=\"full\">ОЛЬГА</ttm:name>"));
    assert!(ttml.contains("&lt;dropping&gt;"));
    let report: Vec<serde_json::Value> =
        serde_json::from_str(&sample("synthetic.report.json")).unwrap();
    let rules: Vec<&str> = report.iter().map(|v| v["rule"].as_str().unwrap()).collect();
    assert_eq!(
        rules,
        [
            "too_short",
            "line_too_long",
            "reading_speed",
            "overlap",
            "too_long",
            "line_too_long",
            "too_many_lines"
        ]
    );
}

#[test]
fn golden_files_are_lf_and_bom_free() {
    for name in [
        "synthetic.srt",
        "synthetic.vtt",
        "synthetic.ttml",
        "synthetic.report.json",
    ] {
        let text = sample(name);
        assert!(!text.contains('\r'), "{name}");
        assert!(!text.starts_with('\u{feff}'), "{name}");
        assert!(text.ends_with('\n'), "{name}");
    }
}

#[test]
fn cli_writes_each_format_to_stdout_and_report_to_stderr() {
    for (format, golden) in [
        ("srt", "synthetic.srt"),
        ("vtt", "synthetic.vtt"),
        ("ttml", "synthetic.ttml"),
    ] {
        let output = cli(&[&sample_path(), "--format", format], b"");
        assert!(output.status.success(), "{}", err(&output));
        assert_eq!(out(&output), sample(golden));
        assert_eq!(err(&output), sample("synthetic.report.json"));
    }
}

#[test]
fn cli_without_format_prints_report_to_stdout() {
    let output = cli(&[&sample_path()], b"");
    assert!(output.status.success());
    assert_eq!(out(&output), sample("synthetic.report.json"));
    assert!(err(&output).is_empty());
}

#[test]
fn cli_strict_exits_one_on_violations_and_zero_when_clean() {
    let output = cli(&[&sample_path(), "--strict"], b"");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(out(&output), sample("synthetic.report.json"));
    let output = cli(&[&sample_path(), "--format", "srt", "--strict"], b"");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        out(&output),
        sample("synthetic.srt"),
        "output is still written"
    );
    let clean = br#"{"version":1,"cues":[{"id":"a","lines":["Hi."],"start_ms":0,"end_ms":1500}]}"#;
    let output = cli(&["-", "--strict", "--format", "vtt"], clean);
    assert!(output.status.success());
    assert!(err(&output).is_empty(), "no report on stderr when clean");
    assert_eq!(
        out(&output),
        "WEBVTT\n\na\n00:00:00.000 --> 00:00:01.500\nHi.\n"
    );
}

#[test]
fn cli_writes_output_and_report_files() {
    let temp = Temp::new();
    let output = cli(
        &[
            &sample_path(),
            "--format",
            "ttml",
            "-o",
            &temp.path("out.ttml"),
            "--report",
            &temp.path("report.json"),
        ],
        b"",
    );
    assert!(output.status.success(), "{}", err(&output));
    assert!(out(&output).is_empty() && err(&output).is_empty());
    assert_eq!(
        std::fs::read_to_string(temp.path("out.ttml")).unwrap(),
        sample("synthetic.ttml")
    );
    assert_eq!(
        std::fs::read_to_string(temp.path("report.json")).unwrap(),
        sample("synthetic.report.json")
    );
    // --report alone (no --format) writes the report file and nothing to stdout.
    let output = cli(&[&sample_path(), "--report", &temp.path("only.json")], b"");
    assert!(output.status.success());
    assert!(out(&output).is_empty());
    assert_eq!(
        std::fs::read_to_string(temp.path("only.json")).unwrap(),
        sample("synthetic.report.json")
    );
}

#[test]
fn cli_limit_options_change_the_report() {
    let output = cli(
        &[
            &sample_path(),
            "--max-cps",
            "100",
            "--max-line-length",
            "80",
            "--max-lines",
            "3",
            "--min-duration-ms",
            "500",
            "--max-duration-ms",
            "8000",
        ],
        b"",
    );
    assert!(output.status.success());
    let report: Vec<serde_json::Value> = serde_json::from_str(&out(&output)).unwrap();
    let rules: Vec<&str> = report.iter().map(|v| v["rule"].as_str().unwrap()).collect();
    assert_eq!(rules, ["overlap"]);
}

#[test]
fn cli_rejects_bad_arguments_with_status_two() {
    for args in [
        &[][..],
        &["--format", "srt"][..],
        &[&sample_path(), "--format", "ass"][..],
        &[&sample_path(), "--format"][..],
        &[&sample_path(), "-o", "x.srt"][..],
        &[&sample_path(), "--max-cps", "abc"][..],
        &[&sample_path(), "--max-cps", "0"][..],
        &[&sample_path(), "--min-duration-ms", "9000"][..],
        &[&sample_path(), "--bogus"][..],
        &[sample_path().as_str(), sample_path().as_str()][..],
    ] {
        let output = cli(args, b"");
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(err(&output).starts_with("error: "), "{args:?}");
    }
}

#[test]
fn cli_reports_input_errors_with_status_two() {
    let output = cli(&["-"], b"{not json");
    assert_eq!(output.status.code(), Some(2));
    assert!(err(&output).contains("invalid JSON at line 1"));
    let output = cli(
        &["-"],
        "{\"version\":1,\"cues\":[{\"id\":\"a\",\"lines\":[\"x\"],\"start_ms\":0,\"end_ms\":0}]}"
            .as_bytes(),
    );
    assert!(
        output.status.success(),
        "non-positive duration is a violation, not an error"
    );
    let output = cli(
        &["-", "--format", "srt"],
        "{\"version\":1,\"cues\":[{\"id\":\"a\",\"lines\":[\"x\"],\"start_ms\":0,\"end_ms\":0}]}"
            .as_bytes(),
    );
    assert_eq!(output.status.code(), Some(2), "...but it cannot be written");
    assert!(err(&output).contains("cues[0].end_ms"));
    let missing = Path::new(env!("CARGO_MANIFEST_DIR")).join("samples/does-not-exist.json");
    let output = cli(&[missing.to_str().unwrap()], b"");
    assert_eq!(output.status.code(), Some(2));
    assert!(err(&output).contains("cannot open input"));
}

#[test]
fn cli_help_exits_zero() {
    let output = cli(&["--help"], b"");
    assert!(output.status.success());
    assert!(out(&output).contains("--strict"));
}
