// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};

use crate::error::{Error, Result};

pub const SCHEMA_VERSION: u32 = 1;

/// A localization string table: one source language, at most one target language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StringTable {
    pub version: u32,
    pub source_lang: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_empty"
    )]
    pub target_lang: Option<String>,
    pub strings: Vec<Entry>,
}

/// One translatable line.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub id: String,
    pub source: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_empty"
    )]
    pub target: Option<String>,
    /// Where or how the line is used (scene, UI slot, situation).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_empty"
    )]
    pub context: Option<String>,
    /// Name of the speaking character, if the line is dialogue.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_empty"
    )]
    pub character: Option<String>,
    /// Free-form note for translators.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_empty"
    )]
    pub note: Option<String>,
    /// Maximum target length in Unicode code points.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<State>,
}

/// Translation state, matching XLIFF 2.1 segment states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Initial,
    Translated,
    Reviewed,
    Final,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            State::Initial => "initial",
            State::Translated => "translated",
            State::Reviewed => "reviewed",
            State::Final => "final",
        }
    }
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for State {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "initial" => Ok(State::Initial),
            "translated" => Ok(State::Translated),
            "reviewed" => Ok(State::Reviewed),
            "final" => Ok(State::Final),
            other => Err(Error::Invalid(format!("unknown state {other:?}"))),
        }
    }
}

/// A target that is longer than its entry's `max_length`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LengthViolation {
    pub id: String,
    pub max_length: u32,
    pub actual: usize,
}

impl StringTable {
    pub fn new(source_lang: impl Into<String>, target_lang: Option<String>) -> Self {
        StringTable {
            version: SCHEMA_VERSION,
            source_lang: source_lang.into(),
            target_lang,
            strings: Vec::new(),
        }
    }

    /// Checks the rules every format relies on: schema version, language tags,
    /// unique XML-safe ids, a target language whenever a target exists, and
    /// text that XML 1.0 can carry.
    pub fn validate(&self) -> Result<()> {
        if self.version != SCHEMA_VERSION {
            return Err(Error::Invalid(format!(
                "unsupported version {} (expected {SCHEMA_VERSION})",
                self.version
            )));
        }
        check_lang("source_lang", &self.source_lang)?;
        if let Some(lang) = &self.target_lang {
            check_lang("target_lang", lang)?;
        }

        let mut seen = HashSet::with_capacity(self.strings.len());
        for entry in &self.strings {
            if !is_nmtoken(&entry.id) {
                return Err(Error::Invalid(format!(
                    "id {:?} must be non-empty and use only letters, digits, '.', '-', '_', ':'",
                    entry.id
                )));
            }
            if !seen.insert(entry.id.as_str()) {
                return Err(Error::Invalid(format!("duplicate id {:?}", entry.id)));
            }
            if entry.target.is_some() && self.target_lang.is_none() {
                return Err(Error::Invalid(format!(
                    "entry {:?} has a target but the table has no target_lang",
                    entry.id
                )));
            }
            for (field, value) in entry.text_fields() {
                if let Some(c) = value.chars().find(|&c| !is_xml_char(c)) {
                    return Err(Error::Invalid(format!(
                        "entry {:?} field {field} contains U+{:04X}, which XML 1.0 cannot represent",
                        entry.id, c as u32
                    )));
                }
            }
        }
        Ok(())
    }

    /// Lists targets longer than their `max_length`, counted in code points.
    pub fn length_violations(&self) -> Vec<LengthViolation> {
        self.strings
            .iter()
            .filter_map(|e| {
                let max = e.max_length?;
                let actual = e.target.as_deref()?.chars().count();
                (actual > max as usize).then(|| LengthViolation {
                    id: e.id.clone(),
                    max_length: max,
                    actual,
                })
            })
            .collect()
    }
}

impl Entry {
    pub fn new(id: impl Into<String>, source: impl Into<String>) -> Self {
        Entry {
            id: id.into(),
            source: source.into(),
            ..Entry::default()
        }
    }

    fn text_fields(&self) -> impl Iterator<Item = (&'static str, &str)> {
        [
            ("source", Some(self.source.as_str())),
            ("target", self.target.as_deref()),
            ("context", self.context.as_deref()),
            ("character", self.character.as_deref()),
            ("note", self.note.as_deref()),
        ]
        .into_iter()
        .filter_map(|(name, v)| v.map(|v| (name, v)))
    }
}

/// Maps an empty string to `None`, so `""` and a missing field mean the same.
pub(crate) fn none_if_empty(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.is_empty())
}

fn non_empty<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Option<String>, D::Error> {
    Ok(none_if_empty(Option::<String>::deserialize(d)?))
}

fn check_lang(field: &str, lang: &str) -> Result<()> {
    // A light BCP 47 shape check: alphanumeric subtags of 1-8 chars joined by '-'.
    let ok = !lang.is_empty()
        && lang
            .split('-')
            .all(|t| (1..=8).contains(&t.len()) && t.bytes().all(|b| b.is_ascii_alphanumeric()));
    if ok {
        Ok(())
    } else {
        Err(Error::Invalid(format!(
            "{field} {lang:?} is not a language tag"
        )))
    }
}

// Restricted to ASCII so ids stay portable across tools that are stricter
// than the XML NMTOKEN production.
fn is_nmtoken(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b':'))
}

fn is_xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..)
}
