# SPDX-License-Identifier: MIT OR Apache-2.0
"""Public Python API for the Storyworld UE5 importer."""

from __future__ import annotations

from pathlib import Path
from typing import Any

from .model import StoryworldImportError, StoryworldDocument, load_storyworld
from .plan import compute_plan
from .report import ImportReport

__all__ = ["ImportReport", "StoryworldDocument", "StoryworldImportError", "import_storyworld", "load_storyworld"]


def import_storyworld(path: str | bytes | Path, delete_orphans: bool = False, dry_run: bool = False) -> ImportReport:
    """Validate and import a Storyworld file, returning a report instead of raising."""
    try:
        document = load_storyworld(path)
    except StoryworldImportError as exc:
        return ImportReport(errors=[exc.to_dict()])
    from .unreal_apply import apply_document

    name = Path(path).stem if isinstance(path, (str, Path)) else "storyworld"
    try:
        return apply_document(document, delete_orphans=delete_orphans, dry_run=dry_run, name=name)
    except Exception as exc:  # keep editor menu/CLI failures structured
        return ImportReport(errors=[{"code": "apply", "path": "$", "message": str(exc)}])
