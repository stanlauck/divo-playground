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

fn story() -> (DialogueGraph, Report) {
    read_compiled(
        Cursor::new(std::fs::read("samples/story.ink.json").unwrap()),
        &ReadOptions {
            package_name: "story.ink".into(),
            ..ReadOptions::default()
        },
    )
    .unwrap()
}

#[test]
fn story_has_expected_knot_and_stitch_parents() {
    let (g, r) = story();
    assert_eq!((r.stats.knots, r.stats.stitches), (2, 1));
    assert_eq!(
        g.nodes
            .iter()
            .find(|n| n.id == "north.entry")
            .unwrap()
            .parent
            .as_deref(),
        Some("north")
    );
    assert!(g
        .nodes
        .iter()
        .any(|n| n.id == "start" && n.parent.is_none()));
}
#[test]
fn story_choice_texts() {
    let (g, _) = story();
    let mut t: Vec<_> = g
        .nodes
        .iter()
        .filter_map(|n| n.menu_text.as_deref())
        .collect();
    t.sort();
    assert_eq!(t, ["Go north", "Go south"]);
}
#[test]
fn story_choice_output_text_is_preserved() {
    let (g, _) = story();
    let south = g
        .nodes
        .iter()
        .find(|n| n.menu_text.as_deref() == Some("Go south"))
        .unwrap();
    assert_eq!(south.text.as_deref(), Some("You went south."));
}
#[test]
fn story_choice_children() {
    let (g, _) = story();
    assert!(g.nodes.iter().any(|n| n
        .parent
        .as_deref()
        .is_some_and(|p| p.starts_with("start:2:"))
        && n.source_type == "Line"));
}
#[test]
fn story_choice_flags() {
    let (g, _) = story();
    let n = g
        .nodes
        .iter()
        .find(|n| n.menu_text.as_deref() == Some("Go north"))
        .unwrap();
    assert_eq!(n.properties["flags"][0], "has_start_content");
    assert!(n.properties["flags"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "once_only"));
}
#[test]
fn story_north_jump_edge() {
    let (g, _) = story();
    let n = g
        .nodes
        .iter()
        .find(|n| n.parent.as_deref() == Some("start:2:1") && n.source_type == "Divert")
        .unwrap();
    assert!(g
        .edges
        .iter()
        .any(|e| e.kind == EdgeKind::Jump && e.source == n.id && e.target == "north"));
}
#[test]
fn story_done_and_end_nodes() {
    let (g, _) = story();
    assert!(g
        .nodes
        .iter()
        .any(|n| n.properties.get("target") == Some(&serde_json::json!("DONE"))));
    assert!(g
        .nodes
        .iter()
        .any(|n| n.properties.get("target") == Some(&serde_json::json!("END"))));
}
#[test]
fn story_condition_script() {
    let (g, _) = story();
    let c = g
        .nodes
        .iter()
        .find(|n| n.source_type == "Condition")
        .unwrap();
    assert_eq!(c.script.as_ref().unwrap().text, "score > 0");
}
#[test]
fn story_condition_branches() {
    let (g, _) = story();
    let c = g
        .nodes
        .iter()
        .find(|n| n.source_type == "Condition")
        .unwrap();
    for label in ["true", "false"] {
        assert!(g
            .edges
            .iter()
            .any(|e| e.source == c.id && e.label.as_deref() == Some(label)));
    }
}
#[test]
fn story_branch_parent_clauses() {
    let (g, _) = story();
    let cid = g
        .nodes
        .iter()
        .find(|n| n.source_type == "Condition")
        .unwrap()
        .id
        .clone();
    assert!(g.nodes.iter().any(
        |n| n.properties.get("clause") == Some(&serde_json::json!("true"))
            && n.parent.as_deref() == Some(cid.as_str())
    ));
    assert_eq!(
        g.nodes
            .iter()
            .filter(|n| n.properties.get("clause") == Some(&serde_json::json!("false")))
            .count(),
        1
    );
}
#[test]
fn story_variable() {
    let (g, r) = story();
    assert_eq!(r.stats.variables, 1);
    assert_eq!(g.variables[0].name, "score");
    assert_eq!(g.variables[0].kind, VariableKind::Integer);
}
#[test]
fn story_tag() {
    let (g, _) = story();
    let n = g
        .nodes
        .iter()
        .find(|n| n.text.as_deref() == Some("Hello there."))
        .unwrap();
    assert_eq!(n.properties["tags"][0], "greeting");
}
#[test]
fn story_zero_warnings() {
    let (_, r) = story();
    assert!(r.warnings.is_empty());
}
#[test]
fn story_report_stats() {
    let (_, r) = story();
    assert_eq!((r.stats.lines, r.stats.choices, r.stats.diverts), (5, 2, 7));
    assert!(r.unsupported.is_empty());
}
#[test]
fn story_golden_graph() {
    let (g, _) = story();
    let expected: DialogueGraph =
        serde_json::from_str(&std::fs::read_to_string("samples/story.graph.json").unwrap())
            .unwrap();
    assert_eq!(g, expected);
}
#[test]
fn story_golden_report() {
    let (_, r) = story();
    let expected: Report =
        serde_json::from_str(&std::fs::read_to_string("samples/story.report.json").unwrap())
            .unwrap();
    assert_eq!(r, expected);
}
#[test]
fn story_deterministic_bytes() {
    let a = story();
    let b = story();
    assert_eq!(
        serde_json::to_vec_pretty(&a.0).unwrap(),
        serde_json::to_vec_pretty(&b.0).unwrap()
    );
    assert_eq!(
        serde_json::to_vec_pretty(&a.1).unwrap(),
        serde_json::to_vec_pretty(&b.1).unwrap()
    );
}

