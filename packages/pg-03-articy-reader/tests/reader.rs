// SPDX-License-Identifier: MIT OR Apache-2.0

use pg_03_articy_reader::*;
use serde_json::{json, Value};
use std::io::{self, Cursor, Read};

const SAMPLE: &str = include_str!("../samples/synthetic.articy.json");
fn read(value: Value) -> DialogueGraph {
    read_export(
        Cursor::new(serde_json::to_vec(&value).unwrap()),
        &ImportOptions::default(),
    )
    .unwrap()
}
fn fails(value: Value) -> bool {
    read_export(
        Cursor::new(serde_json::to_vec(&value).unwrap()),
        &ImportOptions::default(),
    )
    .is_err()
}
fn export(models: Value) -> Value {
    json!({"Packages":[{"Name":"synthetic","Models":models}]})
}
fn node(id: &str, kind: &str) -> Value {
    json!({"Type":kind,"Properties":{"Id":id}})
}
fn linked() -> Value {
    export(json!([
        {"Type":"DialogueFragment","Properties":{"Id":"one","OutputPins":[{"Id":"out","Owner":"one","Text":"Counter += 1;","Connections":[{"Target":"two","TargetPin":"in","Label":"next"}]}]}},
        {"Type":"DialogueFragment","Properties":{"Id":"two","InputPins":[{"Id":"in","Owner":"two","Text":"Counter > 0"}]}}
    ]))
}

#[test]
fn sample_keeps_full_graph_and_native_metadata() {
    let graph = read_export(
        Cursor::new(SAMPLE),
        &ImportOptions {
            strict_references: true,
            ..ImportOptions::default()
        },
    )
    .unwrap();
    assert_eq!(graph.nodes.len(), 8);
    assert_eq!(graph.pins.len(), 4);
    assert_eq!(graph.edges.len(), 3);
    assert_eq!(graph.choices.len(), 2);
    assert_eq!(graph.variables.len(), 4);
    assert_eq!(graph.variable_namespaces.len(), 2);
    assert_eq!(graph.hierarchy.len(), 7);
    assert!(graph.warnings.is_empty());
    assert_eq!(graph.metadata["Settings"]["ExportVersion"], "1.0");
    assert_eq!(graph.nodes[2].kind, NodeKind::DialogueFragment);
    assert_eq!(
        graph.nodes[2].template.as_ref().unwrap()["Mood"]["Tone"],
        "curious"
    );
    assert!(graph.nodes[2]
        .text
        .as_ref()
        .unwrap()
        .contains("[b]amber[/b]"));
    assert_eq!(graph.variables[0].value, json!(false));
    assert_eq!(graph.variables[0].raw_value, "False");
    assert_eq!(graph.variables[3].value, "Север 南");
    assert_eq!(graph.choices[0].text_source, ChoiceTextSource::EdgeLabel);
    assert_eq!(
        graph.choices[1].text_source,
        ChoiceTextSource::TargetMenuText
    );
}

#[test]
fn scripts_are_text_data_and_never_run() {
    let mut value = linked();
    value["Packages"][0]["Models"][0]["Properties"]["OutputPins"][0]["Text"] =
        json!("UnrecognizedFunction(); os_command('synthetic');");
    let graph = read(value);
    assert_eq!(
        graph.pins[0].script.as_ref().unwrap().role,
        ScriptRole::Instruction
    );
    assert!(graph.pins[0]
        .script
        .as_ref()
        .unwrap()
        .text
        .contains("UnrecognizedFunction"));
    assert_eq!(
        graph.pins[1].script.as_ref().unwrap().role,
        ScriptRole::Condition
    );
}

#[test]
fn empty_export_is_valid_but_missing_packages_is_not() {
    let graph = read(json!({"Packages":[]}));
    assert!(graph.nodes.is_empty());
    assert!(fails(json!({})));
    assert!(fails(json!({"Packages":{}})));
    assert!(fails(json!({"Packages":[{"Name":"x"}]})));
}

