// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::model::AUX_REEL;
use crate::{Error, Result, ShotList};
use std::collections::{HashMap, HashSet};

pub(super) fn token(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 8
        && text
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
pub(super) fn reserved(text: &str) -> bool {
    matches!(text, "BL" | "BLACK" | "BARS")
}

/// Reserve *all* original short reels before assigning long-reel aliases;
/// later short reels cannot collide with an earlier generated alias.
pub(super) struct Reels<'a> {
    by_original: HashMap<&'a str, String>,
}
impl<'a> Reels<'a> {
    pub(super) fn new(list: &'a ShotList) -> Result<Self> {
        let mut used: HashSet<String> = [AUX_REEL, "BL", "BLACK", "BARS"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        for m in list.shots.iter().filter_map(|s| s.media.as_ref()) {
            if token(m.reel_name()) {
                used.insert(m.reel_name().to_owned());
            }
        }
        let mut by_original = HashMap::new();
        let mut cursor = 1;
        for m in list.shots.iter().filter_map(|s| s.media.as_ref()) {
            let original = m.reel_name();
            if by_original.contains_key(original) {
                continue;
            }
            let alias = if token(original) && !reserved(original) {
                original.to_owned()
            } else {
                loop {
                    let candidate = format!("R{cursor:07}");
                    cursor += 1;
                    if cursor > 10_000_000 {
                        return Err(Error::new("record_limit", "reels"));
                    }
                    if used.insert(candidate.clone()) {
                        break candidate;
                    }
                }
            };
            by_original.insert(original, alias);
        }
        Ok(Self { by_original })
    }
    pub(super) fn get(&self, original: &str) -> Result<&str> {
        self.by_original
            .get(original)
            .map(String::as_str)
            .ok_or_else(|| Error::invalid("reels"))
    }
}
