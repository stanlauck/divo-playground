// SPDX-License-Identifier: MIT OR Apache-2.0
use crate::Error;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

/// Compile to `out_dir/report.pdf`, writing `out_dir/report.typ` first.
/// Images must already be placed within out_dir. No shell or network is used.
/// Compiler stderr is drained to avoid deadlocks but retained only up to 8 KiB.
pub fn compile_pdf(
    typ_source: &str,
    out_dir: &Path,
    typst_path: Option<&Path>,
) -> Result<PathBuf, Error> {
    fs::create_dir_all(out_dir)?;
    let root = out_dir.canonicalize()?;
    let source = root.join("report.typ");
    let output = root.join("report.pdf");
    fs::write(&source, typ_source)?;
    // An earlier successful compilation must not mask a compiler that creates
    // no new output. Remove stale output before running the process.
    match fs::remove_file(&output) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(Error::Io(e)),
    }
    let mut child = Command::new(typst_path.unwrap_or(Path::new("typst")))
        .arg("compile")
        .arg("--root")
        .arg(&root)
        .arg(&source)
        .arg(&output)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::TypstMissing(
                    typst_path
                        .unwrap_or(Path::new("typst"))
                        .display()
                        .to_string(),
                )
            } else {
                Error::Io(e)
            }
        })?;
    let mut stderr = Vec::new();
    let mut buffer = [0u8; 4096];
    if let Some(mut pipe) = child.stderr.take() {
        loop {
            match pipe.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    let keep = n.min(8192usize.saturating_sub(stderr.len()));
                    stderr.extend_from_slice(&buffer[..keep]);
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(Error::Io(e));
                }
            }
        }
    }
    let status = child.wait()?;
    if !status.success() {
        return Err(Error::TypstFailed {
            status: status.code(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
        });
    }
    // Protect callers from an incorrectly configured executable reporting success.
    let mut header = [0; 4];
    let mut file = fs::File::open(&output)?;
    file.read_exact(&mut header)?;
    if &header != b"%PDF" {
        return Err(Error::Invalid("compiler output is not a PDF".into()));
    }
    Ok(output)
}