#[test]
fn model_pin_and_namespace_ids_are_unique_in_their_domains() {
    assert!(fails(export(json!([
        node("one", "Hub"),
        node("one", "Hub")
    ]))));
    let mut value = linked();
    value["Packages"][0]["Models"][1]["Properties"]["Id"] = json!("out");
    assert!(fails(value));
    let mut value = linked();
    value["Packages"][0]["Models"][1]["Properties"]["InputPins"][0]["Id"] = json!("out");
    assert!(fails(value));
    let value = json!({"Packages":[],"GlobalVariables":[{"Namespace":"N","Variables":[]},{"Namespace":"N","Variables":[]}]});
    assert!(fails(value));
}

#[test]
fn pin_owner_and_target_pin_owner_must_match_containing_models() {
    let mut value = linked();
    value["Packages"][0]["Models"][0]["Properties"]["OutputPins"][0]["Owner"] = json!("two");
    assert!(fails(value));
    let mut value = linked();
    value["Packages"][0]["Models"][0]["Properties"]["OutputPins"][0]["Connections"][0]["Target"] =
        json!("one");
    assert!(fails(value));
}

#[test]
fn missing_targets_are_preserved_and_warned_unless_strict() {
    let mut value = linked();
    value["Packages"][0]["Models"].as_array_mut().unwrap().pop();
    let bytes = serde_json::to_vec(&value).unwrap();
    let graph = read_export(Cursor::new(&bytes), &ImportOptions::default()).unwrap();
    assert_eq!(graph.edges[0].target, "two");
    assert!(graph
        .warnings
        .iter()
        .any(|warning| warning.kind == WarningKind::MissingNode));
    assert!(graph
        .warnings
        .iter()
        .any(|warning| warning.kind == WarningKind::MissingPin));
    assert!(read_export(
        Cursor::new(bytes),
        &ImportOptions {
            strict_references: true,
            ..ImportOptions::default()
        }
    )
    .is_err());
}

#[test]
fn null_parent_and_speaker_sentinels_are_not_dangling_references() {
    let value = export(
        json!([{"Type":"DialogueFragment","Properties":{"Id":"one","Parent":"0x0000000000000000","Speaker":null}}]),
    );
    let graph = read(value);
    assert_eq!(graph.nodes[0].parent, None);
    assert_eq!(graph.nodes[0].speaker, None);
    assert!(graph.warnings.is_empty());
    assert!(fails(export(json!([node("0x0000000000000000", "Hub")]))));
}

#[test]
fn all_source_ids_are_opaque_strings_never_numeric_or_rewritten() {
    let graph = read(export(json!([node("0xFEDCBA9876543210", "Hub")])));
    assert_eq!(graph.nodes[0].id, "0xFEDCBA9876543210");
    assert!(fails(export(
        json!([{"Type":"Hub","Properties":{"Id":9007199254740993u64}}])
    )));
    assert!(fails(export(json!([node(" one ", "Hub")]))));
}

#[test]
fn unknown_object_types_and_custom_features_are_retained() {
    let value = export(
        json!([{"Type":"CustomUnknown","Properties":{"Id":"one","Extra":{"unicode":"مرحبا"}},"Template":{"Synthetic":{"flag":true}},"ModelExtra":[1,2]}]),
    );
    let graph = read(value);
    assert_eq!(graph.nodes[0].kind, NodeKind::Other);
    assert_eq!(graph.nodes[0].properties["Extra"]["unicode"], "مرحبا");
    assert_eq!(graph.nodes[0].metadata["ModelExtra"], json!([1, 2]));
    assert!(graph
        .warnings
        .iter()
        .any(|warning| warning.kind == WarningKind::UnknownType));
}

