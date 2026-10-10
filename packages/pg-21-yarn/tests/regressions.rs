// SPDX-License-Identifier: MIT OR Apache-2.0

use pg_21_yarn::{
    parse, parse_sources_with_report, DialogueGraph, Error, FindingCode, NodeKind, ParseOptions,
    ParseReport, Source, VariableKind,
};
use std::collections::HashSet;
use std::process::Command;

fn run(text: &str) -> (DialogueGraph, ParseReport) {
    parse_sources_with_report(
        &[Source::new("synthetic.yarn", text)],
        &ParseOptions::default(),
    )
    .unwrap()
}
fn unique(graph: &DialogueGraph) {
    let ids: Vec<_> = graph
        .nodes
        .iter()
        .map(|n| &n.id)
        .chain(graph.pins.iter().map(|p| &p.id))
        .collect();
    assert_eq!(ids.iter().collect::<HashSet<_>>().len(), ids.len());
}
fn has(graph: &DialogueGraph, source: &str, target: &str, label: Option<&str>) -> bool {
    graph
        .edges
        .iter()
        .any(|e| e.source == source && e.target == target && e.label.as_deref() == label)
}

// Devin 4235066966.
#[test]
fn duplicate_node_ids_with_suffix_titles() {
    for explicit in ["A#2", "A~2", "A:1", "A#in", "A#out"] {
        let (g, report) = run(&format!("title: A\n---\nOne.\n===\ntitle: A\n---\nTwo.\n===\ntitle: {explicit}\n---\nThree.\n===\n"));
        unique(&g);
        assert!(report
            .findings
            .iter()
            .any(|f| f.code == FindingCode::DuplicateNodeTitle));
        assert_eq!(g.nodes[2].id, "A~2");
    }
}

// Devin 4235067086.
#[test]
fn parser_output_is_compatible_with_pg03_graph() {
    let (g, report) = run("title: Start\n---\nAda: Synthetic. #line:one #calm\n===\n");
    let json = serde_json::to_value(&g).unwrap();
    let expected: HashSet<_> = [
        "version",
        "text_mode",
        "metadata",
        "definitions",
        "packages",
        "nodes",
        "pins",
        "edges",
        "choices",
        "variable_namespaces",
        "variables",
        "hierarchy",
        "warnings",
    ]
    .into_iter()
    .collect();
    assert_eq!(
        json.as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<HashSet<_>>(),
        expected
    );
    let roundtrip: DialogueGraph = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(roundtrip, g);
    assert_eq!(g.nodes[1].properties["line_id"], "one");
    assert_eq!(g.nodes[1].properties["tags"], serde_json::json!(["calm"]));
    assert!(json.get("errors").is_none());
    assert!(report.findings.is_empty());
}

// Devin 4235067215.
#[test]
fn first_choice_hides_no_options() {
    let (g, _) =
        run("title: Start\n---\n-> Left\n-> Right\n===\ntitle: Caller\n---\n<<jump Start>>\n===\n");
    assert_eq!(g.nodes[1].kind, NodeKind::Hub);
    assert!(has(&g, "Start", "Start:1", None));
    assert!(has(&g, "Caller:1", "Start", None));
    assert_eq!(g.choices.len(), 2);
    assert!(g.choices.iter().all(|c| c.source == "Start:1"));
}

// Devin 4235067330.
#[test]
fn options_inside_conditions_keep_clause_labels() {
    let (g, _) = run("title: Start\n---\n<<if $x>>\n    -> Yes\n        -> Nested\n<<elseif $y>>\n    -> Maybe\n<<else>>\n    -> No\n<<endif>>\n===\n");
    let condition = &g.nodes[1];
    let hubs: Vec<_> = g
        .nodes
        .iter()
        .filter(|n| n.parent.as_deref() == Some(&condition.id))
        .collect();
    assert_eq!(hubs.len(), 3);
    for (hub, label) in hubs.iter().zip(["if", "elseif:$y", "else"]) {
        assert_eq!(hub.properties["clause"], label);
        assert!(g
            .nodes
            .iter()
            .filter(|n| n.parent.as_deref() == Some(&hub.id))
            .all(|n| n.properties["clause"] == label));
        assert!(has(&g, &condition.id, &hub.id, Some(label)));
    }
}

