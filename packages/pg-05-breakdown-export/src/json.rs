// SPDX-License-Identifier: MIT OR Apache-2.0
use crate::{Breakdown, Error, Result, MAX_INPUT_BYTES};
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use std::{cell::Cell, fmt, io::Read, rc::Rc};

/// Bounded, unique-key JSON before the typed decoder, including unknown values.
pub fn from_json<R: Read>(reader: R) -> Result<Breakdown> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_INPUT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::new("read", "$"))?;
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(Error::new("input_limit", "$"));
    }
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
    let mut decoder = serde_json::Deserializer::from_slice(bytes);
    let value = Seed {
        depth: 0,
        count: Rc::new(Cell::new(0)),
    }
    .deserialize(&mut decoder)
    .map_err(|_| Error::new("json", "$"))?;
    decoder.end().map_err(|_| Error::new("json", "$"))?;
    let value: Breakdown = serde_json::from_value(value).map_err(|_| Error::new("shape", "$"))?;
    value.validate()?;
    Ok(value)
}
struct Seed {
    depth: usize,
    count: Rc<Cell<usize>>,
}
impl<'de> DeserializeSeed<'de> for Seed {
    type Value = Value;
    fn deserialize<D: de::Deserializer<'de>>(
        self,
        decoder: D,
    ) -> std::result::Result<Value, D::Error> {
        if self.depth > 16 || self.count.get() >= 500_000 {
            return Err(de::Error::custom("JSON limit"));
        }
        self.count.set(self.count.get() + 1);
        decoder.deserialize_any(self)
    }
}
impl Seed {
    fn child(&self) -> Self {
        Self {
            depth: self.depth + 1,
            count: Rc::clone(&self.count),
        }
    }
}
impl<'de> Visitor<'de> for Seed {
    type Value = Value;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded unique-key JSON")
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
            .ok_or_else(|| E::custom("number"))
    }
    fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<Value, E> {
        Ok(Value::String(v.into()))
    }
    fn visit_string<E: de::Error>(self, v: String) -> std::result::Result<Value, E> {
        Ok(Value::String(v))
    }
    fn visit_unit<E: de::Error>(self) -> std::result::Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut values: A) -> std::result::Result<Value, A::Error> {
        let mut result = Vec::new();
        while let Some(value) = values.next_element_seed(self.child())? {
            result.push(value);
        }
        Ok(Value::Array(result))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut values: A) -> std::result::Result<Value, A::Error> {
        let mut result = Map::new();
        while let Some(key) = values.next_key::<String>()? {
            if result.contains_key(&key) {
                return Err(de::Error::custom("duplicate key"));
            }
            result.insert(key, values.next_value_seed(self.child())?);
        }
        Ok(Value::Object(result))
    }
}