#[test]
fn definition_inheritance_is_resolved_and_cycles_are_rejected() {
    let value = json!({"Packages":[{"Name":"x","Models":[node("one","Custom")]}],"ObjectDefinitions":[{"Type":"Custom","Class":"ArticyObject","InheritsFrom":"Base"},{"Type":"Base","Class":"DialogueFragment"}]});
    assert_eq!(read(value).nodes[0].kind, NodeKind::DialogueFragment);
    assert!(fails(
        json!({"Packages":[],"ObjectDefinitions":[{"Type":"A","Class":"ArticyObject","InheritsFrom":"B"},{"Type":"B","Class":"ArticyObject","InheritsFrom":"A"}]})
    ));
    assert!(fails(
        json!({"Packages":[],"ObjectDefinitions":[{"Type":"A","Class":"Hub"},{"Type":"A","Class":"Hub"}]})
    ));
}

#[test]
fn parent_cycles_are_rejected_while_flow_cycles_are_valid() {
    assert!(fails(export(json!([
        {"Type":"Hub","Properties":{"Id":"a","Parent":"b"}},
        {"Type":"Hub","Properties":{"Id":"b","Parent":"a"}}
    ]))));
    let mut value = linked();
    value["Packages"][0]["Models"][1]["Properties"]["OutputPins"] =
        json!([{"Id":"back","Owner":"two","Connections":[{"Target":"one","TargetPin":"first"}]}]);
    value["Packages"][0]["Models"][0]["Properties"]["InputPins"] =
        json!([{"Id":"first","Owner":"one"}]);
    assert_eq!(read(value).edges.len(), 2);
}

#[test]
fn hierarchy_order_depth_unknown_objects_and_metadata_are_preserved() {
    let value = json!({"Packages":[{"Name":"x","Models":[node("a","Hub"),node("b","Hub")]}],"Hierarchy":{"Id":"a","Type":"Hub","Children":[{"Id":"b"},{"Id":"not_exported","Extra":1}]}});
    let graph = read(value);
    assert_eq!(graph.hierarchy[1].parent.as_deref(), Some("a"));
    assert_eq!(graph.hierarchy[1].depth, 1);
    assert_eq!(graph.hierarchy[2].index, 1);
    assert_eq!(graph.hierarchy[2].properties["Extra"], 1);
    assert!(graph
        .warnings
        .iter()
        .any(|warning| warning.kind == WarningKind::MissingHierarchyObject));
    assert!(fails(
        json!({"Packages":[],"Hierarchy":{"Id":"a","Children":[{"Id":"a"}]}})
    ));
}

#[test]
fn omitted_model_parents_are_inferred_from_hierarchy_without_changing_properties() {
    let mut value = export(json!([
        node("parent", "FlowFragment"),
        node("child", "Hub")
    ]));
    value["Hierarchy"] = json!({"Id":"parent","Children":[{"Id":"child"}]});
    let graph = read_export(
        Cursor::new(serde_json::to_vec(&value).unwrap()),
        &ImportOptions {
            strict_references: true,
            ..ImportOptions::default()
        },
    )
    .unwrap();
    assert_eq!(graph.nodes[0].parent, None);
    assert_eq!(graph.nodes[1].parent.as_deref(), Some("parent"));
    assert!(!graph.nodes[1].properties.contains_key("Parent"));
    assert_eq!(graph.hierarchy[1].parent.as_deref(), Some("parent"));
    assert!(graph.warnings.is_empty());
}

#[test]
fn explicit_null_model_parents_conflict_with_nested_hierarchy() {
    for declared in [Value::Null, json!("0x0"), json!("0X00000000000000000")] {
        let mut value = export(json!([
            node("parent", "FlowFragment"),
            node("child", "Hub")
        ]));
        value["Packages"][0]["Models"][1]["Properties"]["Parent"] = declared.clone();
        value["Hierarchy"] = json!({"Id":"parent","Children":[{"Id":"child"}]});
        let graph = read(value.clone());
        assert_eq!(graph.nodes[1].parent, None);
        assert_eq!(graph.nodes[1].properties["Parent"], declared);
        assert_eq!(graph.hierarchy[1].parent.as_deref(), Some("parent"));
        assert_eq!(graph.warnings.len(), 1);
        assert_eq!(graph.warnings[0].kind, WarningKind::HierarchyParentMismatch);
        assert!(read_export(
            Cursor::new(serde_json::to_vec(&value).unwrap()),
            &ImportOptions {
                strict_references: true,
                ..ImportOptions::default()
            },
        )
        .is_err());
    }
}