#[test]
fn story_pg03_invariants() {
    let (g, _) = story();
    let nodes: std::collections::HashSet<_> = g.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(nodes.len(), g.nodes.len());
    assert_eq!(g.packages[0].node_ids.len(), g.nodes.len());
    let pins: std::collections::HashMap<_, _> = g.pins.iter().map(|p| (p.id.as_str(), p)).collect();
    for n in &g.nodes {
        for p in n.input_pins.iter().chain(n.output_pins.iter()) {
            assert_eq!(pins.get(p.as_str()).unwrap().owner, n.id);
        }
    }
    for e in &g.edges {
        assert!(nodes.contains(e.source.as_str()) && nodes.contains(e.target.as_str()));
        assert_eq!(
            pins.get(e.source_pin.as_ref().unwrap().as_str())
                .unwrap()
                .direction,
            PinDirection::Output
        );
        assert_eq!(
            pins.get(e.target_pin.as_ref().unwrap().as_str())
                .unwrap()
                .direction,
            PinDirection::Input
        );
    }
    for h in &g.hierarchy {
        assert!(nodes.contains(h.id.as_str()));
        assert_eq!(
            h.parent.as_deref(),
            g.nodes
                .iter()
                .find(|n| n.id == h.id)
                .unwrap()
                .parent
                .as_deref()
        );
    }
    for n in &g.nodes {
        let mut p = n.parent.as_deref();
        let mut guard = 0;
        while let Some(parent) = p {
            assert!(nodes.contains(parent));
            p = g
                .nodes
                .iter()
                .find(|x| x.id == parent)
                .unwrap()
                .parent
                .as_deref();
            guard += 1;
            assert!(guard <= g.nodes.len());
        }
    }
    for c in &g.choices {
        assert!(g.edges.iter().any(|e| e.id == c.edge));
        assert_eq!(c.source_pin, format!("{}#out", c.source));
    }
}

#[test]
fn story_pin_ids_unique() {
    let (g, _) = story();
    let ids: std::collections::HashSet<_> = g.pins.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids.len(), g.pins.len());
}
#[test]
fn story_edge_ids_unique() {
    let (g, _) = story();
    let ids: std::collections::HashSet<_> = g.edges.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids.len(), g.edges.len());
}
#[test]
fn story_hierarchy_preorder_contains_all_nodes() {
    let (g, _) = story();
    assert_eq!(g.hierarchy.len(), g.nodes.len());
    assert_eq!(g.hierarchy[0].id, "Root");
}
#[test]
fn story_options_have_target_text_source() {
    let (g, _) = story();
    assert_eq!(g.choices.len(), 2);
    assert!(g
        .choices
        .iter()
        .all(|c| c.text_source == ChoiceTextSource::TargetMenuText));
}
#[test]
fn story_no_bookkeeping_nodes() {
    let (g, _) = story();
    assert!(!g
        .nodes
        .iter()
        .any(|n| n.source_type == "Temp" || n.source_type == "Unsupported"));
}
#[test]
fn story_line_count_matches_nodes() {
    let (g, r) = story();
    assert_eq!(
        g.nodes.iter().filter(|n| n.source_type == "Line").count(),
        r.stats.lines
    );
}
#[test]
fn story_absolute_targets_resolve() {
    let (g, _) = story();
    assert!(g.edges.iter().any(|e| e.target == "north"));
    assert!(g.edges.iter().any(|e| e.target == "north.entry"));
}
#[test]
fn story_branch_rejoins_set() {
    let (g, _) = story();
    let set = g.nodes.iter().find(|n| n.source_type == "Set").unwrap();
    assert!(g.edges.iter().any(|e| e.target == set.id));
}
