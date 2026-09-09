"""Fast shell-contract tests; no GitHub calls, releases, or artifact builds."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class ReleaseWorkflowTests(unittest.TestCase):
    def wait(self, head="a" * 40, checks=None):
        with tempfile.TemporaryDirectory() as directory:
            bin_dir = Path(directory)
            gh = bin_dir / "gh"
            gh.write_text(
                "#!/bin/sh\n"
                "if [ \"$2\" = view ]; then printf '%s' \"$TEST_PR\"; "
                "else printf '%s' \"$TEST_CHECKS\"; fi\n"
            )
            gh.chmod(0o755)
            # Stop at the first retry. Missing/pending checks must not return success.
            sleep = bin_dir / "sleep"
            sleep.write_text("#!/bin/sh\nexit 99\n")
            sleep.chmod(0o755)
            env = {
                **os.environ,
                "PATH": f"{bin_dir}:{os.environ['PATH']}",
                "GITHUB_REPOSITORY": "fixture/repo",
                "PR_NUMBER": "1",
                "HEAD_SHA": "a" * 40,
                "TEST_PR": json.dumps({"headRefOid": head, "state": "OPEN"}),
                "TEST_CHECKS": json.dumps(checks or []),
            }
            return subprocess.run(
                ["bash", str(ROOT / "scripts/wait-release-housekeeping.sh")],
                env=env, text=True, capture_output=True, check=False,
            )

    def test_requires_successful_release_validator(self):
        check = {"name": "Validate release state transition", "bucket": "pass"}
        self.assertEqual(self.wait(checks=[check]).returncode, 0)
        self.assertNotEqual(self.wait().returncode, 0)
        self.assertNotEqual(self.wait(checks=[{"name": "unrelated", "bucket": "pass"}]).returncode, 0)
        self.assertNotEqual(self.wait(checks=[{**check, "bucket": "skipping"}]).returncode, 0)
        self.assertNotEqual(self.wait(checks=[{**check, "bucket": "pending"}]).returncode, 0)

    def test_rejects_failed_checks_and_changed_heads(self):
        check = {"name": "Validate release state transition", "bucket": "pass"}
        result = self.wait(head="b" * 40, checks=[check])
        self.assertIn("head changed", result.stderr)
        for bucket in ["fail", "cancel"]:
            result = self.wait(checks=[check, {"name": "CI", "bucket": bucket}])
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("required housekeeping check failed", result.stderr)

    def test_rpm_verifier_uses_the_producers_normalization(self):
        producer = (ROOT / "scripts/package-rpm.sh").read_text()
        consumer = (ROOT / ".github/workflows/publish.yml").read_text()
        self.assertIn('CLEANED_VERSION="${PACKAGE_VERSION//-/.}"', producer)
        self.assertIn('rpm_version="${VECTOR_VERSION//-/.}"', consumer)
        for version in ["0.59.0", "0.59.0-dev", "0.59.0-dev.custom.abcdef"]:
            result = subprocess.run(
                ["bash", "-c", 'printf "%s" "${VECTOR_VERSION//-/.}"'],
                env={**os.environ, "VECTOR_VERSION": version},
                text=True, capture_output=True, check=True,
            )
            self.assertEqual(result.stdout, version.replace("-", "."))

    def test_authentication_precedes_private_git_reads(self):
        script = (ROOT / "scripts/release-housekeeping-pr.sh").read_text()
        self.assertLess(script.index("gh auth setup-git"), script.index("git ls-remote"))

    def test_merge_job_can_read_checks_and_fails_on_api_errors(self):
        workflow = (ROOT / ".github/workflows/release.yml").read_text()
        merge_job = workflow.split("  merge-housekeeping:", 1)[1]
        self.assertIn("checks: read", merge_job)
        self.assertIn("statuses: read", merge_job)
        self.assertIn("actions: read", merge_job)
        result = self.wait(checks="GraphQL: Resource not accessible by integration")
        self.assertEqual(result.returncode, 1)
        self.assertIn("Cannot read required checks", result.stderr)


if __name__ == "__main__":
    unittest.main()