#[test]
fn inferred_parents_report_filtered_hierarchy_roots() {
    let mut value = export(json!([node("child", "Hub")]));
    value["Hierarchy"] = json!({"Id":"not_exported","Children":[{"Id":"child"}]});
    let graph = read(value.clone());
    assert_eq!(graph.nodes[0].parent.as_deref(), Some("not_exported"));
    assert!(graph
        .warnings
        .iter()
        .any(|warning| warning.kind == WarningKind::MissingParent));
    assert!(graph
        .warnings
        .iter()
        .any(|warning| warning.kind == WarningKind::MissingHierarchyObject));
    assert!(read_export(
        Cursor::new(serde_json::to_vec(&value).unwrap()),
        &ImportOptions {
            strict_references: true,
            ..ImportOptions::default()
        },
    )
    .is_err());
}

#[test]
fn inferred_parents_cannot_complete_a_model_parent_cycle() {
    let mut value = export(json!([
        {"Type":"Hub","Properties":{"Id":"a","Parent":"b"}},
        node("b", "Hub")
    ]));
    value["Hierarchy"] = json!({"Id":"a","Children":[{"Id":"b"}]});
    assert!(fails(value));
}

#[test]
fn zero_model_pin_and_hierarchy_ids_are_rejected_at_every_supported_width() {
    for digits in [1, 16, 17, 32, 126] {
        for prefix in ["0x", "0X"] {
            let zero = format!("{prefix}{}", "0".repeat(digits));
            assert!(fails(export(json!([node(&zero, "Hub")]))));
            let mut value = linked();
            value["Packages"][0]["Models"][0]["Properties"]["OutputPins"][0]["Id"] = json!(zero);
            assert!(fails(value));
            assert!(fails(json!({"Packages":[],"Hierarchy":{"Id":zero}})));
            assert!(fails(
                json!({"Packages":[],"Hierarchy":{"Id":"root","Children":[{"Id":zero}]}})
            ));
        }
    }
}

#[test]
fn zero_references_have_no_arbitrary_hex_width_restriction() {
    for digits in [1, 16, 17, 32, 126] {
        for prefix in ["0x", "0X"] {
            let zero = format!("{prefix}{}", "0".repeat(digits));
            let value = export(json!([
                {"Type":"DialogueFragment","Properties":{"Id":"line","Parent":zero,"Speaker":zero}},
                {"Type":"Jump","Properties":{"Id":"jump","Target":zero,"TargetPin":zero}}
            ]));
            let graph = read_export(
                Cursor::new(serde_json::to_vec(&value).unwrap()),
                &ImportOptions {
                    strict_references: true,
                    ..ImportOptions::default()
                },
            )
            .unwrap();
            assert_eq!(graph.nodes[0].parent, None);
            assert_eq!(graph.nodes[0].speaker, None);
            assert_eq!(graph.nodes[0].properties["Parent"], zero);
            assert!(graph.edges.is_empty());
            assert!(graph.warnings.is_empty());
        }
    }
    // Nonzero IDs and bare prefixes remain opaque IDs, not null references.
    for id in ["0x", "0X", "0x00000000000000001", "0X00000000000000001"] {
        let value = export(json!([
            node(id, "Hub"),
            {"Type":"Hub","Properties":{"Id":"child","Parent":id}}
        ]));
        assert_eq!(read(value).nodes[1].parent.as_deref(), Some(id));
    }
}

