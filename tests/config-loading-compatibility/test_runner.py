"""Regression tests for the compatibility runner's acceptance criteria."""

import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).resolve().parents[2] / "scripts/check-config-loading-compatibility.py"
SPEC = importlib.util.spec_from_file_location("config_compatibility", SCRIPT)
RUNNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUNNER)


class RunnerTests(unittest.TestCase):
    def test_breaking_case_requires_its_explicit_new_events(self):
        case = {
            "breaking": "outer-format escaping is removed",
            "released_status": "accepted",
            "released_messages": ["first\nsecond"],
            "parse_first_status": "accepted",
            "parse_first_messages": [r"first\nsecond"],
        }
        result = {"status": "accepted", "exit_code": 0, "events": []}
        self.assertTrue(RUNNER.expectation_issues(case, result, parse_first=True))
        result["events"] = [{"message": r"first\nsecond"}]
        self.assertEqual(RUNNER.expectation_issues(case, result, parse_first=True), [])

    def test_breaking_case_cannot_reject_a_config_that_should_load(self):
        case = {
            "breaking": "comments are no longer interpolated",
            "released_status": "rejected",
            "parse_first_status": "accepted",
        }
        result = {"status": "rejected", "exit_code": 78}
        self.assertEqual(RUNNER.expectation_issues(case, result), [])
        self.assertTrue(RUNNER.expectation_issues(case, result, parse_first=True))

    def test_cli_error_is_not_a_config_rejection(self):
        case = {"released_status": "rejected"}
        result = {"status": "rejected", "exit_code": 2}
        self.assertTrue(RUNNER.expectation_issues(case, result))
        expected = {"status": "rejected", "exit_code": 78}
        self.assertNotEqual(RUNNER.outcome(result), RUNNER.outcome(expected))

    def test_successful_command_with_no_tests_is_not_a_test_pass(self):
        case = {"released_status": "accepted", "passed_tests": ["expected test"]}
        result = {"status": "accepted", "exit_code": 0, "passed_tests": []}
        self.assertTrue(RUNNER.expectation_issues(case, result))
        result["passed_tests"] = ["expected test", "unexpected second test"]
        self.assertTrue(RUNNER.expectation_issues(case, result))
        result["passed_tests"] = ["expected test"]
        self.assertEqual(RUNNER.expectation_issues(case, result), [])

    def test_test_command_extracts_the_named_pass(self):
        case = {
            "name": "test fixture",
            "mode": "test",
            "files": {"vector.yaml": "runtime.yaml"},
        }
        output = {
            "status": "accepted",
            "exit_code": 0,
            "stdout": "Running tests\ntest expected test ... passed\n",
            "stderr": "",
        }
        with tempfile.TemporaryDirectory() as temporary:
            with patch.object(RUNNER, "execute", return_value=output):
                result = RUNNER.run_case(
                    Path("unused-vector"), case, Path(temporary) / "case", {}, 1
                )
        self.assertEqual(result["passed_tests"], ["expected test"])

    def test_signal_exit_is_a_crash(self):
        self.assertEqual(RUNNER.exit_status(-9), "crashed")
        self.assertEqual(RUNNER.exit_status(0), "accepted")
        self.assertEqual(RUNNER.exit_status(78), "rejected")


if __name__ == "__main__":
    unittest.main()
