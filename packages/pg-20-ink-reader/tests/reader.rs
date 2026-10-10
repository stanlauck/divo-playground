// SPDX-License-Identifier: MIT OR Apache-2.0
use ink_reader::*;
use std::io::Cursor;
fn fixture(name: &str) -> (DialogueGraph, Report) {
    let p = format!("samples/{name}.ink.json");
    read_compiled(
        Cursor::new(std::fs::read(p).unwrap()),
        &ReadOptions::default(),
    )
    .unwrap()
}
#[test]
fn knots_stitches_and_lines() {
    let (g, r) = fixture("two-knots");
    assert_eq!(r.stats.knots, 2);
    assert!(g
        .nodes
        .iter()
        .any(|n| n.id == "First" && n.kind == NodeKind::FlowFragment));
    assert!(g.nodes.iter().any(|n| n.text.as_deref() == Some("One")));
    assert_eq!(r.stats.lines, 2);
}
#[test]
fn choices_flags_and_entries() {
    let (g, r) = fixture("choices");
    assert_eq!(r.stats.choices, 1);
    assert_eq!(g.choices.len(), 1);
    let o = g.nodes.iter().find(|n| n.source_type == "Option").unwrap();
    assert_eq!(o.menu_text.as_deref(), Some("Go north"));
    assert_eq!(o.properties["flags"][0], "has_start_content");
}
#[test]
fn unsupported_report_and_raw() {
    let (g, r) = fixture("unsupported");
    assert_eq!(r.unsupported[0].feature, "tunnels");
    assert!(g
        .nodes
        .iter()
        .any(|n| n.source_type == "Unsupported" && n.properties.contains_key("raw")));
}
#[test]
fn malformed_and_limits() {
    let e = read_compiled(
        Cursor::new(br#"{"inkVersion":20,"root":{}}"#),
        &ReadOptions::default(),
    );
    assert!(e.is_err());
    let o = ReadOptions {
        max_bytes: 2,
        ..ReadOptions::default()
    };
    assert!(read_compiled(Cursor::new(b"{}"), &o).is_err());
}
#[test]
fn deterministic() {
    let a = fixture("two-knots");
    let b = fixture("two-knots");
    assert_eq!(
        serde_json::to_vec_pretty(&a.0).unwrap(),
        serde_json::to_vec_pretty(&b.0).unwrap()
    );
}
#[test]
fn pg03_invariants_basic() {
    let (g, _) = fixture("choices");
    let ids = g
        .nodes
        .iter()
        .map(|n| n.id.clone())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(ids.len(), g.nodes.len());
    for n in &g.nodes {
        assert!(n
            .input_pins
            .iter()
            .all(|p| g.pins.iter().any(|x| x.id == *p && x.owner == n.id)));
        assert!(n
            .output_pins
            .iter()
            .all(|p| g.pins.iter().any(|x| x.id == *p && x.owner == n.id)));
    }
    for e in &g.edges {
        assert!(ids.contains(&e.source) && ids.contains(&e.target));
        assert!(g.pins.iter().any(|p| Some(&p.id) == e.source_pin.as_ref()));
    }
}

#[test]
fn conditional_and_variables_are_retained() {
    let (graph, report) = fixture("conditional");
    assert_eq!(report.stats.lines, 1);
    let condition = graph
        .nodes
        .iter()
        .find(|node| node.source_type == "Condition")
        .unwrap();
    assert_eq!(condition.script.as_ref().unwrap().text, "1");
    assert!(graph
        .edges
        .iter()
        .any(|edge| edge.source == condition.id && edge.label.as_deref() == Some("true")));
    let (graph, report) = fixture("variables");
    assert_eq!(report.stats.variables, 1);
    assert_eq!(graph.variables[0].name, "score");
    assert_eq!(graph.variables[0].kind, VariableKind::Integer);
}

#[test]
fn cli_strict_rejects_unsupported() {
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_ink2graph"))
        .arg("samples/unsupported.ink.json")
        .arg("--strict")
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(1));
}
