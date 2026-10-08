# SPDX-License-Identifier: MIT OR Apache-2.0
"""Write the version-1 JSON Schema to stdout; no network or code generation."""

import json


def ref(name):
    return {"$ref": f"#/$defs/{name}"}


def obj(required, optional=None):
    return {
        "type": "object",
        "required": list(required),
        "properties": {**required, **(optional or {})},
        "additionalProperties": False,
    }


def array(items, unique=False, nonempty=False):
    return {
        "type": "array",
        "items": items,
        "maxItems": 50_000,
        **({"uniqueItems": True} if unique else {}),
        **({"minItems": 1} if nonempty else {}),
    }


def nullable(schema):
    return {"anyOf": [schema, {"type": "null"}]}


def make_schema():
    string = {"type": "string"}
    optional_string = nullable(string)
    metadata = {"type": "object"}
    index = {"type": "integer", "minimum": 0, "maximum": 9223372036854775807}
    integer = {"type": "integer", "minimum": -9223372036854775808, "maximum": 9223372036854775807}
    version = {"type": "integer", "const": 1}
    id_ref = ref("id")
    node_id = ref("nonzeroId")
    optional_id = nullable(node_id)
    extensions = {"condition": ref("condition"), "event": ref("event")}
    definitions = {
        "id": {
            "type": "string",
            "minLength": 1,
            "maxLength": 128,
            "pattern": r"^(?!\s)(?!.*[\u0000-\u001f\u007f-\u009f]).*?(?<!\s)$",
            "description": "Trimmed, control-free, at most 128 UTF-8 bytes (runtime enforces bytes).",
        },
        "nonzeroId": {
            "allOf": [id_ref, {"not": {"pattern": r"^0[xX]0+$"}}],
        },
        "variableReference": obj({"namespace": id_ref, "name": id_ref}),
        "condition": {
            "oneOf": [
                obj(
                    {"op": {"enum": ["eq", "ne"]}, "variable": ref("variableReference"), "value": {}}
                ),
                obj(
                    {"op": {"enum": ["all", "any"]}, "conditions": array(ref("condition"), nonempty=True)}
                ),
                obj({"op": {"const": "not"}, "condition": ref("condition")}),
            ],
            "description": "Declarative data only. Runtime checks depth, variable existence and value type.",
        },
        "event": {
            "oneOf": [
                obj({"op": {"const": "set"}, "variable": ref("variableReference"), "value": {}}),
                obj({"op": {"const": "emit"}, "name": id_ref}, {"payload": {}}),
            ],
        },
        "location": obj(
            {"id": id_ref, "name": string},
            {
                "description": string,
                "dialogue": optional_id,
                "metadata": metadata,
                **extensions,
            },
        ),
        "exit": obj(
            {"id": id_ref, "from": id_ref, "to": id_ref},
            {"name": string, "metadata": metadata, **extensions},
        ),
        "item": obj(
            {"id": id_ref, "name": string, "location": nullable(id_ref)},
            {
                "description": string,
                "dialogue": optional_id,
                "metadata": metadata,
                **extensions,
            },
        ),
        "world": obj(
            {
                "locations": array(ref("location")),
                "exits": array(ref("exit")),
                "items": array(ref("item")),
            }
        ),
        "script": obj(
            {"language": string, "role": {"enum": ["condition", "instruction"]}, "text": string}
        ),
        "package": obj(
            {
                "index": index,
                "name": string,
                "metadata": metadata,
                "node_ids": array(node_id, unique=True),
            }
        ),
        "node": obj(
            {
                "id": node_id,
                "package": index,
                "source_type": id_ref,
                "kind": {
                    "enum": [
                        "flow_fragment", "dialogue", "dialogue_fragment", "hub", "jump",
                        "condition", "instruction", "entity", "user_folder", "comment",
                        "asset", "other",
                    ]
                },
                "parent": optional_id,
                "technical_name": optional_string,
                "display_name": optional_string,
                "text": optional_string,
                "menu_text": optional_string,
                "speaker": optional_id,
                "script": nullable(ref("script")),
                "input_pins": array(node_id, unique=True),
                "output_pins": array(node_id, unique=True),
                "properties": metadata,
                "template": {},
                "metadata": metadata,
            },
            extensions,
        ),
        "pin": obj(
            {
                "id": node_id,
                "owner": node_id,
                "direction": {"enum": ["input", "output"]},
                "index": index,
                "script": nullable(ref("script")),
                "properties": metadata,
            }
        ),
        "edge": obj(
            {
                "id": node_id,
                "kind": {"enum": ["connection", "jump"]},
                "source": node_id,
                "source_pin": optional_id,
                "target": node_id,
                "target_pin": optional_id,
                "index": index,
                "label": optional_string,
                "properties": metadata,
            }
        ),
        "choice": obj(
            {
                "edge": node_id,
                "source": node_id,
                "source_pin": node_id,
                "target": node_id,
                "text_source": {
                    "enum": ["edge_label", "target_menu_text", "target_text", "none"]
                },
            },
            extensions,
        ),
        "variableNamespace": obj(
            {"name": id_ref, "metadata": metadata, "variables": array(id_ref, unique=True)}
        ),
        "variable": {
            **obj(
                {
                    "namespace": id_ref,
                    "name": id_ref,
                    "source_type": id_ref,
                    "kind": {"enum": ["boolean", "integer", "float", "string", "other"]},
                    "value": {},
                    "raw_value": {},
                    "description": optional_string,
                    "metadata": metadata,
                }
            ),
            "allOf": [
                {
                    "if": {"properties": {"kind": {"const": kind}}},
                    "then": {"properties": {"value": schema}},
                }
                for kind, schema in [
                    ("boolean", {"type": "boolean"}),
                    ("integer", integer),
                    ("float", {"type": "number"}),
                    ("string", string),
                ]
            ],
        },
        "hierarchyEntry": obj(
            {
                "id": node_id,
                "parent": optional_id,
                "index": index,
                "depth": {**index, "maximum": 64},
                "properties": metadata,
            }
        ),
        "pg03Warning": obj(
            {
                "kind": {
                    "enum": [
                        "missing_node", "missing_pin", "missing_parent", "missing_speaker",
                        "missing_hierarchy_object", "hierarchy_parent_mismatch", "unknown_type",
                        "unknown_variable_type", "missing_script", "localization_keys",
                    ]
                },
                "source": nullable(id_ref),
                "reference": nullable(id_ref),
            }
        ),
        "dialogue": obj(
            {
                "version": version,
                "text_mode": {"enum": ["literal", "localization_keys"]},
                "metadata": metadata,
                "definitions": array({}),
                "packages": array(ref("package")),
                "nodes": array(ref("node")),
                "pins": array(ref("pin")),
                "edges": array(ref("edge")),
                "choices": array(ref("choice")),
                "variable_namespaces": array(ref("variableNamespace")),
                "variables": array(ref("variable")),
                "hierarchy": array(ref("hierarchyEntry")),
                "warnings": array(ref("pg03Warning")),
            }
        ),
    }
    return {
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "PG-04 Storyworld v1, embedding PG-03 DialogueGraph v1",
        "description": (
            "Shape validation only. The importer additionally checks strict JSON/UTF-8, "
            "numeric representability, byte/depth budgets, IDs, references, ownership, "
            "hierarchy and typed variable expressions. Raw JSON fields remain data only."
        ),
        **obj({"version": version, "world": ref("world"), "dialogue": ref("dialogue")}),
        "$defs": definitions,
    }


if __name__ == "__main__":
    print(json.dumps(make_schema(), ensure_ascii=False, indent=2))
