// SPDX-License-Identifier: MIT OR Apache-2.0

//! Behaviour that is awkward to pin down with a golden file: byte-order marks,
//! CRLF endings, the hard limits, and the guarantees the CLI relies on.

use std::path::Path;

use pg_21_yarn::{
    parse, parse_sources, parse_sources_with_report, read_files, to_json, Error, FindingCode,
    NodeKind, ParseOptions, Severity, Source,
};

fn one(text: &str) -> pg_21_yarn::DialogueGraph {
    parse("memory.yarn", text).expect("a well formed script must parse")
}

#[test]
fn a_byte_order_mark_is_ignored() {
    let graph = one("\u{feff}title: Start\n---\nAda: Hello. #line:bom1\n");
    assert_eq!(graph.nodes[0].technical_name.as_deref(), Some("Start"));
    assert_eq!(graph.nodes[0].metadata["line"], 1);
    assert_eq!(graph.warnings, []);
}

#[test]
fn crlf_endings_parse_like_lf() {
    let lf = one("title: Start\n---\nAda: Hello. #line:crlf1\n-> Option.\n    Ada: Body.\n");
    let crlf =
        one("title: Start\r\n---\r\nAda: Hello. #line:crlf1\r\n-> Option.\r\n    Ada: Body.\r\n");
    assert_eq!(to_json(&lf).unwrap(), to_json(&crlf).unwrap());
}

#[test]
fn output_uses_lf_and_ends_with_a_newline() {
    let text = to_json(&one("title: Start\n---\nAda: Hello.\n")).unwrap();
    assert!(
        text.ends_with("}\n"),
        "the document must end with a newline"
    );
    assert!(!text.contains('\r'), "the document must not contain CR");
}

#[test]
fn an_unterminated_node_is_closed_at_end_of_file() {
    let graph = one("title: Start\n---\nAda: No closing delimiter.");
    assert_eq!(graph.nodes.len(), 2);
    assert_eq!(graph.warnings, []);
}

#[test]
fn a_script_with_no_nodes_yields_no_start_node() {
    let graph = one("// nothing but a comment\n");
    assert!(graph.hierarchy.is_empty());
    assert!(graph.nodes.is_empty());
}

#[test]
fn empty_containers_are_retained_with_named_entries() {
    let graph = one("title: Start\n---\n===\n\ntitle: Fallback\n---\nAda: Here.\n");
    assert_eq!(graph.nodes[0].id, "Start");
    assert!(!graph.edges.iter().any(|e| e.source == "Start"));
    assert!(graph
        .edges
        .iter()
        .any(|e| e.source == "Fallback" && e.target == "Fallback:1"));
}

#[test]
fn source_names_are_recorded_exactly_as_given() {
    let options = ParseOptions::default();
    let graph = read_files(&[Path::new("tests/fixtures/01-minimal.yarn")], &options)
        .expect("the fixture must be readable");
    assert_eq!(
        graph.packages[0].metadata["path"],
        "tests/fixtures/01-minimal.yarn"
    );
}

#[test]
fn an_oversize_file_is_rejected_before_parsing() {
    let options = ParseOptions {
        max_input_bytes: 4,
        ..ParseOptions::default()
    };
    let error = read_files(&[Path::new("tests/fixtures/01-minimal.yarn")], &options)
        .expect_err("4 bytes cannot hold this fixture");
    assert!(
        matches!(error, Error::TooLarge { ref source, .. } if source == "tests/fixtures/01-minimal.yarn"),
        "unexpected error: {error}"
    );
}

#[test]
fn the_findings_cap_is_a_hard_error_not_a_truncation() {
    let script = "title: A\n---\n<<one>>\n<<two>>\n<<three>>\n===\n";
    let options = ParseOptions {
        max_findings: 2,
        ..ParseOptions::default()
    };
    let sources = [Source {
        name: "capped.yarn".to_string(),
        text: script.to_string(),
    }];
    match parse_sources(&sources, &options) {
        Err(Error::TooManyFindings { source, limit }) => {
            assert_eq!(source, "capped.yarn");
            assert_eq!(limit, 2);
        }
        other => panic!("expected TooManyFindings, got {other:?}"),
    }
}

