# SPDX-License-Identifier: MIT OR Apache-2.0
"""The wrapper must not report zero-exit Godot script failures as success."""

import contextlib
import importlib.util
import io
import pathlib
import subprocess
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location("pg04_runner", pathlib.Path(__file__).with_name("run.py"))
RUNNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUNNER)


class RunnerTests(unittest.TestCase):
    def invoke(self, output, code=0):
        result = subprocess.CompletedProcess(["not-executed"], code, output)
        with mock.patch.object(RUNNER.subprocess, "run", return_value=result), contextlib.redirect_stdout(
            io.StringIO()
        ):
            return RUNNER.run(["not-executed"])

    def test_clean_output(self):
        self.assertEqual("PASS: synthetic\n", self.invoke("PASS: synthetic\n"))

    def test_zero_exit_script_error(self):
        with self.assertRaises(RuntimeError):
            self.invoke("SCRIPT ERROR: synthetic\n")

    def test_zero_exit_engine_error(self):
        with self.assertRaises(RuntimeError):
            self.invoke("ERROR: synthetic\n")

    def test_zero_exit_warning(self):
        with self.assertRaises(RuntimeError):
            self.invoke("WARNING: synthetic\n")

    def test_ansi_colored_error(self):
        with self.assertRaises(RuntimeError):
            self.invoke("\x1b[31mERROR: synthetic\x1b[0m\n")

    def test_failed_exit_without_error_output(self):
        with self.assertRaises(RuntimeError):
            self.invoke("synthetic\n", code=1)


if __name__ == "__main__":
    unittest.main()