#[test]
fn variable_values_are_typed_but_original_values_survive() {
    let value = json!({"Packages":[],"GlobalVariables":[{"Namespace":"N","Description":"namespace data","Variables":[
        {"Variable":"bool","Type":"Boolean","Value":"True"},
        {"Variable":"int","Type":"Integer","Value":"-12"},
        {"Variable":"float","Type":"Float","Value":"2.5"},
        {"Variable":"str","Type":"String","Value":"true"},
        {"Variable":"other","Type":"Custom","Value":{"opaque":1},"Extra":7}
    ]}]});
    let graph = read(value);
    assert_eq!(graph.variables[0].value, true);
    assert_eq!(graph.variables[1].value, -12);
    assert_eq!(graph.variables[2].value, 2.5);
    assert_eq!(graph.variables[3].value, "true");
    assert_eq!(graph.variables[4].kind, VariableKind::Other);
    assert_eq!(graph.variables[4].metadata["Extra"], 7);
    assert_eq!(
        graph.variable_namespaces[0].metadata["Description"],
        "namespace data"
    );
}

#[test]
fn invalid_variable_values_and_duplicate_names_fail() {
    for (kind, value) in [
        ("Boolean", json!("yes")),
        ("Integer", json!("2.5")),
        ("Float", json!("NaN")),
        ("Float", json!("inf")),
        ("String", json!(42)),
    ] {
        assert!(fails(
            json!({"Packages":[],"GlobalVariables":[{"Namespace":"N","Variables":[{"Variable":"v","Type":kind,"Value":value}]}]})
        ));
    }
    assert!(fails(
        json!({"Packages":[],"GlobalVariables":[{"Namespace":"N","Variables":[{"Variable":"v","Type":"String","Value":"a"},{"Variable":"v","Type":"String","Value":"b"}]}]})
    ));
}

#[test]
fn localization_keys_are_annotated_not_fetched_or_misread_as_prose() {
    let value = json!({"Settings":{"set_Localization":"True","set_UsedLanguage":"fr"},"Packages":[{"Name":"x","Models":[{"Type":"DialogueFragment","Properties":{"Id":"one","Text":"loc-key-1"}}]}]});
    let graph = read(value);
    assert_eq!(graph.text_mode, TextMode::LocalizationKeys);
    assert_eq!(graph.nodes[0].text.as_deref(), Some("loc-key-1"));
    assert!(graph
        .warnings
        .iter()
        .any(|warning| warning.kind == WarningKind::LocalizationKeys));
}

#[test]
fn malformed_json_duplicate_keys_trailing_data_and_invalid_utf8_fail() {
    for bytes in [
        br#"{"Packages":[],"Packages":[]}"#.as_slice(),
        br#"{"Packages":[{"Name":"x","Models":[{"Type":"Hub","Properties":{"Id":"a","Id":"b"}}]}]}"#.as_slice(),
        br#"{"Packages":[]} {"Packages":[]}"#.as_slice(),
        br#"{"Packages":[}"#.as_slice(),
        b"\xff".as_slice(),
    ] { assert!(read_export(Cursor::new(bytes), &ImportOptions::default()).is_err()); }
}

#[test]
fn utf8_bom_and_unicode_are_supported() {
    let source = format!(
        "\u{feff}{}",
        export(
            json!([{"Type":"DialogueFragment","Properties":{"Id":"one","Text":"مرحبا עברית नमस्ते 南 e\u{301}"}}])
        )
    );
    let graph = read_export(Cursor::new(source), &ImportOptions::default()).unwrap();
    assert_eq!(
        graph.nodes[0].text.as_deref(),
        Some("مرحبا עברית नमस्ते 南 e\u{301}")
    );
}

#[test]
fn graph_round_trip_and_output_order_are_deterministic() {
    let graph = read_export(Cursor::new(SAMPLE), &ImportOptions::default()).unwrap();
    let bytes = serde_json::to_vec(&graph).unwrap();
    assert_eq!(
        serde_json::from_slice::<DialogueGraph>(&bytes).unwrap(),
        graph
    );
    assert_eq!(
        serde_json::to_vec(&read_export(Cursor::new(SAMPLE), &ImportOptions::default()).unwrap())
            .unwrap(),
        bytes
    );
}

