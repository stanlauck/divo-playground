// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(dead_code)]
use report_renderer::{from_json, Report};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

pub const SAMPLE: &str = include_str!("../../samples/synthetic-report.json");
pub fn sample() -> Report {
    from_json(SAMPLE).expect("valid fixture")
}
pub fn minimal() -> Report {
    from_json(
        r#"{"version":1,"title":"Example","sections":[{"id":"one","title":"One","blocks":[]}]}"#,
    )
    .unwrap()
}
pub struct Temp(pub PathBuf);
impl Temp {
    pub fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        loop {
            let path = std::env::temp_dir().join(format!(
                "report-renderer-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("test directory: {e}"),
            }
        }
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
