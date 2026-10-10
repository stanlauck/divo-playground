# SPDX-License-Identifier: MIT OR Apache-2.0
"""UnrealEditor-Cmd entry point for headless Storyworld imports."""

from __future__ import annotations

import argparse
import json
import sys


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Import Storyworld JSON into the current UE level")
    parser.add_argument("path", help="path to a Storyworld JSON file")
    parser.add_argument("--delete-orphans", action="store_true")
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args(argv)
    from storyworld_importer import import_storyworld

    report = import_storyworld(args.path, delete_orphans=args.delete_orphans, dry_run=args.dry_run)
    print(report.to_json())
    return 1 if report.errors else 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
