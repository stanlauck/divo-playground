// SPDX-License-Identifier: MIT OR Apache-2.0
use crate::{
    category::{FdxCategory, LOCATION, NOTES, SCRIPT_DAY, SYNOPSIS, UNIT},
    Breakdown, Error, Result, MAX_OUTPUT_BYTES,
};
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

// Deterministic namespaces for this carrier's definitions, not security IDs.
const NAMESPACE: Uuid = Uuid::from_u128(0xf74a26b99a6e598ba9c39dfcb8e02051);
struct Definition {
    category: FdxCategory,
    id: Uuid,
    label: String,
}
struct Registry {
    definitions: Vec<Definition>,
    categories: BTreeMap<u8, FdxCategory>,
    element_tags: HashMap<String, usize>,
    scene_tags: Vec<[Option<usize>; 5]>,
}
impl Registry {
    fn new(value: &Breakdown) -> Self {
        let mut registry = Self {
            definitions: Vec::new(),
            categories: BTreeMap::new(),
            element_tags: HashMap::new(),
            scene_tags: Vec::new(),
        };
        for item in &value.elements {
            let number = registry.add(
                item.category.fdx(),
                &format!("element:{}", item.id),
                &item.name,
            );
            registry.element_tags.insert(item.id.clone(), number);
        }
        for scene in &value.scenes {
            let fields = [
                (
                    SYNOPSIS,
                    "synopsis",
                    Some(scene.synopsis.as_str()).filter(|s| !s.is_empty()),
                ),
                (LOCATION, "location", Some(scene.set.as_str())),
                (SCRIPT_DAY, "script_day", scene.script_day.as_deref()),
                (UNIT, "unit", scene.unit.as_deref()),
                (
                    NOTES,
                    "notes",
                    Some(scene.notes.as_str()).filter(|s| !s.is_empty()),
                ),
            ];
            let mut tags = [None; 5];
            for (i, (category, name, label)) in fields.into_iter().enumerate() {
                if let Some(label) = label {
                    tags[i] =
                        Some(registry.add(category, &format!("scene:{}:{name}", scene.id), label));
                }
            }
            registry.scene_tags.push(tags);
        }
        registry
    }
    fn add(&mut self, category: FdxCategory, key: &str, label: &str) -> usize {
        self.categories.insert(category.number, category);
        self.definitions.push(Definition {
            category,
            id: Uuid::new_v5(&NAMESPACE, key.as_bytes()),
            label: label.into(),
        });
        self.definitions.len()
    }
}
struct Xml(String);
impl Xml {
    fn raw(&mut self, value: &str) -> Result<()> {
        if self.0.len().saturating_add(value.len()) > MAX_OUTPUT_BYTES {
            return Err(Error::new("output_limit", "$.fdx"));
        }
        self.0.push_str(value);
        Ok(())
    }
    fn text(&mut self, value: &str) -> Result<()> {
        for c in value.chars() {
            match c {
                '&' => self.raw("&amp;")?,
                '<' => self.raw("&lt;")?,
                '>' => self.raw("&gt;")?,
                '"' => self.raw("&quot;")?,
                '\'' => self.raw("&apos;")?,
                '\r' => self.raw("&#13;")?,
                '\n' => self.raw("&#10;")?,
                '\t' => self.raw("&#9;")?,
                c => {
                    let mut b = [0; 4];
                    self.raw(c.encode_utf8(&mut b))?;
                }
            }
        }
        Ok(())
    }
    fn attr(&mut self, name: &str, value: &str) -> Result<()> {
        self.raw(&format!(" {name}=\""))?;
        self.text(value)?;
        self.raw("\"")
    }
    fn text_node(&mut self, value: &str, tag: Option<usize>) -> Result<()> {
        self.raw("<Text")?;
        if let Some(tag) = tag {
            self.attr("TagNumber", &tag.to_string())?;
        }
        self.raw(">")?;
        self.text(value)?;
        self.raw("</Text>")
    }
}

