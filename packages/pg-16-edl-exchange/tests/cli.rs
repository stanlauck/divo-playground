// SPDX-License-Identifier: MIT OR Apache-2.0

use edl_exchange::{from_edl, from_json};
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

fn cli(args: &[&str], input: &[u8]) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_edl-exchange"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}
struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "pg16-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
const INPUT: &str = r#"{"version":1,"frame_rate":24,"shots":[{"id":"ONE","duration_frames":24}]}"#;

#[test]
fn stdin_stdout_conversion_has_no_diagnostics_on_success() {
    let edl = cli(
        &[
            "--from", "json", "--to", "edl", "--input", "-", "--output", "-",
        ],
        INPUT.as_bytes(),
    );
    assert!(edl.status.success());
    assert!(edl.stderr.is_empty());
    assert_eq!(
        from_edl(std::str::from_utf8(&edl.stdout).unwrap(), None).unwrap(),
        from_json(INPUT).unwrap()
    );
    let json = cli(
        &[
            "--from", "edl", "--to", "json", "--input", "-", "--output", "-",
        ],
        &edl.stdout,
    );
    assert!(json.status.success());
    assert_eq!(
        from_json(std::str::from_utf8(&json.stdout).unwrap()).unwrap(),
        from_json(INPUT).unwrap()
    );
}

#[test]
fn external_fps_is_required_and_validated_as_integer_or_rational() {
    let text = b"001 CAM V C 00:00:00:00 00:00:00:01 00:00:00:00 00:00:00:01\n";
    let args = [
        "--from", "edl", "--to", "json", "--input", "-", "--output", "-",
    ];
    assert!(String::from_utf8_lossy(&cli(&args, text).stderr).contains("missing_frame_rate"));
    for rate in ["24", "24000/1001", "30000/1001"] {
        let mut a = args.to_vec();
        a.extend(["--fps", rate]);
        assert!(cli(&a, text).status.success());
    }
    for rate in ["23.976", "24/0", "24/1/1", "-24", "60", "+24"] {
        let mut a = args.to_vec();
        a.extend(["--fps", rate]);
        assert!(!cli(&a, text).status.success());
    }
}

#[test]
fn files_are_create_new_and_invalid_input_does_not_create_output() {
    let temp = Temp::new();
    let input = temp.0.join("input.json");
    let output = temp.0.join("output.edl");
    std::fs::write(&input, INPUT).unwrap();
    let args = [
        "--from",
        "json",
        "--to",
        "edl",
        "--input",
        input.to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
    ];
    assert!(cli(&args, b"").status.success());
    let original = std::fs::read(&output).unwrap();
    let second = cli(&args, b"");
    assert!(!second.status.success());
    assert!(String::from_utf8_lossy(&second.stderr).contains("already_exists at output"));
    assert_eq!(std::fs::read(&output).unwrap(), original);
    std::fs::write(&input, "INVALID PRIVATE INPUT").unwrap();
    let invalid_output = temp.0.join("absent.edl");
    let bad = cli(
        &[
            "--from",
            "json",
            "--to",
            "edl",
            "--input",
            input.to_str().unwrap(),
            "--output",
            invalid_output.to_str().unwrap(),
        ],
        b"",
    );
    assert!(!bad.status.success());
    assert!(!invalid_output.exists());
    assert!(!String::from_utf8_lossy(&bad.stderr).contains("PRIVATE"));
}

#[test]
fn missing_input_reports_safe_kind_and_side_without_filename() {
    let temp = Temp::new();
    let missing = temp.0.join("PRIVATE-NAME.json");
    let output = cli(
        &[
            "--from",
            "json",
            "--to",
            "edl",
            "--input",
            missing.to_str().unwrap(),
            "--output",
            "-",
        ],
        b"",
    );
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(error.contains("not_found at input"));
    assert!(!error.contains("PRIVATE-NAME"));
    assert!(output.stdout.is_empty());
}

#[test]
fn help_duplicate_flags_unknown_formats_and_json_fps_are_handled() {
    let help = cli(&["--help"], b"");
    assert!(help.status.success());
    assert!(help.stderr.is_empty());
    for args in [
        vec![
            "--from", "json", "--from", "edl", "--input", "-", "--output", "-",
        ],
        vec![
            "--from", "xml", "--to", "edl", "--input", "-", "--output", "-",
        ],
        vec![
            "--from", "json", "--to", "edl", "--input", "-", "--output", "-", "--fps", "24",
        ],
        vec!["--help", "extra"],
    ] {
        let error = cli(&args, INPUT.as_bytes());
        assert!(!error.status.success());
        assert!(String::from_utf8_lossy(&error.stderr).contains("invalid_arguments"));
    }
}