#[test]
fn input_byte_depth_node_pin_edge_and_warning_limits_are_enforced() {
    let bytes = serde_json::to_vec(&linked()).unwrap();
    for options in [
        ImportOptions {
            max_input_bytes: 10,
            ..ImportOptions::default()
        },
        ImportOptions {
            max_depth: 2,
            ..ImportOptions::default()
        },
        ImportOptions {
            max_nodes: 1,
            ..ImportOptions::default()
        },
        ImportOptions {
            max_pins: 1,
            ..ImportOptions::default()
        },
        ImportOptions {
            max_identifier_bytes: 2,
            ..ImportOptions::default()
        },
    ] {
        assert!(read_export(Cursor::new(&bytes), &options).is_err());
    }
    let mut value = linked();
    value["Packages"][0]["Models"][0]["Properties"]["OutputPins"][0]["Connections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"Target":"two","TargetPin":"in"}));
    assert!(read_export(
        Cursor::new(serde_json::to_vec(&value).unwrap()),
        &ImportOptions {
            max_edges: 1,
            ..ImportOptions::default()
        }
    )
    .is_err());
    value["Packages"][0]["Models"].as_array_mut().unwrap().pop();
    assert!(read_export(
        Cursor::new(serde_json::to_vec(&value).unwrap()),
        &ImportOptions {
            max_warnings: 1,
            ..ImportOptions::default()
        }
    )
    .is_err());
}

#[test]
fn reader_errors_propagate_and_zero_limits_are_invalid() {
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("synthetic read failure"))
        }
    }
    assert!(matches!(
        read_export(Broken, &ImportOptions::default()),
        Err(Error::Io(_))
    ));
    assert!(read_export(
        Cursor::new("{}"),
        &ImportOptions {
            max_nodes: 0,
            ..ImportOptions::default()
        }
    )
    .is_err());
}

#[test]
fn jump_transfers_are_separate_edges_with_optional_target_pin() {
    let graph = read(export(
        json!([{"Type":"Jump","Properties":{"Id":"j","Target":"h","TargetPin":"0x0000000000000000"}},node("h","Hub")]),
    ));
    assert_eq!(graph.edges[0].kind, EdgeKind::Jump);
    assert_eq!(graph.edges[0].source_pin, None);
    assert_eq!(graph.edges[0].target_pin, None);
}

#[test]
fn multiple_packages_keep_input_order_and_duplicate_ids_are_not_deduplicated() {
    let graph = read(
        json!({"Packages":[{"Name":"first","Models":[node("one","Hub")]},{"Name":"second","Models":[node("two","Hub")]}]}),
    );
    assert_eq!(graph.nodes[1].package, 1);
    assert_eq!(graph.packages[1].node_ids, ["two"]);
    assert!(fails(
        json!({"Packages":[{"Name":"a","Models":[node("one","Hub")]},{"Name":"b","Models":[node("one","Hub")]}]})
    ));
}

#[test]
fn container_input_pin_connections_are_retained_without_direction_restrictions() {
    let graph = read(export(json!([
        {"Type":"FlowFragment","Properties":{"Id":"f","InputPins":[{"Id":"entry","Owner":"f","Connections":[{"Target":"line","TargetPin":"input"}]}]}},
        {"Type":"DialogueFragment","Properties":{"Id":"line","Parent":"f","InputPins":[{"Id":"input","Owner":"line"}]}}
    ])));
    assert_eq!(graph.edges[0].source_pin.as_deref(), Some("entry"));
    assert_eq!(graph.edges[0].target_pin.as_deref(), Some("input"));
    assert!(graph.choices.is_empty());
}

#[test]
fn parallel_connections_and_output_pin_order_are_not_collapsed() {
    let mut value = linked();
    let connection =
        value["Packages"][0]["Models"][0]["Properties"]["OutputPins"][0]["Connections"][0].clone();
    value["Packages"][0]["Models"][0]["Properties"]["OutputPins"][0]["Connections"]
        .as_array_mut()
        .unwrap()
        .push(connection);
    let graph = read(value);
    assert_eq!(graph.edges.len(), 2);
    assert_eq!(graph.edges[0].index, 0);
    assert_eq!(graph.edges[1].index, 1);
    assert_ne!(graph.edges[0].id, graph.edges[1].id);
    assert_eq!(graph.choices.len(), 2);
}

