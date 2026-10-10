// SPDX-License-Identifier: MIT OR Apache-2.0

//! Golden tests.
//!
//! Every `tests/fixtures/*.json` is the exact document the parser must produce
//! for the `.yarn` files that share its stem. A stem with several matching
//! inputs is parsed as one run, which is how cross-file `<<jump>>` resolution is
//! covered: `16-cross-file.json` belongs to `16-cross-file-a.yarn` and
//! `16-cross-file-b.yarn`.
//!
//! Inputs are handed to the parser under their bare file names, so no golden
//! can capture the directory the checkout happens to live in.

use std::fs;
use std::path::{Path, PathBuf};

use pg_21_yarn::{
    parse_sources, parse_sources_with_report, report_to_json, to_json, ParseOptions, Source,
};

const FIXTURES: &str = "tests/fixtures";

fn fixture_dir() -> PathBuf {
    Path::new(FIXTURES).to_path_buf()
}

fn list(extension: &str) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(fixture_dir())
        .expect("tests/fixtures must exist")
        .map(|entry| {
            entry
                .expect("readable directory entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.ends_with(extension))
        .collect();
    names.sort();
    names
}

fn stem(name: &str) -> &str {
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}

fn inputs_for(stem: &str) -> Vec<String> {
    list(".yarn")
        .into_iter()
        .filter(|name| name.starts_with(stem))
        .collect()
}

fn first_difference(expected: &str, actual: &str) -> String {
    for (index, (want, got)) in expected.lines().zip(actual.lines()).enumerate() {
        if want != got {
            return format!(
                "line {} differs:\n  expected: {}\n  actual:   {}",
                index + 1,
                want,
                got
            );
        }
    }
    format!(
        "documents agree line by line but differ in length: expected {} lines, actual {} lines",
        expected.lines().count(),
        actual.lines().count()
    )
}

#[test]
fn every_fixture_matches_its_golden() {
    let options = ParseOptions::default();
    let mut failures = Vec::new();
    let mut checked = 0usize;

    for golden in list(".json") {
        let golden_stem = stem(&golden);
        let inputs = inputs_for(golden_stem);
        assert!(
            !inputs.is_empty(),
            "golden {golden} has no `.yarn` input starting with `{golden_stem}`"
        );

        let sources: Vec<Source> = inputs
            .iter()
            .map(|name| Source {
                name: name.clone(),
                text: fs::read_to_string(fixture_dir().join(name))
                    .unwrap_or_else(|error| panic!("cannot read {name}: {error}")),
            })
            .collect();

        let (graph, report) = parse_sources_with_report(&sources, &options)
            .unwrap_or_else(|error| panic!("{golden}: parsing failed: {error}"));
        let actual = to_json(&graph).expect("the graph must serialize");
        let expected = fs::read_to_string(fixture_dir().join(&golden))
            .unwrap_or_else(|error| panic!("cannot read {golden}: {error}"));

        let expected_report = fs::read_to_string(Path::new("tests/reports").join(&golden))
            .expect("report golden exists");
        assert_eq!(
            report_to_json(&report).unwrap(),
            expected_report,
            "{golden}: report differs"
        );
        checked += 1;
        if actual != expected {
            failures.push(format!(
                "{golden} (from {}) is stale\n{}",
                inputs.join(", "),
                first_difference(&expected, &actual)
            ));
        }
    }

    assert!(
        checked >= 12,
        "expected at least 12 goldens, found {checked}"
    );
    assert!(
        failures.is_empty(),
        "{} golden(s) did not match:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

#[test]
fn every_yarn_file_is_covered_by_a_golden() {
    let golden_stems: Vec<String> = list(".json")
        .iter()
        .map(|name| stem(name).to_string())
        .collect();
    let uncovered: Vec<String> = list(".yarn")
        .into_iter()
        .filter(|name| !golden_stems.iter().any(|stem| name.starts_with(stem)))
        .collect();
    assert!(
        uncovered.is_empty(),
        "these fixtures have no golden: {}",
        uncovered.join(", ")
    );
}

#[test]
fn goldens_are_stable_across_runs() {
    let options = ParseOptions::default();
    for golden in list(".json") {
        let sources: Vec<Source> = inputs_for(stem(&golden))
            .iter()
            .map(|name| Source {
                name: name.clone(),
                text: fs::read_to_string(fixture_dir().join(name)).expect("readable fixture"),
            })
            .collect();
        let first = to_json(&parse_sources(&sources, &options).expect("parses")).expect("encodes");
        let second = to_json(&parse_sources(&sources, &options).expect("parses")).expect("encodes");
        assert_eq!(first, second, "{golden} is not deterministic");
    }
}
