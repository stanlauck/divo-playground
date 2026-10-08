# SPDX-License-Identifier: MIT OR Apache-2.0
"""Offline checks, with optional real Godot 4.7.2 import and runtime tests."""

import argparse
import os
import pathlib
import re
import subprocess
import sys
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]


def run(command):
    result = subprocess.run(
        command, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        text=True, encoding="utf-8", errors="replace", timeout=120, check=False,
    )
    print(result.stdout, end="")
    # Godot may return zero after a script fails; exit status alone is insufficient.
    plain_output = re.sub(r"\x1b\[[0-9;]*[mK]", "", result.stdout)
    if result.returncode or re.search(r"(?m)^(?:SCRIPT ERROR:|ERROR:|WARNING:|FAIL:)", plain_output):
        raise RuntimeError("Validation subprocess failed.")
    return result.stdout


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--godot", default=os.environ.get("GODOT_BIN"))
    arguments = parser.parse_args()
    tests = unittest.defaultTestLoader.discover(str(ROOT / "tests"), pattern="test_*.py")
    if not unittest.TextTestRunner(verbosity=2).run(tests).wasSuccessful():
        return 1
    run([sys.executable, "-m", "gdtoolkit.formatter", "--check", "addons", "tests", "examples"])
    run([sys.executable, "-m", "gdtoolkit.linter", "addons", "tests", "examples"])
    if not arguments.godot:
        print("Godot checks SKIPPED: pass --godot or set GODOT_BIN (Godot 4.7.2 stable).")
        return 0
    version = run([arguments.godot, "--version"]).strip()
    if not version.startswith("4.7.2.stable."):
        raise RuntimeError("The real-engine test suite supports Godot 4.7.2 stable only.")
    run([arguments.godot, "--headless", "--editor", "--path", str(ROOT), "--import"])
    output = run(
        [arguments.godot, "--headless", "--path", str(ROOT), "--script", str(ROOT / "tests/run.gd")]
    )
    if not re.search(r"PG-04: \d+ tests passed, \d+ assertions, 0 failed", output):
        raise RuntimeError("Godot test summary missing.")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, subprocess.TimeoutExpired, RuntimeError) as error:
        print(f"PG-04 validation failed: {type(error).__name__}", file=sys.stderr)
        raise SystemExit(1)
