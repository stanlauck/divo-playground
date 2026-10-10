// SPDX-License-Identifier: MIT OR Apache-2.0

//! Property test: randomly generated nested option trees.
//!
//! The generator builds a Yarn node whose body is a chain of choice groups
//! nested to arbitrary depth, then the parser must place every option in
//! exactly one place in the graph — one incoming `option` edge, one parent
//! (or none, for the outermost group), and one surviving `#line:` id.
//!
//! `proptest` is not vendored in the offline registry this repository builds
//! against, so the randomness comes from a small seeded xorshift generator
//! instead. Every seed is fixed, so a failure reproduces exactly.

use std::collections::{HashMap, HashSet};

use pg_21_yarn::{parse, NodeKind};

const INDENT: &str = "    ";
const SEEDS: [u64; 64] = seed_table();
const MAX_DEPTH: usize = 5;

const fn seed_table() -> [u64; 64] {
    let mut table = [0u64; 64];
    let mut value: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut index = 0;
    while index < 64 {
        value = value
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        table[index] = value | 1;
        index += 1;
    }
    table
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value >> 12;
        value ^= value << 25;
        value ^= value >> 27;
        self.0 = value;
        value.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

/// Grows one script and remembers the `#line:` id of every option it wrote.
struct Generator {
    lines: Vec<String>,
    /// `#line:` id of every option written, in emission order.
    options: Vec<String>,
    /// Parallel to `options`: which generated choice group the option belongs
    /// to. Two options share a value exactly when they are siblings.
    group_of: Vec<usize>,
    groups: usize,
    replies: usize,
    depth: usize,
}

impl Generator {
    fn new() -> Self {
        Self {
            lines: Vec::new(),
            options: Vec::new(),
            group_of: Vec::new(),
            groups: 0,
            replies: 0,
            depth: 0,
        }
    }

    fn group(&mut self, indent: usize, rng: &mut Rng) {
        let pad = INDENT.repeat(indent);
        self.groups += 1;
        let group = self.groups;
        for _ in 0..1 + rng.below(3) {
            let number = self.options.len() + 1;
            let id = format!("opt{number:04}");
            self.options.push(id.clone());
            self.group_of.push(group);
            self.lines
                .push(format!("{pad}-> Option {number}. #line:{id}"));

            // Every body opens with a line, so a nested group always has a
            // statement to hang off. An option with an empty body is covered by
            // the golden fixtures instead.
            if self.depth >= MAX_DEPTH || rng.below(5) == 0 {
                continue;
            }
            self.replies += 1;
            let reply = format!("reply{:04}", self.replies);
            self.lines
                .push(format!("{pad}{INDENT}Ada: Reply {number}. #line:{reply}"));
            if rng.below(2) == 0 {
                self.depth += 1;
                self.group(indent + 1, rng);
                self.depth -= 1;
            }
        }
    }

    fn script(&mut self, rng: &mut Rng) -> String {
        self.lines
            .push("Ada: The prompt. #line:prompt0000".to_string());
        self.group(0, rng);
        let mut text = String::from("title: Generated\n---\n");
        text.push_str(&self.lines.join("\n"));
        text.push_str("\n===\n");
        text
    }
}

#[test]
fn every_generated_option_has_exactly_one_parent() {
    for seed in SEEDS {
        let mut rng = Rng(seed);
        let mut generator = Generator::new();
        let script = generator.script(&mut rng);
        let graph = parse("generated.yarn", &script).unwrap();
        assert!(graph.warnings.is_empty(), "seed {seed:#x}");
        let by_id: HashMap<_, _> = graph.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
        let options: Vec<_> = graph
            .nodes
            .iter()
            .filter(|n| n.source_type == "Option")
            .collect();
        assert_eq!(options.len(), generator.options.len());
        for option in options {
            let parent = by_id[option.parent.as_deref().unwrap()];
            assert_eq!(parent.kind, NodeKind::Hub);
            let incoming: Vec<_> = graph
                .choices
                .iter()
                .filter(|c| c.target == option.id)
                .collect();
            assert_eq!(incoming.len(), 1);
            assert_eq!(incoming[0].source, parent.id);
        }
        let by_line_id: HashMap<_, _> = graph
            .nodes
            .iter()
            .filter_map(|n| {
                n.properties
                    .get("line_id")
                    .and_then(|v| v.as_str())
                    .map(|id| (id, n))
            })
            .collect();
        let mut group_hub = HashMap::new();
        let mut hub_group = HashMap::new();
        for (id, group) in generator.options.iter().zip(&generator.group_of) {
            let hub = by_line_id[id.as_str()].parent.as_ref().unwrap();
            if let Some(previous) = group_hub.insert(*group, hub) {
                assert_eq!(previous, hub);
            }
            if let Some(previous) = hub_group.insert(hub, *group) {
                assert_eq!(previous, *group);
            }
        }
        assert_eq!(
            by_line_id.len(),
            generator.options.len() + generator.replies + 1
        );
        assert_eq!(
            graph
                .nodes
                .iter()
                .map(|n| &n.id)
                .collect::<HashSet<_>>()
                .len(),
            graph.nodes.len()
        );
    }
}

#[test]
fn generated_scripts_reparse_identically() {
    for seed in SEEDS {
        let mut rng = Rng(seed);
        let script = Generator::new().script(&mut rng);
        let first = pg_21_yarn::to_json(&parse("generated.yarn", &script).expect("parses"))
            .expect("encodes");
        let second = pg_21_yarn::to_json(&parse("generated.yarn", &script).expect("parses"))
            .expect("encodes");
        assert_eq!(first, second, "seed {seed:#x} is not deterministic");
    }
}
