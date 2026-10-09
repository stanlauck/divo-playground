// SPDX-License-Identifier: MIT OR Apache-2.0
use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};
static COUNT: AtomicU64 = AtomicU64::new(0);
fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pg-05-breakdown-export"))
}
#[test]
fn cli_stdin_stdout_and_help_work_offline() {
    let mut child = binary()
        .args(["csv", "-", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(include_bytes!("../samples/synthetic.json"))
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .contains("\"yard\""));
    assert!(binary().arg("--help").output().unwrap().status.success());
    assert!(!binary()
        .args(["fdx", "-", "-", "--raw-csv"])
        .output()
        .unwrap()
        .status
        .success());
}
#[test]
fn cli_does_not_overwrite_existing_output_or_open_it_for_invalid_input() {
    let folder = std::env::temp_dir().join(format!(
        "pg05-cli-{}-{}",
        std::process::id(),
        COUNT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&folder).unwrap();
    let input = folder.join("invented.json");
    let output = folder.join("sentinel.csv");
    fs::write(&input, include_bytes!("../samples/synthetic.json")).unwrap();
    fs::write(&output, "keep this agent-created sentinel").unwrap();
    let run = binary()
        .arg("csv")
        .arg(&input)
        .arg(&output)
        .output()
        .unwrap();
    assert!(!run.status.success());
    assert_eq!(
        fs::read_to_string(&output).unwrap(),
        "keep this agent-created sentinel"
    );
    fs::write(&input, "private malformed input").unwrap();
    let fresh = folder.join("must-not-exist.fdx");
    let run = binary()
        .arg("fdx")
        .arg(&input)
        .arg(&fresh)
        .output()
        .unwrap();
    assert!(!run.status.success());
    assert!(!fresh.exists());
    assert!(!String::from_utf8_lossy(&run.stderr).contains("private"));
    fs::remove_dir_all(folder).unwrap();
}
