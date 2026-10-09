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

use pg_21_yarn::{parse, EdgeKind, NodeKind};

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

/// True when `node` is `owner` or sits somewhere inside `owner`'s subtree.
fn owns(parents: &HashMap<String, Option<String>>, owner: &str, node: &str) -> bool {
    let mut cursor = node.to_string();
    loop {
        if cursor == owner {
            return true;
        }
        match parents.get(&cursor).and_then(Option::as_ref) {
            Some(parent) => cursor = parent.clone(),
            None => return false,
        }
    }
}

#[test]
fn every_generated_option_has_exactly_one_parent() {
    for seed in SEEDS {
        let mut rng = Rng(seed);
        let mut generator = Generator::new();
        let script = generator.script(&mut rng);
        let generated = generator.options.clone();
        let group_of = generator.group_of.clone();
        let replies = generator.replies;

        let graph = parse("generated.yarn", &script)
            .unwrap_or_else(|error| panic!("seed {seed:#x} failed to parse: {error}"));

        assert!(
            graph.errors.is_empty(),
            "seed {seed:#x} produced findings: {:?}",
            graph.errors
        );

        let by_id: HashMap<&str, &pg_21_yarn::Node> = graph
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect();
        let parents: HashMap<String, Option<String>> = graph
            .nodes
            .iter()
            .map(|node| (node.id.clone(), node.parent.clone()))
            .collect();

        let options: Vec<&pg_21_yarn::Node> = graph
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Options)
            .collect();
        assert_eq!(
            options.len(),
            generated.len(),
            "seed {seed:#x}: expected {} options, the graph holds {}\n{script}",
            generated.len(),
            options.len()
        );

        // Every option is entered exactly once, from exactly one place.
        let mut incoming: HashMap<&str, Vec<&str>> = HashMap::new();
        for edge in &graph.edges {
            if edge.kind == EdgeKind::Option {
                incoming
                    .entry(edge.target.as_str())
                    .or_default()
                    .push(edge.source.as_str());
            }
        }
        for node in &options {
            let sources = incoming
                .get(node.id.as_str())
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            assert_eq!(
                sources.len(),
                1,
                "seed {seed:#x}: option {} has {} incoming option edges ({sources:?})\n{script}",
                node.id,
                sources.len()
            );

            // `parent` names the option that owns this one; the outermost
            // group belongs to the node body and has no parent. The incoming
            // edge comes from whatever statement precedes the group, which for
            // a nested group sits inside the owner's own body — not from the
            // owner itself.
            let entered_from = by_id.get(sources[0]).unwrap_or_else(|| {
                panic!("seed {seed:#x}: edge source {} does not exist", sources[0])
            });
            match &node.parent {
                None => assert!(
                    entered_from.parent.is_none(),
                    "seed {seed:#x}: a parentless option must be entered from the node body, \
                     not from inside {}\n{script}",
                    sources[0]
                ),
                Some(parent) => {
                    let owner = by_id
                        .get(parent.as_str())
                        .unwrap_or_else(|| panic!("seed {seed:#x}: {parent} does not exist"));
                    assert_eq!(
                        owner.kind,
                        NodeKind::Options,
                        "seed {seed:#x}: {parent} owns an option but is not one\n{script}"
                    );
                    assert!(
                        owns(&parents, parent, &entered_from.id),
                        "seed {seed:#x}: option {} is owned by {parent} but entered from {}, \
                         which is not inside that owner\n{script}",
                        node.id,
                        sources[0]
                    );
                }
            }
        }

        // Choice groups line up with the ones that were generated: siblings
        // share a `(parent, option_group)` pair, and no two generated groups
        // collapse onto the same pair.
        let by_line_id: HashMap<&str, &pg_21_yarn::Node> = graph
            .nodes
            .iter()
            .filter_map(|node| node.line_id.as_deref().map(|id| (id, node)))
            .collect();
        let mut group_key: HashMap<usize, (Option<String>, usize)> = HashMap::new();
        let mut key_group: HashMap<(Option<String>, usize), usize> = HashMap::new();
        for (id, generated_group) in generated.iter().zip(&group_of) {
            let node = by_line_id
                .get(id.as_str())
                .unwrap_or_else(|| panic!("seed {seed:#x}: option {id} is missing"));
            let number = node
                .option_group
                .unwrap_or_else(|| panic!("seed {seed:#x}: option {id} has no choice group"));
            let key = (node.parent.clone(), number);
            if let Some(previous) = group_key.insert(*generated_group, key.clone()) {
                assert_eq!(
                    previous, key,
                    "seed {seed:#x}: generated group {generated_group} was split across \
                     {previous:?} and {key:?}\n{script}"
                );
            }
            if let Some(other) = key_group.insert(key.clone(), *generated_group) {
                assert_eq!(
                    other, *generated_group,
                    "seed {seed:#x}: generated groups {other} and {generated_group} both \
                     became {key:?}\n{script}"
                );
            }
        }

        // Nothing was dropped on the floor: every generated id survived, and the
        // bodies generated alongside them too.
        let ids: Vec<&str> = graph
            .nodes
            .iter()
            .filter_map(|node| node.line_id.as_deref())
            .collect();
        assert_eq!(
            ids.len(),
            generated.len() + replies + 1,
            "seed {seed:#x}\n{script}"
        );
        for id in &generated {
            assert!(
                ids.contains(&id.as_str()),
                "seed {seed:#x}: {id} was lost\n{script}"
            );
        }
        assert_eq!(
            ids.iter().collect::<HashSet<_>>().len(),
            ids.len(),
            "seed {seed:#x}: a line id was reused\n{script}"
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
