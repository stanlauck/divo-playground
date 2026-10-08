# SPDX-License-Identifier: MIT OR Apache-2.0
"""Expand an existing synthetic PG-03 neutral output; never overwrite files."""

import copy
import json
import pathlib
import sys


def make_samples(graph):
    base = {
        "version": 1,
        "world": {
            "locations": [
                {
                    "id": "room",
                    "name": "Учебная комната",
                    "description": "Synthetic room.",
                    "dialogue": "0x0100000000000003",
                    "metadata": {"marker": "res://this-is-data-not-a-loaded-asset"},
                },
                {"id": "yard", "name": "Synthetic yard"},
            ],
            "exits": [
                {
                    "id": "door",
                    "from": "room",
                    "to": "yard",
                    "condition": {
                        "op": "eq",
                        "variable": {"namespace": "Story", "name": "visited"},
                        "value": False,
                    },
                    "event": {"op": "emit", "name": "door_opened", "payload": {"source": "room"}},
                }
            ],
            "items": [
                {
                    "id": "token",
                    "name": "Synthetic token",
                    "location": "room",
                    "event": {
                        "op": "set",
                        "variable": {"namespace": "Story", "name": "coins"},
                        "value": 4,
                    },
                }
            ],
        },
        "dialogue": copy.deepcopy(graph),
    }
    extended = copy.deepcopy(base)
    condition = {
        "op": "all",
        "conditions": [
            {"op": "ne", "variable": {"namespace": "Story", "name": "coins"}, "value": 0},
            {
                "op": "not",
                "condition": {
                    "op": "eq",
                    "variable": {"namespace": "Story", "name": "visited"},
                    "value": True,
                },
            },
        ],
    }
    for node in extended["dialogue"]["nodes"]:
        if node["kind"] == "condition":
            node["condition"] = condition
            node["event"] = {"op": "emit", "name": "gate_checked"}
    extended["dialogue"]["choices"][0]["condition"] = {
        "op": "any",
        "conditions": [
            {"op": "eq", "variable": {"namespace": "Story", "name": "sign"}, "value": "Север 南"},
            {"op": "eq", "variable": {"namespace": "Story", "name": "coins"}, "value": 3},
        ],
    }
    extended["dialogue"]["choices"][0]["event"] = {
        "op": "set",
        "variable": {"namespace": "Story", "name": "visited"},
        "value": True,
    }
    return {
        "pg03-neutral.json": graph,
        "pg03-compatible.storyworld": base,
        "declarative.storyworld": extended,
    }


def main():
    if len(sys.argv) != 3:
        raise ValueError("Usage: make_samples.py <synthetic-pg03-neutral.json> <new-directory>")
    graph = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
    destination = pathlib.Path(sys.argv[2])
    destination.mkdir()
    for name, data in make_samples(graph).items():
        with (destination / name).open("x", encoding="utf-8", newline="\n") as output:
            json.dump(data, output, ensure_ascii=False, indent=2)
            output.write("\n")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, IndexError, TypeError):
        print("Unable to create new synthetic samples; existing paths are not overwritten.", file=sys.stderr)
        raise SystemExit(1)