#[test]
fn branches_spanning_two_output_pins_keep_their_source_pin_ids() {
    let mut value = linked();
    let mut second = value["Packages"][0]["Models"][0]["Properties"]["OutputPins"][0].clone();
    second["Id"] = json!("out2");
    value["Packages"][0]["Models"][0]["Properties"]["OutputPins"]
        .as_array_mut()
        .unwrap()
        .push(second);
    let graph = read(value);
    assert_eq!(graph.choices[0].source_pin, "out");
    assert_eq!(graph.choices[1].source_pin, "out2");
    assert_eq!(graph.nodes[0].output_pins, ["out", "out2"]);
}

#[test]
fn input_pin_connections_do_not_create_false_output_branch_candidates() {
    let graph = read(export(json!([
        {"Type":"FlowFragment","Properties":{"Id":"f","InputPins":[{"Id":"entry","Owner":"f","Connections":[{"Target":"line","TargetPin":"in"}]}],"OutputPins":[{"Id":"exit","Owner":"f","Connections":[{"Target":"line","TargetPin":"in"}]}]}},
        {"Type":"DialogueFragment","Properties":{"Id":"line","InputPins":[{"Id":"in","Owner":"line"}]}}
    ])));
    assert_eq!(graph.edges.len(), 2);
    assert!(graph.choices.is_empty());
}

#[test]
fn choices_reference_large_target_text_instead_of_copying_it_per_edge() {
    let mut value = linked();
    value["Packages"][0]["Models"][1]["Properties"]["Text"] = json!("x".repeat(100_000));
    let connection =
        value["Packages"][0]["Models"][0]["Properties"]["OutputPins"][0]["Connections"][0].clone();
    value["Packages"][0]["Models"][0]["Properties"]["OutputPins"][0]["Connections"] =
        json!(vec![connection; 100]);
    let graph = read(value);
    assert_eq!(graph.choices.len(), 100);
    let choices = serde_json::to_vec(&graph.choices).unwrap();
    assert!(choices.len() < 40_000);
}

#[test]
fn condition_instruction_and_pin_script_roles_survive_without_evaluation() {
    let graph = read(export(json!([
        {"Type":"Condition","Properties":{"Id":"condition","Expression":"SyntheticUndefinedFunction()","OutputPins":[{"Id":"true","Owner":"condition","Text":"A = 1;"},{"Id":"false","Owner":"condition","Text":""}]}},
        {"Type":"Instruction","Properties":{"Id":"instruction","Expression":"SyntheticUndefinedFunction();"}}
    ])));
    assert_eq!(
        graph.nodes[0].script.as_ref().unwrap().role,
        ScriptRole::Condition
    );
    assert_eq!(
        graph.nodes[1].script.as_ref().unwrap().role,
        ScriptRole::Instruction
    );
    assert_eq!(
        graph.pins[0].script.as_ref().unwrap().role,
        ScriptRole::Instruction
    );
    assert!(graph.pins[1].script.is_none());
}

#[test]
fn missing_expressions_are_warned_and_blank_expressions_remain_blank() {
    let graph = read(export(
        json!([node("missing","Condition"),{"Type":"Instruction","Properties":{"Id":"blank","Expression":""}}]),
    ));
    assert!(graph
        .warnings
        .iter()
        .any(|warning| warning.kind == WarningKind::MissingScript));
    assert_eq!(graph.nodes[1].script.as_ref().unwrap().text, "");
}

#[test]
fn parent_speaker_and_hierarchy_pin_references_are_structural_errors() {
    for field in ["Parent", "Speaker"] {
        let mut value = linked();
        value["Packages"][0]["Models"][0]["Properties"][field] = json!("in");
        assert!(fails(value));
    }
    let mut value = linked();
    value["Hierarchy"] = json!({"Id":"out"});
    assert!(fails(value));
}