// Devin 4235067449.
#[test]
fn variable_description_does_not_leak_from_previous_node() {
    let (g, _) =
        run("title: A\n---\n/// Old description\n===\ntitle: B\n---\n<<declare $x = 1>>\n===\n");
    assert_eq!(g.variables[0].description, None);
}

// Devin 4235067584: exercise the actual strict CLI, not a model of it.
#[test]
fn comments_after_commands_are_accepted_in_strict_mode() {
    let dir = temp_dir("strict-comments");
    let input = dir.join("synthetic.yarn");
    std::fs::write(&input, "title: Start\n---\n<<set $x = 1>> // comment\n<<wait 2>> #quiet // comment\n<<stop>>\n===\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_yarn2graph"))
        .arg(&input)
        .arg("--strict")
        .arg("--report")
        .arg(dir.join("report.json"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let graph: DialogueGraph = serde_json::from_slice(&output.stdout).unwrap();
    let report: ParseReport =
        serde_json::from_slice(&std::fs::read(dir.join("report.json")).unwrap()).unwrap();
    assert!(report.findings.is_empty());
    assert!(graph.warnings.is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

// Devin 4235067725.
#[test]
fn declared_float_type_is_not_lost_for_integer_literal() {
    let (g, report) = run(
        "title: A\n---\n<<declare $speed = 1 as float>>\n<<declare $pace = 2 as double>>\n===\n",
    );
    assert!(report.findings.is_empty());
    assert!(g.variables.iter().all(|v| v.kind == VariableKind::Float));
    assert_eq!(g.variables[0].raw_value, "1");
    assert_eq!(g.variables[0].value, 1);
    assert_eq!(g.nodes.len(), 1); // declarations populate variables only
}

// Qodo 4235085098: collision in the opposite title order.
#[test]
fn duplicate_titles_do_not_corrupt_graph_links() {
    let (g, _) = run("title: A~2\n---\nFirst.\n===\ntitle: A\n---\nSecond.\n===\ntitle: A\n---\nThird.\n<<jump A>>\n===\n");
    unique(&g);
    assert!(g.nodes.iter().any(|n| n.id == "A~3"));
    assert!(has(&g, "A~3:2", "A", None));
}

// Qodo 4235085112, EOF and multiple files.
#[test]
fn earlier_comments_do_not_label_later_variables() {
    let sources = [
        Source::new("a.yarn", "title: A\n---\n/// Old description"),
        Source::new(
            "b.yarn",
            "title: B\n---\n/// Local description\n<<declare $x = 1>>\n===\n",
        ),
    ];
    let (g, report) = parse_sources_with_report(&sources, &ParseOptions::default()).unwrap();
    assert!(report.findings.is_empty());
    assert_eq!(
        g.variables[0].description.as_deref(),
        Some("Local description")
    );
}

// Qodo 4235085116.
#[test]
fn a_final_option_does_not_crash_the_parser() {
    for end in ["", "\n===\n"] {
        let (g, report) = run(&format!("title: Start\n---\n-> Choose this{end}"));
        assert!(report.findings.is_empty());
        assert_eq!(g.choices.len(), 1);
        assert!(has(&g, "Start", "Start:1", None));
    }
}

// Qodo 4235085124.
#[test]
fn false_conditions_keep_the_next_statement() {
    for elseif in ["", "<<elseif $y>>\nSecond branch.\n"] {
        let (g, _) = run(&format!(
            "title: A\n---\n<<if $x>>\nFirst branch.\n{elseif}<<endif>>\nAfter.\n===\n"
        ));
        assert!(has(&g, "A:1", "A:2", Some("else")));
        assert!(has(&g, "A:1:1", "A:2", None));
        if !elseif.is_empty() {
            assert!(has(&g, "A:1", "A:1:2", Some("elseif:$y")));
        }
    }
    let (g, _) =
        run("title: A\n---\n-> Option\n    <<if $x>>\n    Branch.\n    <<endif>>\nAfter.\n===\n");
    let condition = g
        .nodes
        .iter()
        .find(|n| n.kind == NodeKind::Condition)
        .unwrap();
    assert!(has(&g, &condition.id, "A:2", Some("else")));
}

// Qodo 4235085137.
#[test]
fn later_markup_does_not_go_unmarked() {
    let (g, _) = run("title: A\n---\nLiteral [ before [wave]hi[/wave].\n===\n");
    assert_eq!(g.nodes[1].properties["markup"], true);
    assert_eq!(
        g.nodes[1].text.as_deref(),
        Some("Literal [ before [wave]hi[/wave].")
    );
}

// Qodo 4235085144.
#[test]
fn in_memory_parsing_enforces_the_input_size_limit() {
    let source = Source::new("a.yarn", "title: A\n---\nSynthetic.\n");
    let options = ParseOptions {
        max_input_bytes: source.text.len() - 1,
        ..ParseOptions::default()
    };
    assert!(matches!(
        parse_sources_with_report(&[source], &options),
        Err(Error::TooLarge { .. })
    ));
    let text = "x".repeat(ParseOptions::default().max_input_bytes + 1);
    assert!(matches!(
        parse("large.yarn", &text),
        Err(Error::TooLarge { .. })
    ));
}

// Qodo 4235085155: a zero-length device with an infinite stream.
#[cfg(unix)]
#[test]
fn reading_a_pipe_or_device_cannot_exhaust_memory() {
    let options = ParseOptions {
        max_input_bytes: 64,
        ..ParseOptions::default()
    };
    assert!(matches!(
        pg_21_yarn::read_files(&["/dev/zero"], &options),
        Err(Error::TooLarge {
            bytes: 65,
            limit: 64,
            ..
        })
    ));
}

// Qodo 4235085166.
#[test]
fn modulo_assignments_keep_a_valid_variable_name() {
    let (g, report) = run("title: A\n---\n<<set $x %= 2>>\n===\n");
    assert!(report.findings.is_empty());
    assert_eq!(g.nodes[1].properties["operator"], "%=");
    assert_eq!(g.nodes[1].properties["variable"], "x");
    for invalid in [
        "$x <= 3",
        "$x >= 3",
        "$x == 3",
        "$x != 3",
        "$x bad = 3",
        "$1x = 3",
        "$x =",
    ] {
        let (_, report) = run(&format!("title: A\n---\n<<set {invalid}>>\n===\n"));
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == FindingCode::BadCommandSyntax),
            "{invalid}"
        );
    }
}

// Qodo 4235085173.
#[test]
fn jumping_to_an_empty_node_is_valid() {
    let (g, report) =
        run("title: A\n---\n<<jump Empty>>\n===\ntitle: Empty\n---\n// only a comment\n===\n");
    assert!(report.findings.is_empty());
    assert!(g.warnings.is_empty());
    assert!(has(&g, "A:1", "Empty", None));
}

// Qodo 4235085179.
#[test]
fn expression_jump_targets_are_not_flagged_as_missing_nodes() {
    let (g, report) = run("title: A\n---\n<<jump {$destination}>>\nAfter.\n===\n");
    assert!(report.findings.is_empty());
    assert!(g.warnings.is_empty());
    assert_eq!(g.nodes[1].properties["target"], "{$destination}");
    assert!(!g.edges.iter().any(|e| e.source == "A:1"));
}

// Qodo 4235085185.
#[test]
fn comments_after_commands_raise_no_warnings() {
    let (_, report) = run("title: A\n---\n<<wait 1>> // go\n<<stop>> #quiet // done\n===\n");
    assert!(report.findings.is_empty());
    let (_, report) = run("title: A\n---\n<<wait 1>> extra text // comment\n===\n");
    assert_eq!(
        report.findings[0].code,
        FindingCode::TrailingContentAfterCommand
    );
}

// Qodo 4235085192.
#[test]
fn task_board_row_has_review_status_and_pr_link() {
    let board = std::fs::read_to_string("../../TASKS.md").unwrap();
    let row = board
        .lines()
        .find(|line| line.starts_with("| PG-21 |"))
        .unwrap();
    assert!(row.ends_with("| review | codex-sol, 2026-10-10 | #43 |"));
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("pg21-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    path
}
