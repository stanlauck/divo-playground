# SPDX-License-Identifier: MIT OR Apache-2.0
"""Regenerate the structural schema offline; Rust enforces reference invariants."""
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
ID = {"type": "string", "minLength": 1, "maxLength": 128,
      "pattern": "^[A-Za-z0-9][A-Za-z0-9_.:-]*$"}
CATEGORIES = [
    "cast", "background_actors", "stunts", "vehicles", "props", "camera",
    "special_effects", "wardrobe", "makeup_hair", "animals", "animal_wrangler",
    "music", "sound", "art_department", "set_dressing", "greenery",
    "special_equipment", "security", "additional_labor", "visual_effects",
    "mechanical_effects", "miscellaneous",
]


def text(maximum, nullable=False):
    return {"type": ["string", "null"] if nullable else "string",
            "maxLength": maximum}


def line(maximum, nullable=False):
    forbidden = "\\u0000-\\u001F\\u007F-\\u009F\\u2028\\u2029"
    return {**text(maximum, nullable), "minLength": 1,
            "pattern": f"^[^{forbidden}]*[^\\s{forbidden}][^{forbidden}]*$(?![\\s\\S])"}


def record(required, properties):
    return {"type": "object", "required": required, "properties": properties,
            "additionalProperties": False}


def array(item):
    return {"type": "array", "maxItems": 10_000, "items": item}


def schema():
    element = record(["id", "category", "name"], {
        "id": ID, "category": {"enum": CATEGORIES}, "name": line(1024)})
    reference = record(["element_id"], {
        "element_id": ID, "quantity": {"type": "integer", "minimum": 1,
                                      "maximum": 1_000_000, "default": 1}})
    scene = record(["id", "number", "int_ext", "set", "time_of_day", "pages_eighths"], {
        "id": ID, "number": line(32),
        "int_ext": {"enum": ["interior", "exterior", "interior_exterior"]},
        "set": line(1024),
        "time_of_day": {"enum": ["day", "night", "dawn", "dusk", "morning",
                               "afternoon", "evening", "continuous", "later"]},
        "pages_eighths": {"type": "integer", "minimum": 0, "maximum": 80_000},
        "synopsis": text(65_536), "notes": text(65_536),
        "elements": {"type": "array", "maxItems": 200_000, "items": reference},
        "script_day": line(1024, True), "unit": line(1024, True),
    })
    day = record(["id", "label", "scenes"], {
        "id": ID, "label": line(1024),
        "date": {"type": ["string", "null"], "format": "date",
                 "pattern": "^[0-9]{4}-[0-9]{2}-[0-9]{2}$"},
        "scenes": {**array(ID), "uniqueItems": True},
    })
    return {
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "PG-05 neutral production breakdown v1",
        "$comment": "Structural schema only. Rust enforces UTF-8 byte limits, XML characters, global IDs/references, unique category/name pairs and single day assignment. Enable format checking for dates.",
        **record(["version", "elements", "scenes"], {
            "version": {"const": 1}, "title": line(1024, True),
            "elements": array({"$ref": "#/$defs/element"}),
            "scenes": array({"$ref": "#/$defs/scene"}),
            "shooting_days": array({"$ref": "#/$defs/shooting_day"}),
        }),
        "$defs": {"element": element, "element_ref": reference,
                  "scene": scene, "shooting_day": day},
    }


if __name__ == "__main__":
    result = json.dumps(schema(), ensure_ascii=False, indent=2) + "\n"
    if sys.argv[1:] == ["--write"]:
        (ROOT / "schema.json").write_text(result, encoding="utf-8", newline="\n")
    elif not sys.argv[1:]:
        sys.stdout.buffer.write(result.encode("utf-8"))
    else:
        raise SystemExit("Usage: make_schema.py [--write]")
