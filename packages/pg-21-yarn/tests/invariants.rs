// SPDX-License-Identifier: MIT OR Apache-2.0

use pg_21_yarn::{
    parse, parse_sources, ChoiceTextSource, DialogueGraph, EdgeKind, ParseOptions, PinDirection,
    Source,
};
use std::collections::{HashMap, HashSet};

fn validate(graph: &DialogueGraph) {
    let nodes: HashMap<_, _> = graph.nodes.iter().map(|n| (&n.id, n)).collect();
    let pins: HashMap<_, _> = graph.pins.iter().map(|p| (&p.id, p)).collect();
    let edges: HashMap<_, _> = graph.edges.iter().map(|e| (&e.id, e)).collect();
    assert_eq!(nodes.len(), graph.nodes.len());
    assert_eq!(pins.len(), graph.pins.len());
    assert_eq!(edges.len(), graph.edges.len());
    assert!(nodes.keys().all(|id| !pins.contains_key(id)));
    let mut membership = HashMap::new();
    for (index, package) in graph.packages.iter().enumerate() {
        assert_eq!(package.index, index);
        let ordered: Vec<_> = graph
            .nodes
            .iter()
            .filter(|n| n.package == index)
            .map(|n| n.id.clone())
            .collect();
        assert_eq!(package.node_ids, ordered);
        for id in &package.node_ids {
            assert_eq!(nodes[id].package, index);
            assert!(membership.insert(id, index).is_none());
        }
    }
    assert_eq!(membership.len(), nodes.len());
    for node in &graph.nodes {
        assert_eq!(node.input_pins.len(), 1);
        assert_eq!(node.output_pins.len(), 1);
        for (ids, direction) in [
            (&node.input_pins, PinDirection::Input),
            (&node.output_pins, PinDirection::Output),
        ] {
            let pin = pins[&ids[0]];
            assert_eq!(pin.owner, node.id);
            assert_eq!(pin.direction, direction);
            assert_eq!(pin.index, 0);
            assert!(pin.script.is_none());
        }
        let mut ancestor = Some(&node.id);
        let mut seen = HashSet::new();
        while let Some(id) = ancestor {
            assert!(seen.insert(id), "parent cycle at {id}");
            ancestor = nodes[id].parent.as_ref();
        }
    }
    for pin in &graph.pins {
        let owner = nodes[&pin.owner];
        let listed = match pin.direction {
            PinDirection::Input => &owner.input_pins,
            PinDirection::Output => &owner.output_pins,
        };
        assert!(listed.contains(&pin.id));
    }
    let mut outgoing = HashMap::new();
    for (number, edge) in graph.edges.iter().enumerate() {
        assert_eq!(edge.id, format!("e{number}"));
        let source = nodes[&edge.source];
        let target = nodes[&edge.target];
        let from = pins[edge.source_pin.as_ref().unwrap()];
        let to = pins[edge.target_pin.as_ref().unwrap()];
        assert_eq!(from.owner, source.id);
        assert_eq!(to.owner, target.id);
        assert_eq!(from.direction, PinDirection::Output);
        assert_eq!(to.direction, PinDirection::Input);
        let index = outgoing.entry(&from.id).or_insert(0);
        assert_eq!(edge.index, *index);
        *index += 1;
    }
    let mut choice_edges = HashSet::new();
    for choice in &graph.choices {
        let edge = edges[&choice.edge];
        assert!(choice_edges.insert(&choice.edge));
        assert_eq!(edge.kind, EdgeKind::Connection);
        assert_eq!(edge.source, choice.source);
        assert_eq!(edge.target, choice.target);
        assert_eq!(edge.source_pin.as_ref(), Some(&choice.source_pin));
        assert_eq!(pins[&choice.source_pin].direction, PinDirection::Output);
        assert_eq!(pins[&choice.source_pin].owner, choice.source);
        assert_eq!(nodes[&choice.source].source_type, "OptionGroup");
        assert_eq!(nodes[&choice.target].source_type, "Option");
        assert_eq!(choice.text_source, ChoiceTextSource::TargetMenuText);
    }
    assert_eq!(graph.hierarchy.len(), nodes.len());
    let mut hierarchy = HashSet::new();
    let mut sibling_counts = HashMap::new();
    let mut stack: Vec<String> = Vec::new();
    for entry in &graph.hierarchy {
        assert!(hierarchy.insert(&entry.id));
        assert_eq!(nodes[&entry.id].parent, entry.parent);
        let count = sibling_counts.entry(&entry.parent).or_insert(0);
        assert_eq!(entry.index, *count);
        *count += 1;
        if let Some(parent) = &entry.parent {
            while stack.last() != Some(parent) {
                assert!(stack.pop().is_some(), "parent must precede children");
            }
            assert_eq!(entry.depth, stack.len());
        } else {
            stack.clear();
            assert_eq!(entry.depth, 0);
        }
        assert!(entry.properties.is_empty());
        stack.push(entry.id.clone());
    }
}

#[test]
fn pg03_invariants_hold_for_all_synthetic_fixtures() {
    let mut files: Vec<_> = std::fs::read_dir("tests/fixtures")
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "yarn"))
        .collect();
    files.sort();
    let sources: Vec<_> = files
        .iter()
        .map(|p| {
            Source::new(
                p.file_name().unwrap().to_str().unwrap(),
                std::fs::read_to_string(p).unwrap(),
            )
        })
        .collect();
    validate(&parse_sources(&sources, &ParseOptions::default()).unwrap());
    for source in &sources {
        validate(&parse(&source.name, &source.text).unwrap());
    }
}

#[test]
fn pg03_invariants_hold_for_colliding_names_and_nested_branches() {
    let script = "title: A\n---\n<<if $x>>\n-> First\n    <<if $y>>\n    Nested.\n    <<endif>>\n<<else>>\n-> Second\n<<endif>>\nAfter.\n<<jump Empty>>\n===\ntitle: A:1\n---\nExplicit.\n===\ntitle: A#in\n---\nPin name.\n===\ntitle: A\n---\nDuplicate.\n===\ntitle: Empty\n---\n===\n";
    validate(&parse("collision.yarn", script).unwrap());
}