#[test]
fn the_node_cap_is_a_hard_error() {
    let mut script = String::from("title: A\n---\n");
    for index in 0..10 {
        script.push_str(&format!("Ada: Line {index}.\n"));
    }
    let options = ParseOptions {
        max_nodes: 4,
        ..ParseOptions::default()
    };
    let error = parse_sources(
        &[Source {
            name: "capped.yarn".to_string(),
            text: script,
        }],
        &options,
    )
    .expect_err("10 statements cannot fit in 4");
    assert!(
        matches!(error, Error::TooManyNodes { .. }),
        "unexpected error: {error}"
    );
}

#[test]
fn the_nesting_cap_reports_and_stops_instead_of_overflowing() {
    let mut script = String::from("title: Deep\n---\n");
    for depth in 0..40 {
        script.push_str(&format!("{}-> Level {depth}.\n", "    ".repeat(depth)));
    }
    let options = ParseOptions {
        max_block_depth: 8,
        ..ParseOptions::default()
    };
    let (graph, report) = parse_sources_with_report(
        &[Source {
            name: "deep.yarn".to_string(),
            text: script,
        }],
        &options,
    )
    .expect("the depth cap must be reported, not fatal");
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == FindingCode::NestingTooDeep),
        "expected a nesting_too_deep finding, got {:?}",
        report.findings
    );
    assert!(graph.nodes.len() < 40, "parsing must stop at the cap");
}

#[test]
fn zero_limits_are_rejected() {
    let options = ParseOptions {
        max_nodes: 0,
        ..ParseOptions::default()
    };
    let error = parse_sources(
        &[Source {
            name: "any.yarn".to_string(),
            text: "title: A\n---\nAda: Hello.\n".to_string(),
        }],
        &options,
    )
    .expect_err("a zero limit cannot be honoured");
    assert_eq!(error, Error::InvalidLimits);
}

#[test]
fn severity_counts_split_errors_from_warnings() {
    let (_, report) = parse_sources_with_report(
        &[Source::new(
            "memory.yarn",
            "title: A\n---\n<<custom>>\n<<jump Nowhere>>\n",
        )],
        &ParseOptions::default(),
    )
    .unwrap();
    assert_eq!(report.error_count(), 1);
    assert_eq!(report.finding_count(), 2);
    assert_eq!(report.findings[0].severity, Severity::Warning);
    assert_eq!(report.findings[0].code, FindingCode::UnknownCommand);
    assert_eq!(report.findings[1].severity, Severity::Error);
    assert_eq!(report.findings[1].code, FindingCode::MissingJumpTarget);
}

#[test]
fn a_stop_command_ends_the_branch() {
    let graph = one("title: A\n---\n<<stop>>\nAda: Never reached.\n");
    assert_eq!(graph.nodes.len(), 3);
    assert!(
        !graph.edges.iter().any(|edge| edge.source == "A:1"),
        "`<<stop>>` must not flow onwards"
    );
}

#[test]
fn option_nesting_follows_indentation() {
    let graph = one("title: A\n---\n-> Outer.\n    -> Inner.\n        Ada: Deep.\n-> Sibling.\n");
    let parents: Vec<(String, Option<String>)> = graph
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node.parent.clone()))
        .collect();
    assert_eq!(
        parents,
        [
            ("A".into(), None),
            ("A:1".into(), Some("A".into())),
            ("A:1:1".into(), Some("A:1".into())),
            ("A:1:1:1".into(), Some("A:1:1".into())),
            ("A:1:1:1:1".into(), Some("A:1:1:1".into())),
            ("A:1:1:1:1:1".into(), Some("A:1:1:1:1".into())),
            ("A:1:2".into(), Some("A:1".into())),
        ]
    );
    assert_eq!(
        graph
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Hub)
            .count(),
        2
    );
}
