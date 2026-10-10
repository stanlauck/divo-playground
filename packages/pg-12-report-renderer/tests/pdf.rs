// SPDX-License-Identifier: MIT OR Apache-2.0
mod common;
use common::*;
use report_renderer::*;
use std::{fs, path::Path, process::Command};

#[test]
fn missing_compiler_has_typed_error() {
    let t = Temp::new();
    let error = compile_pdf("example", &t.0, Some(&t.0.join("absent"))).unwrap_err();
    assert!(matches!(error, Error::TypstMissing(_)));
    assert_eq!(
        fs::read_to_string(t.0.join("report.typ")).unwrap(),
        "example"
    );
}

#[cfg(unix)]
fn fake(temp: &Temp, body: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = temp.0.join("compiler");
    fs::write(
        &path,
        format!("#!/bin/sh\n# SPDX-License-Identifier: MIT OR Apache-2.0\n{body}\n"),
    )
    .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    path
}
#[cfg(unix)]
#[test]
fn compiler_stderr_is_bounded_and_status_captured() {
    let t = Temp::new();
    let path = fake(
        &t,
        "i=0\nwhile [ \"$i\" -lt 12000 ]; do printf 'x' >&2; i=$((i+1)); done\nexit 7",
    );
    let error = compile_pdf("text", &t.0, Some(&path)).unwrap_err();
    match error {
        Error::TypstFailed { status, stderr } => {
            assert_eq!(status, Some(7));
            assert_eq!(stderr.len(), 8192);
        }
        e => panic!("{e:?}"),
    }
}
#[cfg(unix)]
#[test]
fn successful_compiler_without_pdf_is_error() {
    let t = Temp::new();
    let path = fake(&t, "exit 0");
    assert!(compile_pdf("text", &t.0, Some(&path)).is_err());
}
#[cfg(unix)]
#[test]
fn successful_compiler_with_invalid_header_is_error() {
    let t = Temp::new();
    let path = fake(&t, "printf 'invalid' > \"$5\"");
    assert!(matches!(
        compile_pdf("text", &t.0, Some(&path)),
        Err(Error::Invalid(_))
    ));
}
#[cfg(unix)]
#[test]
fn compiler_arguments_support_spaces_without_shell_interpretation() {
    let t = Temp::new();
    let path=fake(&t,"[ \"$1\" = compile ] || exit 2\n[ \"$2\" = --root ] || exit 3\n[ -d \"$3\" ] || exit 4\n[ -f \"$4\" ] || exit 5\nprintf '%%PDF fake' > \"$5\"");
    let renamed = t.0.join("fake compiler ; literal");
    fs::rename(path, &renamed).unwrap();
    let dir = t.0.join("out with spaces ; literal");
    let pdf = compile_pdf("text", &dir, Some(&renamed)).unwrap();
    assert_eq!(pdf, dir.join("report.pdf"));
}

/// The only test that uses an actual optional Typst installation. All other
/// compiler tests use local fake executables and never require that tool.
#[test]
fn sample_compiles_to_pdf_when_typst_is_available() {
    match Command::new("typst").arg("--version").output() {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("SKIP: typst is absent from PATH");
            return;
        }
        Err(e) => panic!("typst detection failed: {e}"),
        Ok(out) => assert!(out.status.success(), "typst --version failed"),
    }
    let t = Temp::new();
    let mut report = sample();
    fs::write(t.0.join("lamp.svg"),"<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"80\" height=\"40\"><rect width=\"80\" height=\"40\" fill=\"#dda833\"/></svg>").unwrap();
    report.sections[1].blocks.push(Block::Image {
        path: "lamp.svg".into(),
        caption: Some("Existing synthetic image".into()),
        width_percent: Some(25.0),
    });
    report.sections[0].blocks.push(Block::Paragraph {
        text: "Literal # *_ @ <> $ ` ~ // \\ \"\t\nПроверка".into(),
    });
    let options = RenderOptions {
        include_optional: true,
        toc: true,
        page: PageSize::Letter,
        image_root: Some(t.0.clone()),
        ..Default::default()
    };
    let rendered = render_typst(&report, &options);
    let pdf = compile_pdf(&rendered.text, &t.0, None).unwrap();
    assert!(fs::read(pdf).unwrap().starts_with(b"%PDF"));
    // Verify warning-free compilation as well as the library's error handling.
    let result = Command::new("typst")
        .arg("compile")
        .arg("--root")
        .arg(&t.0)
        .arg(t.0.join("report.typ"))
        .arg(t.0.join("checked.pdf"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        result.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(Path::new(&t.0.join("checked.pdf")).is_file());
}