#[test]
fn hierarchy_parent_conflicts_warn_or_fail_in_strict_mode() {
    let value = json!({"Packages":[{"Name":"x","Models":[node("a","Hub"),node("b","Hub"),{"Type":"Hub","Properties":{"Id":"c","Parent":"b"}}]}],"Hierarchy":{"Id":"a","Children":[{"Id":"c"}]}});
    let graph = read(value.clone());
    assert!(graph
        .warnings
        .iter()
        .any(|warning| warning.kind == WarningKind::HierarchyParentMismatch));
    assert!(read_export(
        Cursor::new(serde_json::to_vec(&value).unwrap()),
        &ImportOptions {
            strict_references: true,
            ..ImportOptions::default()
        }
    )
    .is_err());
}

#[test]
fn variable_and_hierarchy_entry_limits_are_enforced() {
    let sample: Value = serde_json::from_str(SAMPLE).unwrap();
    for options in [
        ImportOptions {
            max_variables: 1,
            ..ImportOptions::default()
        },
        ImportOptions {
            max_hierarchy_entries: 1,
            ..ImportOptions::default()
        },
    ] {
        assert!(read_export(Cursor::new(serde_json::to_vec(&sample).unwrap()), &options).is_err());
    }
}

#[test]
fn native_scalar_initial_values_and_signed_integer_boundaries_are_exact() {
    let graph = read(
        json!({"Packages":[],"GlobalVariables":[{"Namespace":"N","Variables":[
            {"Variable":"bool","Type":"Boolean","Value":true},
            {"Variable":"int","Type":"Integer","Value":i64::MIN},
            {"Variable":"max","Type":"Integer","Value":i64::MAX.to_string()},
            {"Variable":"float","Type":"Float","Value":0.125}
        ]}]}),
    );
    assert_eq!(graph.variables[1].value.as_i64(), Some(i64::MIN));
    assert_eq!(graph.variables[2].value.as_i64(), Some(i64::MAX));
    assert!(fails(
        json!({"Packages":[],"GlobalVariables":[{"Namespace":"N","Variables":[{"Variable":"v","Type":"Integer","Value":"9223372036854775808"}]}]})
    ));
}

#[test]
fn long_parent_chains_use_iterative_validation() {
    let models: Vec<_> = (0..5000).map(|i| {
        if i == 4999 { node(&format!("n{i}"),"Hub") }
        else { json!({"Type":"Hub","Properties":{"Id":format!("n{i}"),"Parent":format!("n{}",i+1)}}) }
    }).collect();
    let graph = read(export(json!(models)));
    assert_eq!(graph.nodes.len(), 5000);
    assert!(graph.warnings.is_empty());
}

#[test]
fn asset_paths_and_unknown_root_metadata_are_data_not_io_requests() {
    let graph = read(
        json!({"Packages":[{"Name":"x","Models":[{"Type":"Asset","Properties":{"Id":"asset","Filename":"synthetic.png"},"AssetRef":"https://example.invalid/never-fetched"}]}],"SyntheticExtra":{"url":"https://example.invalid/never-fetched"}}),
    );
    assert_eq!(
        graph.nodes[0].metadata["AssetRef"],
        "https://example.invalid/never-fetched"
    );
    assert_eq!(
        graph.metadata["SyntheticExtra"]["url"],
        "https://example.invalid/never-fetched"
    );
}

#[test]
fn malformed_optional_fields_and_connections_are_not_silently_ignored() {
    for replacement in [json!({}), json!("not an array"), json!(true)] {
        let mut value = linked();
        value["Packages"][0]["Models"][0]["Properties"]["OutputPins"] = replacement;
        assert!(fails(value));
    }
    for field in ["Target", "TargetPin"] {
        let mut value = linked();
        value["Packages"][0]["Models"][0]["Properties"]["OutputPins"][0]["Connections"][0][field] =
            json!("0x0000000000000000");
        assert!(fails(value));
    }
}
