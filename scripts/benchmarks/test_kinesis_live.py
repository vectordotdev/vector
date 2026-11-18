"""Offline checks for live-test validation and resource ownership."""

import base64
import gzip
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import unittest

import kinesis_live as live


def record(data, key="a", sequence="1"):
    return {"Data": base64.b64encode(data).decode(), "PartitionKey": key,
            "SequenceNumber": sequence}


def kpl(payload, key=b"a"):
    user = b"\x08\x00\x1a" + bytes([len(payload)]) + payload
    message = b"\x0a" + bytes([len(key)]) + key + b"\x1a" + bytes([len(user)]) + user
    return b"\xf3\x89\x9a\xc2" + message + hashlib.md5(message).digest()


class PayloadChecks(unittest.TestCase):
    def test_exact_newline_and_kpl_bytes(self):
        self.assertEqual(live.validate_records([record(b"one\ntwo\n")],
                         [(b"one", "a"), (b"two", "a")], "newline", "none")["original_logs"], 2)
        self.assertEqual(live.validate_records([record(kpl(b"one\n"))],
                         [(b"one\n", "a")], "kpl", "none")["original_logs"], 1)
        with self.assertRaisesRegex(ValueError, "payload"):
            live.validate_records([record(kpl(b"one\n"))], [(b"one", "a")], "kpl", "none")

    def test_missing_duplicate_extra_and_wrong_key_fail(self):
        for data in [b"one\n", b"one\none\n", b"one\ntwo\nextra\n"]:
            with self.subTest(data=data), self.assertRaisesRegex(ValueError, "payload"):
                live.validate_records([record(data)], [(b"one", "a"), (b"two", "a")],
                                      "newline", "none")
        with self.assertRaisesRegex(ValueError, "key"):
            live.validate_records([record(b"one\n", "wrong")], [(b"one", "a")], "newline", "none")
        with self.assertRaisesRegex(ValueError, "key"):
            live.validate_records([record(kpl(b"one"), "wrong")], [(b"one", None)], "kpl", "none")

    def test_checksum_and_unexpected_framing_fail(self):
        broken = bytearray(kpl(b"one"))
        broken[-1] ^= 1
        with self.assertRaisesRegex(ValueError, "checksum"):
            live.validate_records([record(broken)], [(b"one", None)], "kpl", "none")
        with self.assertRaises(ValueError):
            live.validate_records([record(b"one")], [(b"one", None)], "newline", "none")

    def test_compression_at_the_correct_boundary(self):
        for fmt, data in [("newline", gzip.compress(b"one\n")),
                          ("kpl", kpl(gzip.compress(b"one")))]:
            self.assertEqual(live.validate_records([record(data)], [(b"one", "a")],
                             fmt, "gzip")["original_logs"], 1)
        with self.assertRaises(ValueError):
            live.validate_records([record(gzip.compress(kpl(b"one")))],
                                  [(b"one", "a")], "kpl", "gzip")

    def test_identity_mismatch_stops_before_stream_creation(self):
        with self.assertRaisesRegex(ValueError, "account"):
            live.check_identity({"Account": "111111111111"}, "222222222222")

    def test_aggregate_cannot_exceed_configured_event_limit(self):
        with self.assertRaisesRegex(ValueError, "20"):
            live.validate_records([record(b"one\n" * 21)], [(b"one", "a")] * 21,
                                  "newline", "none")

    def test_dry_run_needs_no_binary_or_aws_login(self):
        command = [sys.executable, str(Path(live.__file__)), "--binary", "/missing/vector",
                   "--account-id", "123456789012", "--region", "us-east-1",
                   "--out", "/missing/results"]
        result = subprocess.run(command, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(json.loads(result.stdout)["execute"])


class StreamOwnership(unittest.TestCase):
    class Aws:
        def __init__(self, fail_create=False, fail_delete=False):
            self.calls = []
            self.fail_create = fail_create
            self.fail_delete = fail_delete

        def call(self, service, action, *args):
            self.calls.append((service, action, *args))
            if action == "create-stream" and self.fail_create:
                raise RuntimeError("name collision")
            if action == "delete-stream" and self.fail_delete:
                raise RuntimeError("delete denied")
            return {}

    def test_owned_stream_is_deleted_on_acceptance_failure(self):
        aws, report = self.Aws(), {}
        with self.assertRaisesRegex(ValueError, "acceptance failed"):
            with live.temporary_stream(aws, "vector-aggregation-live-unique", report):
                raise ValueError("acceptance failed")
        self.assertEqual(report["cleanup"], "deleted")
        self.assertIn(("kinesis", "delete-stream", "--stream-name",
                       "vector-aggregation-live-unique"), aws.calls)

    def test_collision_is_never_deleted(self):
        aws, report = self.Aws(fail_create=True), {}
        with self.assertRaisesRegex(RuntimeError, "name collision"):
            with live.temporary_stream(aws, "vector-aggregation-live-unique", report):
                self.fail("must not enter")
        self.assertFalse(any(call[1] == "delete-stream" for call in aws.calls))

    def test_cleanup_failure_is_reported_and_cannot_pass(self):
        aws, report = self.Aws(fail_delete=True), {}
        with self.assertRaisesRegex(RuntimeError, "delete denied"):
            with live.temporary_stream(aws, "vector-aggregation-live-unique", report):
                pass
        self.assertEqual(report["cleanup"], "failed")
        self.assertIn("delete denied", report["cleanup_error"])

    def test_cleanup_failure_preserves_original_failure(self):
        aws, report = self.Aws(fail_delete=True), {}
        with self.assertRaisesRegex(ValueError, "delivery failed"):
            with live.temporary_stream(aws, "vector-aggregation-live-unique", report):
                raise ValueError("delivery failed")
        self.assertEqual(report["cleanup"], "failed")


if __name__ == "__main__":
    unittest.main()
