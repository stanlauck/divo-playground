# SPDX-License-Identifier: MIT OR Apache-2.0
"""Deterministic import report."""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from typing import Any


@dataclass
class ImportReport:
    created: list[str] = field(default_factory=list)
    updated: list[str] = field(default_factory=list)
    unchanged: list[str] = field(default_factory=list)
    orphans: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)
    errors: list[Any] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        return {
            "created": sorted(self.created),
            "updated": sorted(self.updated),
            "unchanged": sorted(self.unchanged),
            "orphans": sorted(self.orphans),
            "warnings": sorted(self.warnings),
            "errors": sorted(self.errors, key=lambda item: json.dumps(item, sort_keys=True, ensure_ascii=False) if isinstance(item, dict) else str(item)),
        }

    def to_json(self) -> str:
        return json.dumps(self.to_dict(), ensure_ascii=False, sort_keys=True, separators=(",", ":"))