/// A numbered, tagged FDX Script carrier, not a native MMS schedule or screenplay.
pub fn to_fdx(value: &Breakdown) -> Result<String> {
    value.validate()?;
    let registry = Registry::new(value);
    let mut xml = Xml(String::new());
    xml.raw("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>\n<FinalDraft DocumentType=\"Script\" Template=\"No\" Version=\"1\">\n  <Content>\n")?;
    let mut page_eighths = 0u64;
    let elements: HashMap<_, _> = value.elements.iter().map(|e| (e.id.as_str(), e)).collect();
    for (i, scene) in value.scenes.iter().enumerate() {
        xml.raw("    <Paragraph Type=\"Scene Heading\"")?;
        xml.attr("Number", &scene.number)?;
        xml.raw(">\n      <SceneProperties")?;
        xml.attr("Number", &scene.number)?;
        xml.attr("Length", &format!("{}/8", scene.pages_eighths))?;
        xml.attr("Page", &(page_eighths / 8 + 1).to_string())?;
        xml.attr("Title", "")?;
        xml.raw(">")?;
        if !scene.synopsis.is_empty() {
            xml.raw("<Summary><Paragraph>")?;
            xml.text_node(&scene.synopsis, None)?;
            xml.raw("</Paragraph></Summary>")?;
        }
        xml.raw("<SceneArcBeats/></SceneProperties>\n      ")?;
        xml.text_node(
            &format!(
                "{} {} - {}",
                scene.int_ext.slug(),
                scene.set,
                scene.time_of_day.slug()
            ),
            None,
        )?;
        xml.raw("\n    </Paragraph>\n")?;
        // Tagged stand-in paragraphs make each supplied breakdown item visible to importers.
        for (j, tag) in registry.scene_tags[i].into_iter().enumerate() {
            if let Some(tag) = tag {
                xml.raw("    <Paragraph Type=\"Action\">")?;
                let prefix = [
                    "Synopsis: ",
                    "Location: ",
                    "Script day: ",
                    "Unit: ",
                    "Notes: ",
                ][j];
                xml.text_node(prefix, None)?;
                xml.text_node(&registry.definitions[tag - 1].label, Some(tag))?;
                xml.raw("</Paragraph>\n")?;
            }
        }
        for item in &scene.elements {
            let element = elements[item.element_id.as_str()];
            let tag = registry.element_tags[item.element_id.as_str()];
            xml.raw("    <Paragraph Type=\"Action\">")?;
            xml.text_node(&format!("{}: ", element.category.fdx().name), None)?;
            xml.text_node(&element.name, Some(tag))?;
            if item.quantity != 1 {
                xml.text_node(&format!(" (quantity {})", item.quantity), None)?;
            }
            xml.raw("</Paragraph>\n")?;
        }
        page_eighths += u64::from(scene.pages_eighths);
    }
    xml.raw("  </Content>\n")?;
    if let Some(title) = &value.title {
        xml.raw("  <TitlePage><Content><Paragraph Alignment=\"Center\">")?;
        xml.text_node(title, None)?;
        xml.raw("</Paragraph></Content></TitlePage>\n")?;
    }
    xml.raw("  <TagData>\n    <TagCategories>\n")?;
    for category in registry.categories.values() {
        xml.raw("      <TagCategory")?;
        for (key, value) in [
            ("Id", category.id),
            ("Name", category.name),
            ("Color", "#000000000000"),
            ("Style", "Bold"),
        ] {
            xml.attr(key, value)?;
        }
        xml.attr("Number", &category.number.to_string())?;
        xml.raw("/>\n")?;
    }
    xml.raw("    </TagCategories>\n    <TagDefinitions>\n")?;
    for (i, definition) in registry.definitions.iter().enumerate() {
        xml.raw("      <TagDefinition")?;
        xml.attr("CatId", definition.category.id)?;
        xml.attr("Id", &definition.id.to_string())?;
        xml.attr("Label", &definition.label)?;
        xml.attr("Number", &(i + 1).to_string())?;
        xml.raw("/>\n")?;
    }
    xml.raw("    </TagDefinitions>\n    <Tags>\n")?;
    for (i, definition) in registry.definitions.iter().enumerate() {
        xml.raw(&format!(
            "      <Tag Number=\"{}\"><DefId>{}</DefId></Tag>\n",
            i + 1,
            definition.id
        ))?;
    }
    xml.raw("    </Tags>\n  </TagData>\n  <PageLayout><PageSize Height=\"11.0\" Width=\"8.5\"/></PageLayout>\n</FinalDraft>\n")?;
    Ok(xml.0)
}
