// SPDX-License-Identifier: MIT OR Apache-2.0
use crate::{Category, Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_RECORDS: usize = 10_000;
pub const MAX_REFERENCES: usize = 200_000;
pub const MAX_TEXT_BYTES: usize = 65_536;
pub const MAX_PAGE_EIGHTHS: u32 = 80_000;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Breakdown {
    pub version: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub elements: Vec<Element>,
    /// Narrative order, also the fallback order for unscheduled scenes.
    pub scenes: Vec<Scene>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shooting_days: Vec<ShootingDay>,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Element {
    pub id: String,
    pub category: Category,
    pub name: String,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ElementRef {
    pub element_id: String,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub quantity: u32,
}
fn one() -> u32 {
    1
}
fn is_one(value: &u32) -> bool {
    *value == 1
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IntExt {
    Interior,
    Exterior,
    InteriorExterior,
}
impl IntExt {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Interior => "INT.",
            Self::Exterior => "EXT.",
            Self::InteriorExterior => "INT./EXT.",
        }
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeOfDay {
    Day,
    Night,
    Dawn,
    Dusk,
    Morning,
    Afternoon,
    Evening,
    Continuous,
    Later,
}
impl TimeOfDay {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Day => "DAY",
            Self::Night => "NIGHT",
            Self::Dawn => "DAWN",
            Self::Dusk => "DUSK",
            Self::Morning => "MORNING",
            Self::Afternoon => "AFTERNOON",
            Self::Evening => "EVENING",
            Self::Continuous => "CONTINUOUS",
            Self::Later => "LATER",
        }
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Scene {
    pub id: String,
    pub number: String,
    pub int_ext: IntExt,
    pub set: String,
    pub time_of_day: TimeOfDay,
    pub pages_eighths: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub synopsis: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub elements: Vec<ElementRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script_day: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ShootingDay {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    pub scenes: Vec<String>,
}

impl Breakdown {
    /// Validate typed API inputs too, before opening or writing any output.
    pub fn validate(&self) -> Result<()> {
        if self.version != 1 {
            return Err(Error::new("version", "$.version"));
        }
        for (name, count) in [
            ("elements", self.elements.len()),
            ("scenes", self.scenes.len()),
            ("shooting_days", self.shooting_days.len()),
        ] {
            if count > MAX_RECORDS {
                return Err(Error::new("record_limit", format!("$.{name}")));
            }
        }
        let mut budget = TextBudget(0);
        if let Some(title) = &self.title {
            budget.text(title, "$.title", true, 1024)?;
        }
        let mut ids = HashSet::new();
        let mut elements = HashMap::new();
        let mut names = HashSet::new();
        for (i, item) in self.elements.iter().enumerate() {
            let p = format!("$.elements[{i}]");
            budget.id(&item.id, &format!("{p}.id"))?;
            if !ids.insert(item.id.as_str()) {
                return Err(Error::new("duplicate_id", format!("{p}.id")));
            }
            budget.text(&item.name, &format!("{p}.name"), true, 1024)?;
            if !names.insert((item.category, item.name.as_str())) {
                return Err(Error::new("duplicate_element_label", format!("{p}.name")));
            }
            elements.insert(item.id.as_str(), item);
        }
        let mut scenes = HashMap::new();
        let mut numbers = HashSet::new();
        let mut references = 0usize;
        for (i, scene) in self.scenes.iter().enumerate() {
            let p = format!("$.scenes[{i}]");
            budget.id(&scene.id, &format!("{p}.id"))?;
            if !ids.insert(scene.id.as_str()) {
                return Err(Error::new("duplicate_id", format!("{p}.id")));
            }
            budget.text(&scene.number, &format!("{p}.number"), true, 32)?;
            if !numbers.insert(scene.number.as_str()) {
                return Err(Error::new("duplicate_scene_number", format!("{p}.number")));
            }
            budget.text(&scene.set, &format!("{p}.set"), true, 1024)?;
            budget.text(
                &scene.synopsis,
                &format!("{p}.synopsis"),
                false,
                MAX_TEXT_BYTES,
            )?;
            budget.text(&scene.notes, &format!("{p}.notes"), false, MAX_TEXT_BYTES)?;
            for (name, text) in [("script_day", &scene.script_day), ("unit", &scene.unit)] {
                if let Some(text) = text {
                    budget.text(text, &format!("{p}.{name}"), true, 1024)?;
                }
            }
            if scene.pages_eighths > MAX_PAGE_EIGHTHS {
                return Err(Error::new("page_limit", format!("{p}.pages_eighths")));
            }
            references += scene.elements.len();
            if references > MAX_REFERENCES {
                return Err(Error::new("reference_limit", format!("{p}.elements")));
            }
            let mut used = HashSet::new();
            for (j, item) in scene.elements.iter().enumerate() {
                let q = format!("{p}.elements[{j}]");
                budget.id(&item.element_id, &format!("{q}.element_id"))?;
                if !elements.contains_key(item.element_id.as_str()) {
                    return Err(Error::new("element_reference", format!("{q}.element_id")));
                }
                if !used.insert(item.element_id.as_str()) {
                    return Err(Error::new("duplicate_reference", format!("{q}.element_id")));
                }
                if item.quantity == 0 || item.quantity > 1_000_000 {
                    return Err(Error::new("quantity", format!("{q}.quantity")));
                }
            }
            scenes.insert(scene.id.as_str(), i);
        }
        let mut assigned = HashSet::new();
        for (i, day) in self.shooting_days.iter().enumerate() {
            let p = format!("$.shooting_days[{i}]");
            budget.id(&day.id, &format!("{p}.id"))?;
            if !ids.insert(day.id.as_str()) {
                return Err(Error::new("duplicate_id", format!("{p}.id")));
            }
            budget.text(&day.label, &format!("{p}.label"), true, 1024)?;
            if let Some(date) = &day.date {
                budget.text(date, &format!("{p}.date"), true, 10)?;
                if !valid_date(date) {
                    return Err(Error::new("date", format!("{p}.date")));
                }
            }
            if day.scenes.len() > MAX_RECORDS {
                return Err(Error::new("record_limit", format!("{p}.scenes")));
            }
            for (j, id) in day.scenes.iter().enumerate() {
                let q = format!("{p}.scenes[{j}]");
                budget.id(id, &q)?;
                if !scenes.contains_key(id.as_str()) {
                    return Err(Error::new("scene_reference", q));
                }
                if !assigned.insert(id.as_str()) {
                    return Err(Error::new("duplicate_assignment", q));
                }
            }
        }
        Ok(())
    }
    pub(crate) fn ordered_scenes(&self) -> Vec<(&Scene, Option<&ShootingDay>)> {
        let scenes: HashMap<_, _> = self
            .scenes
            .iter()
            .map(|scene| (scene.id.as_str(), scene))
            .collect();
        let mut used = HashSet::new();
        let mut result = Vec::with_capacity(self.scenes.len());
        for day in &self.shooting_days {
            for id in &day.scenes {
                result.push((scenes[id.as_str()], Some(day)));
                used.insert(id.as_str());
            }
        }
        result.extend(
            self.scenes
                .iter()
                .filter(|s| !used.contains(s.id.as_str()))
                .map(|s| (s, None)),
        );
        result
    }
}
struct TextBudget(usize);
impl TextBudget {
    fn text(&mut self, value: &str, path: &str, single_line: bool, max: usize) -> Result<()> {
        if value.len() > max {
            return Err(Error::new("text_limit", path));
        }
        if single_line
            && (value.trim().is_empty()
                || value
                    .chars()
                    .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}')))
        {
            return Err(Error::new("text", path));
        }
        if value.chars().any(|c| !matches!(c as u32, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)) {
            return Err(Error::new("xml_character", path));
        }
        self.0 += value.len();
        if self.0 > MAX_INPUT_BYTES {
            return Err(Error::new("text_budget", "$"));
        }
        Ok(())
    }
    fn id(&mut self, value: &str, path: &str) -> Result<()> {
        self.text(value, path, true, 128)?;
        if !value.as_bytes()[0].is_ascii_alphanumeric()
            || !value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_.:-".contains(&c))
        {
            return Err(Error::new("id", path));
        }
        Ok(())
    }
}
fn valid_date(value: &str) -> bool {
    let b = value.as_bytes();
    if b.len() != 10
        || b[4] != b'-'
        || b[7] != b'-'
        || !b
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return false;
    }
    let year: u32 = value[..4].parse().unwrap_or(0);
    let month: usize = value[5..7].parse().unwrap_or(0);
    let day: u32 = value[8..].parse().unwrap_or(0);
    if year == 0 || !(1..=12).contains(&month) {
        return false;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    day > 0 && day <= days[month - 1]
}
