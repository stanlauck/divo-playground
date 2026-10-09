// SPDX-License-Identifier: MIT OR Apache-2.0
use pg_05_breakdown_export::*;
use roxmltree::Document;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

fn fixture() -> Breakdown {
    from_json(include_bytes!("../samples/synthetic.json").as_slice()).unwrap()
}
fn rows(csv: &str) -> Vec<csv::StringRecord> {
    csv::Reader::from_reader(csv.as_bytes())
        .records()
        .map(|r| r.unwrap())
        .collect()
}
#[test]
fn fdx_has_numbered_scene_headings_summary_lengths_and_start_pages() {
    let value = fixture();
    let xml = to_fdx(&value).unwrap();
    let doc = Document::parse(&xml).unwrap();
    let root = doc.root_element();
    assert_eq!(root.tag_name().name(), "FinalDraft");
    assert_eq!(root.attribute("DocumentType"), Some("Script"));
    assert_eq!(root.attribute("Version"), Some("1"));
    let headings: Vec<_> = doc
        .descendants()
        .filter(|n| n.has_tag_name("Paragraph") && n.attribute("Type") == Some("Scene Heading"))
        .collect();
    assert_eq!(headings.len(), 3);
    for (i, scene) in value.scenes.iter().enumerate() {
        assert_eq!(headings[i].attribute("Number"), Some(scene.number.as_str()));
        let properties = headings[i]
            .children()
            .find(|n| n.has_tag_name("SceneProperties"))
            .unwrap();
        assert_eq!(
            properties.attribute("Length").unwrap(),
            format!("{}/8", scene.pages_eighths)
        );
        assert_eq!(properties.attribute("Number"), Some(scene.number.as_str()));
        assert_eq!(properties.attribute("Page").unwrap(), ["1", "2", "3"][i]);
        assert_eq!(
            headings[i]
                .children()
                .find(|n| n.has_tag_name("Text"))
                .unwrap()
                .text()
                .unwrap(),
            format!(
                "{} {} - {}",
                scene.int_ext.slug(),
                scene.set,
                scene.time_of_day.slug()
            )
        );
    }
    assert_eq!(
        doc.descendants()
            .filter(|n| n.has_tag_name("Summary"))
            .count(),
        2
    );
    assert!(xml.contains("quantity 2"));
}
#[test]
fn all_tag_references_resolve_to_correct_category_and_exact_label() {
    let xml = to_fdx(&fixture()).unwrap();
    let doc = Document::parse(&xml).unwrap();
    let categories: HashMap<_, _> = doc
        .descendants()
        .filter(|n| n.has_tag_name("TagCategory"))
        .map(|n| (n.attribute("Id").unwrap(), n.attribute("Name").unwrap()))
        .collect();
    assert_eq!(
        categories["01fc9642-84ff-4366-b37c-a3068dee57e8"],
        "Cast Members"
    );
    assert_eq!(categories["05c556eb-6bc1-4a3a-b09f-f8b5ba1b6afa"], "Props");
    assert_eq!(
        categories["8e5e75c2-713b-47df-a75f-f12648b98ded"],
        "Synopsis"
    );
    let mut definitions = HashMap::new();
    let mut definition_numbers = HashSet::new();
    for node in doc
        .descendants()
        .filter(|n| n.has_tag_name("TagDefinition"))
    {
        let id = node.attribute("Id").unwrap();
        assert_eq!(Uuid::parse_str(id).unwrap().get_version_num(), 5);
        assert!(definitions.insert(id, node).is_none());
        assert!(definition_numbers.insert(node.attribute("Number").unwrap()));
        assert!(categories.contains_key(node.attribute("CatId").unwrap()));
    }
    let tags: HashMap<_, _> = doc
        .descendants()
        .filter(|n| n.has_tag_name("Tag"))
        .map(|n| {
            (
                n.attribute("Number").unwrap(),
                n.children()
                    .find(|n| n.has_tag_name("DefId"))
                    .unwrap()
                    .text()
                    .unwrap(),
            )
        })
        .collect();
    assert_eq!(tags.len(), definitions.len());
    for text in doc
        .descendants()
        .filter(|n| n.has_tag_name("Text") && n.attribute("TagNumber").is_some())
    {
        let id = tags[text.attribute("TagNumber").unwrap()];
        assert_eq!(definitions[id].attribute("Label"), text.text());
    }
}
#[test]
fn fdx_preserves_xml_sensitive_unicode_and_all_line_endings() {
    let mut value = fixture();
    value.title = Some("Title < & \" ' 🌟".into());
    value.scenes[0].synopsis = "a<&>\"'\t\r\nb العربية".into();
    value.scenes[0].notes = "]\r]> final".into();
    value.elements[1].name = "Cup <&> \" ' עברית".into();
    let xml = to_fdx(&value).unwrap();
    let doc = Document::parse(&xml).unwrap();
    let summary = doc
        .descendants()
        .find(|n| n.has_tag_name("Summary"))
        .unwrap();
    assert_eq!(
        summary
            .descendants()
            .find(|n| n.has_tag_name("Text"))
            .unwrap()
            .text(),
        Some(value.scenes[0].synopsis.as_str())
    );
    assert!(doc.descendants().any(|n| n.has_tag_name("TagDefinition")
        && n.attribute("Label") == Some(value.scenes[0].synopsis.as_str())));
    assert!(doc
        .descendants()
        .any(|n| n.has_tag_name("Text") && n.text() == value.title.as_deref()));
    assert!(!xml.contains("<!DOCTYPE"));
    assert!(!xml.contains("<!ENTITY"));
}
#[test]
fn definition_ids_are_stable_even_when_scene_or_element_input_order_changes() {
    let value = fixture();
    let mut changed = value.clone();
    changed.elements.reverse();
    changed.scenes.reverse();
    fn ids(value: &Breakdown) -> HashMap<String, String> {
        let xml = to_fdx(value).unwrap();
        let doc = Document::parse(&xml).unwrap();
        doc.descendants()
            .filter(|n| n.has_tag_name("TagDefinition"))
            .map(|n| {
                (
                    format!(
                        "{}:{}",
                        n.attribute("CatId").unwrap(),
                        n.attribute("Label").unwrap()
                    ),
                    n.attribute("Id").unwrap().to_string(),
                )
            })
            .collect()
    }
    assert_eq!(ids(&value), ids(&changed));
    assert_eq!(to_fdx(&value).unwrap(), to_fdx(&value).unwrap());
}
#[test]
fn csv_has_fixed_headers_supplied_order_then_unassigned_scenes() {
    let value = fixture();
    let csv = to_csv(&value).unwrap();
    assert!(csv.starts_with("\"order\",\"shooting_day_id\""));
    let headers = csv::Reader::from_reader(csv.as_bytes())
        .headers()
        .unwrap()
        .clone();
    assert_eq!(&headers, CSV_COLUMNS);
    let records = rows(&csv);
    assert_eq!(records.len(), 3);
    assert_eq!(
        records.iter().map(|r| &r[4]).collect::<Vec<_>>(),
        ["yard", "workshop", "doorway"]
    );
    assert_eq!(&records[0][1], "shoot-day-b");
    assert_eq!(&records[0][3], "2026-10-14");
    assert_eq!(&records[1][9], "13");
    assert_eq!(&records[1][10], "1 5/8");
    assert_eq!(&records[2][1], "");
    assert_eq!(&records[2][3], "");
    assert_eq!(&records[2][9], "0");
    assert_eq!(&records[2][10], "0");
    assert!(csv.ends_with("\r\n"));
}
#[test]
fn csv_without_days_uses_narrative_order_and_exact_eighths() {
    let mut value = fixture();
    value.shooting_days.clear();
    let records = rows(&to_csv(&value).unwrap());
    assert_eq!(
        records.iter().map(|r| &r[5]).collect::<Vec<_>>(),
        ["10A", "2", "3"]
    );
    for (i, n) in [0, 1, 7, 8, 9, 80_000].into_iter().enumerate() {
        value.scenes[0].pages_eighths = n;
        let records = rows(&to_csv(&value).unwrap());
        assert_eq!(&records[0][9], n.to_string());
        assert_eq!(
            &records[0][10],
            ["0", "1/8", "7/8", "1", "1 1/8", "10000"][i]
        );
    }
}
#[test]
fn csv_embedded_json_keeps_references_quantities_and_name_delimiters() {
    let mut value = fixture();
    value.elements[1].name = "a; b | c, \"d\" 🌟".into();
    let csv = to_csv(&value).unwrap();
    let records = rows(&csv);
    let elements: serde_json::Value = serde_json::from_str(&records[1][14]).unwrap();
    assert_eq!(elements[1]["name"], value.elements[1].name);
    assert_eq!(elements[1]["id"], "cup");
    assert_eq!(elements[1]["quantity"], 2);
    assert_eq!(&records[1][15], value.scenes[0].notes);
}
#[test]
fn safe_csv_neutralizes_formula_prefixes_and_raw_mode_preserves_cells() {
    let mut value = fixture();
    for text in [
        "=1+1",
        "+SUM(A1)",
        "-2+3",
        "@SUM(A1)",
        " \t\r\n=bad",
        "already ' quoted",
    ] {
        value.scenes[0].synopsis = text.into();
        let safe = rows(&to_csv(&value).unwrap());
        let raw = rows(&to_csv_with_mode(&value, CsvMode::Raw).unwrap());
        assert_eq!(&raw[1][11], text);
        if text == "already ' quoted" {
            assert_eq!(&safe[1][11], text);
        } else {
            assert_eq!(&safe[1][11], format!("'{text}"));
        }
    }
}
#[test]
fn output_size_limit_prevents_large_reference_expansion() {
    let mut value = fixture();
    value.elements[1].name = "&".repeat(1024);
    value.scenes.clear();
    value.shooting_days.clear();
    for i in 0..MAX_RECORDS {
        value.scenes.push(Scene {
            id: format!("scene-{i}"),
            number: i.to_string(),
            int_ext: IntExt::Interior,
            set: "Room".into(),
            time_of_day: TimeOfDay::Day,
            pages_eighths: 1,
            synopsis: String::new(),
            script_day: None,
            unit: None,
            notes: String::new(),
            elements: vec![ElementRef {
                element_id: "cup".into(),
                quantity: 1,
            }],
        });
    }
    value.validate().unwrap();
    // One repeated label is ~51 MiB after XML escaping, still a bounded export.
    assert!(to_fdx(&value).unwrap().len() < MAX_OUTPUT_BYTES);
    value.elements[1].name = "\"".repeat(1024); // &quot; expansion exceeds 64 MiB with multiple references.
    let extra = Element {
        id: "extra".into(),
        category: Category::Props,
        name: "&".repeat(1024),
    };
    value.elements.push(extra);
    for scene in &mut value.scenes {
        scene.elements.push(ElementRef {
            element_id: "extra".into(),
            quantity: 1,
        });
    }
    assert_eq!(to_fdx(&value).unwrap_err().code, "output_limit");
}
#[test]
fn csv_output_limit_is_enforced_even_for_small_source_text() {
    let mut value = fixture();
    value.elements.clear();
    value.scenes.clear();
    value.shooting_days.clear();
    for i in 0..10 {
        value.elements.push(Element {
            id: format!("e-{i}"),
            category: Category::Props,
            name: format!("{i}{}", "x".repeat(1000)),
        });
    }
    for i in 0..MAX_RECORDS {
        value.scenes.push(Scene {
            id: format!("s-{i}"),
            number: i.to_string(),
            int_ext: IntExt::Interior,
            set: "Room".into(),
            time_of_day: TimeOfDay::Day,
            pages_eighths: 0,
            synopsis: String::new(),
            script_day: None,
            unit: None,
            notes: String::new(),
            elements: value
                .elements
                .iter()
                .map(|e| ElementRef {
                    element_id: e.id.clone(),
                    quantity: 1,
                })
                .collect(),
        });
    }
    value.validate().unwrap();
    assert_eq!(to_csv(&value).unwrap_err().code, "output_limit");
}
#[test]
fn every_supported_category_uses_a_distinct_standard_fdx_identity() {
    let categories = [
        Category::Cast,
        Category::BackgroundActors,
        Category::Stunts,
        Category::Vehicles,
        Category::Props,
        Category::Camera,
        Category::SpecialEffects,
        Category::Wardrobe,
        Category::MakeupHair,
        Category::Animals,
        Category::AnimalWrangler,
        Category::Music,
        Category::Sound,
        Category::ArtDepartment,
        Category::SetDressing,
        Category::Greenery,
        Category::SpecialEquipment,
        Category::Security,
        Category::AdditionalLabor,
        Category::VisualEffects,
        Category::MechanicalEffects,
        Category::Miscellaneous,
    ];
    let mut value = fixture();
    value.elements.clear();
    value.scenes[0].elements.clear();
    value.scenes.truncate(1);
    value.shooting_days.clear();
    for (i, category) in categories.into_iter().enumerate() {
        let id = format!("element-{i}");
        value.elements.push(Element {
            id: id.clone(),
            category,
            name: format!("Item {i}"),
        });
        value.scenes[0].elements.push(ElementRef {
            element_id: id,
            quantity: 1,
        });
    }
    let xml = to_fdx(&value).unwrap();
    let doc = Document::parse(&xml).unwrap();
    let actual: Vec<_> = doc
        .descendants()
        .filter(|n| n.has_tag_name("TagCategory"))
        .collect();
    // Five additional supplied metadata categories: synopsis/location/script day/unit/notes.
    assert_eq!(actual.len(), 27);
    let mut ids = HashSet::new();
    let mut numbers = HashSet::new();
    for category in actual {
        assert!(ids.insert(category.attribute("Id").unwrap()));
        assert!(numbers.insert(category.attribute("Number").unwrap()));
    }
    assert!(ids.contains("12ab0932-e3b9-4b4a-bcd0-3da1b4e61d5e"));
    assert!(ids.contains("0ae40617-cc7c-48e6-ae2b-5aaecc09986f"));
}
#[test]
fn shared_location_script_day_and_unit_have_one_definition_each() {
    let mut value = fixture();
    for scene in &mut value.scenes {
        scene.set = "Kitchen".into();
        scene.script_day = Some("Story 1".into());
        scene.unit = Some("Main".into());
        scene.synopsis = "Shared synopsis".into();
        scene.notes = "Shared note".into();
    }
    let xml = to_fdx(&value).unwrap();
    let doc = Document::parse(&xml).unwrap();
    for category in [
        "c5e89e4d-f83e-4c28-950c-92a63f1b5f26",
        "63c140da-ef2b-491a-b416-b46f461abb89",
        "849f1ebf-5507-4f33-bff6-3a5b4d73be14",
    ] {
        let definitions: Vec<_> = doc
            .descendants()
            .filter(|n| n.has_tag_name("TagDefinition") && n.attribute("CatId") == Some(category))
            .collect();
        assert_eq!(definitions.len(), 1);
        let number = definitions[0].attribute("Number").unwrap();
        assert_eq!(
            doc.descendants()
                .filter(|n| n.has_tag_name("Text") && n.attribute("TagNumber") == Some(number))
                .count(),
            3
        );
    }
    for category in [
        "8e5e75c2-713b-47df-a75f-f12648b98ded",
        "15b6f4fd-4e74-4ad8-9971-b239d88c2997",
    ] {
        assert_eq!(
            doc.descendants()
                .filter(
                    |n| n.has_tag_name("TagDefinition") && n.attribute("CatId") == Some(category)
                )
                .count(),
            3
        );
    }
}
