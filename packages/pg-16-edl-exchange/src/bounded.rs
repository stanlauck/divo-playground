// SPDX-License-Identifier: MIT OR Apache-2.0

// Adapted from this repository's PG-06: standalone, with EDL-specific budgets.
use crate::{Error, Result};
use serde::Serialize;
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use std::cell::Cell;
use std::fmt;
use std::io::{Read, Write};

pub const MAX_JSON_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_EDL_BYTES: usize = 8 * 1024 * 1024;
const MAX_VALUES: usize = 2_000_000;
const MAX_DEPTH: usize = 32;

/// Reads bounded UTF-8; all I/O errors identify the input, not its filename.
pub fn read_input(reader: impl Read, limit: usize) -> Result<String> {
    if limit > MAX_JSON_BYTES.max(MAX_EDL_BYTES) {
        return Err(Error::invalid("limit"));
    }
    let mut bytes = Vec::new();
    reader
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| Error::from(e).at("input"))?;
    if bytes.len() > limit {
        return Err(Error::new("input_limit", "input"));
    }
    String::from_utf8(bytes).map_err(|_| Error::new("invalid_utf8", "input"))
}

pub(crate) fn parse(text: &str, limit: usize) -> Result<Value> {
    if text.len() > limit {
        return Err(Error::new("input_limit", "input"));
    }
    let mut decoder =
        serde_json::Deserializer::from_str(text.strip_prefix('\u{feff}').unwrap_or(text));
    let remaining = Cell::new(MAX_VALUES);
    let value = Seed {
        depth: 0,
        remaining: &remaining,
    }
    .deserialize(&mut decoder)?;
    decoder.end()?;
    Ok(value)
}

struct Seed<'a> {
    depth: usize,
    remaining: &'a Cell<usize>,
}
impl<'de> DeserializeSeed<'de> for Seed<'_> {
    type Value = Value;
    fn deserialize<D: de::Deserializer<'de>>(
        self,
        decoder: D,
    ) -> std::result::Result<Value, D::Error> {
        if self.depth > MAX_DEPTH || self.remaining.get() == 0 {
            return Err(de::Error::custom("JSON budget"));
        }
        self.remaining.set(self.remaining.get() - 1);
        decoder.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Seed<'_> {
    type Value = Value;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded JSON")
    }
    fn visit_unit<E: de::Error>(self) -> std::result::Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_bool<E: de::Error>(self, v: bool) -> std::result::Result<Value, E> {
        Ok(Value::Bool(v))
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> std::result::Result<Value, E> {
        Ok(Value::Number(v.into()))
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> std::result::Result<Value, E> {
        Ok(Value::Number(v.into()))
    }
    fn visit_f64<E: de::Error>(self, v: f64) -> std::result::Result<Value, E> {
        Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| E::custom("finite number"))
    }
    fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<Value, E> {
        Ok(Value::String(v.to_owned()))
    }
    fn visit_string<E: de::Error>(self, v: String) -> std::result::Result<Value, E> {
        Ok(Value::String(v))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> std::result::Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = seq.next_element_seed(Seed {
            depth: self.depth + 1,
            remaining: self.remaining,
        })? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<Value, A::Error> {
        let mut values = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if values.contains_key(&key) {
                return Err(de::Error::custom("duplicate key"));
            }
            let value = map.next_value_seed(Seed {
                depth: self.depth + 1,
                remaining: self.remaining,
            })?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}

pub(crate) struct Output {
    bytes: Vec<u8>,
    limit: usize,
}
impl Output {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
        }
    }
    pub(crate) fn finish(self) -> Result<String> {
        String::from_utf8(self.bytes).map_err(|_| Error::new("invalid_utf8", "output"))
    }
}
impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.limit - self.bytes.len() {
            return Err(std::io::Error::other("output budget"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub(crate) fn json_string(value: &impl Serialize, limit: usize) -> Result<String> {
    let mut out = Output::new(limit);
    serde_json::to_writer(&mut out, value).map_err(|_| Error::new("output_limit", "output"))?;
    out.finish()
}
