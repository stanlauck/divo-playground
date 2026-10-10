# SPDX-License-Identifier: MIT OR Apache-2.0
"""Pure import planning for Storyworld records."""

from __future__ import annotations

import base64
import json
from dataclasses import dataclass
from typing import Any, Mapping

from .model import StoryworldDocument


@dataclass(frozen=True)
class ActorSpec:
    kind: str
    id: str
    label: str
    transform: tuple[float, float, float]
    metadata: dict[str, Any]
    fingerprint: str


@dataclass(frozen=True)
class ImportPlan:
    desired: tuple[ActorSpec, ...]
    create: tuple[ActorSpec, ...]
    update: tuple[ActorSpec, ...]
    unchanged: tuple[ActorSpec, ...]
    orphans: tuple[tuple[str, str], ...]


def _fingerprint(record: Mapping[str, Any]) -> str:
    encoded = json.dumps(record, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    return base64.b64encode(encoded.encode("utf-8")).decode("ascii")


def _positions(document: StoryworldDocument) -> dict[str, tuple[float, float, float]]:
    locations = document.world["locations"]
    positions = {r["id"]: (float(i % 8) * 1000.0, float(i // 8) * 1000.0, 0.0) for i, r in enumerate(locations)}
    return positions


def make_specs(document: StoryworldDocument) -> tuple[ActorSpec, ...]:
    positions = _positions(document)
    specs: list[ActorSpec] = []
    for record in document.world["locations"]:
        specs.append(ActorSpec("location", record["id"], f"SW_location_{record['id']}", positions[record["id"]], record, _fingerprint(record)))
    for record in document.world["exits"]:
        a, b = positions[record["from"]], positions[record["to"]]
        transform = tuple((a[i] + b[i]) / 2.0 for i in range(3))
        specs.append(ActorSpec("exit", record["id"], f"SW_exit_{record['id']}", transform, record, _fingerprint(record)))
    for i, record in enumerate(document.world["items"]):
        base = positions[record["location"]] if record["location"] is not None else (0.0, 0.0, 0.0)
        transform = (base[0] + 180.0 + (i % 4) * 80.0, base[1] + 180.0 + (i // 4) * 80.0, base[2])
        specs.append(ActorSpec("item", record["id"], f"SW_item_{record['id']}", transform, record, _fingerprint(record)))
    return tuple(specs)


def compute_plan(document: StoryworldDocument, existing: Mapping[str, Mapping[str, Any]] | None = None) -> ImportPlan:
    """Compute a deterministic plan; ``existing`` values contain kind, data tag, label and transform."""
    existing = existing or {}
    desired = make_specs(document)
    create, update, unchanged = [], [], []
    desired_keys = {s.id for s in desired}
    for spec in desired:
        current = existing.get(spec.id)
        if current is None:
            create.append(spec)
        elif (current.get("data") == [spec.fingerprint] and current.get("kind") == spec.kind and current.get("label") == spec.label and current.get("transform") == spec.transform):
            unchanged.append(spec)
        else:
            update.append(spec)
    orphans = tuple((existing[rid]["kind"], rid) for rid in sorted(set(existing) - desired_keys))
    return ImportPlan(desired, tuple(create), tuple(update), tuple(unchanged), orphans)
