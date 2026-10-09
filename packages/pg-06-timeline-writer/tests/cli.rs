// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use timeline_writer::*;

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "pg06-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_timeline-writer"))
}

#[test]
fn cli_converts_without_overwriting_and_hides_paths() {
    let tmp = Temp::new();
    let input = tmp.0.join("invented-private-input.json");
    let output = tmp.0.join("invented-private-output.fcpxml");
    fs::write(&input, include_str!("../samples/synthetic.json")).unwrap();
    let run = || {
        command()
            .args(["--from", "json", "--to", "fcpxml", "--input"])
            .arg(&input)
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap()
    };
    assert!(run().status.success());
    let before = fs::read(&output).unwrap();
    assert_eq!(
        from_fcpxml(std::str::from_utf8(&before).unwrap()).unwrap(),
        from_json(include_str!("../samples/synthetic.json")).unwrap()
    );
    let failure = run();
    assert!(!failure.status.success());
    assert_eq!(fs::read(&output).unwrap(), before);
    assert!(
        !String::from_utf8(failure.stderr)
            .unwrap()
            .contains("invented-private")
    );
    let bad = tmp.0.join("bad.json");
    let absent = tmp.0.join("must-not-exist.otio");
    fs::write(&bad, b"not JSON").unwrap();
    let failure = command()
        .args(["--from", "json", "--to", "otio", "--input"])
        .arg(&bad)
        .arg("--output")
        .arg(&absent)
        .output()
        .unwrap();
    assert!(!failure.status.success() && !absent.exists());
    let failure = command()
        .args(["--from", "json", "--to", "otio", "--input"])
        .arg(&absent)
        .args(["--output", "-"])
        .output()
        .unwrap();
    assert!(
        !failure.status.success()
            && !String::from_utf8(failure.stderr)
                .unwrap()
                .contains("must-not-exist")
    );
}

#[test]
fn cli_stdin_stdout_and_argument_validation() {
    let mut child = command()
        .args([
            "--from", "json", "--to", "otio", "--input", "-", "--output", "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(include_bytes!("../samples/synthetic.json"))
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    assert_eq!(
        from_otio(std::str::from_utf8(&out.stdout).unwrap()).unwrap(),
        from_json(include_str!("../samples/synthetic.json")).unwrap()
    );
    for args in [
        vec![],
        vec!["--from", "xml"],
        vec![
            "--from", "json", "--to", "bad", "--input", "-", "--output", "-",
        ],
        vec![
            "--from", "json", "--from", "otio", "--input", "-", "--output", "-",
        ],
    ] {
        let out = command().args(args).output().unwrap();
        assert!(!out.status.success());
        assert!(
            String::from_utf8(out.stderr)
                .unwrap()
                .starts_with("invalid_arguments")
        );
    }
    let help = command().arg("--help").output().unwrap();
    assert!(help.status.success() && String::from_utf8(help.stdout).unwrap().contains("Usage:"));
}
