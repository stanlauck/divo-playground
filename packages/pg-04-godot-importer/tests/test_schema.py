# SPDX-License-Identifier: MIT OR Apache-2.0
"""Offline JSON Schema and reproducible synthetic-corpus checks."""

import copy
import importlib.util
import json
import pathlib
import unittest

from jsonschema import Draft202012Validator

ROOT = pathlib.Path(__file__).resolve().parents[1]


def load_module(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / "examples" / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class SchemaTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.schema = json.loads((ROOT / "schema.json").read_text(encoding="utf-8"))
        cls.sample = json.loads((ROOT / "samples/declarative.storyworld").read_text(encoding="utf-8"))
        cls.validator = Draft202012Validator(cls.schema)

    def test_schema_is_valid_and_reproducible(self):
        Draft202012Validator.check_schema(self.schema)
        self.assertEqual(self.schema, load_module("make_schema").make_schema())

    def test_samples_are_valid_and_reproducible(self):
        graph = json.loads((ROOT / "samples/pg03-neutral.json").read_text(encoding="utf-8"))
        for name, document in load_module("make_samples").make_samples(graph).items():
            with self.subTest(name=name):
                self.assertEqual(
                    document, json.loads((ROOT / "samples" / name).read_text(encoding="utf-8"))
                )
                if name.endswith(".storyworld"):
                    self.validator.validate(document)
        self.assertEqual(
            graph,
            json.loads((ROOT / "samples/pg03-compatible.storyworld").read_text(encoding="utf-8"))[
                "dialogue"
            ],
        )

    def test_minimal_document(self):
        sample = copy.deepcopy(self.sample)
        sample["world"] = {"locations": [], "exits": [], "items": []}
        sample["dialogue"] = {
            key: value if key in ("version", "text_mode", "metadata") else []
            for key, value in sample["dialogue"].items()
        }
        self.validator.validate(sample)

    def test_unsupported_version_and_operation(self):
        sample = copy.deepcopy(self.sample)
        sample["version"] = 2
        self.assertFalse(self.validator.is_valid(sample))
        sample = copy.deepcopy(self.sample)
        sample["world"]["exits"][0]["condition"]["op"] = "run_script"
        self.assertFalse(self.validator.is_valid(sample))

    def test_structural_unknown_fields(self):
        for target in ([], ["world"], ["dialogue"], ["dialogue", "nodes", 0]):
            sample = copy.deepcopy(self.sample)
            value = sample
            for component in target:
                value = value[component]
            value["unknown_field"] = True
            with self.subTest(path=target):
                self.assertFalse(self.validator.is_valid(sample))

    def test_variable_types(self):
        for kind, wrong in [
            ("boolean", 1), ("integer", True), ("integer", 1.5),
            ("integer", 9223372036854775808), ("float", "1.25"), ("string", False),
        ]:
            sample = copy.deepcopy(self.sample)
            sample["dialogue"]["variables"][0].update(kind=kind, value=wrong)
            with self.subTest(kind=kind, value=wrong):
                self.assertFalse(self.validator.is_valid(sample))

    def test_optional_fields_and_raw_data(self):
        sample = copy.deepcopy(self.sample)
        sample["world"]["items"][0]["location"] = None
        sample["dialogue"]["nodes"][0]["template"] = [None, {"unknown": "data"}]
        sample["dialogue"]["metadata"]["not_executed"] = "load('res://not-real.gd').new()"
        self.validator.validate(sample)

    def test_ordinary_json_does_not_gain_a_remap(self):
        self.assertFalse((ROOT / "samples/pg03-neutral.json.import").exists())


if __name__ == "__main__":
    unittest.main()
