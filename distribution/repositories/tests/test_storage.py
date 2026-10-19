import base64
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timedelta, timezone
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import threading
import unittest
from unittest.mock import Mock

from botocore.exceptions import ClientError

sys.path.insert(0, str(Path(__file__).resolve().parents[1] ))
from storage import (Busy, Conflict, Interrupted, PublicationError, Publisher,
                          S3Store, Snapshot, byte_entry, canonical)


CONFIG = {"live_prefix": "repository/publications/test/live/",
          "archive_prefix": "repository/publications/test/snapshots/",
          "control_key": "repository/control/test.json"}


class MemoryStore:
    def __init__(self):
        self.values = {}
        self.owner = "test-fixed-session"
        self.server_time = datetime(2026, 10, 19, tzinfo=timezone.utc)
        self.mutex = threading.Lock()
        self.serial = 0
        self.lose_response = None
        self.barrier = None

    def head(self, key):
        with self.mutex:
            item = self.values.get(key)
            return dict(item[1]) if item else None

    def read(self, key):
        with self.mutex:
            data, head = self.values[key]
            result = data, dict(head)
        if self.barrier and key.startswith("repository/control/"):
            self.barrier.wait(timeout=5)
        return result

    def put(self, key, body, entry, *, previous=None, immutable=False):
        data = body if isinstance(body, bytes) else body.read()
        assert byte_entry(data) == entry
        with self.mutex:
            old = self.values.get(key)
            if (old and previous is None) or (previous and (
                not old or old[1]["ETag"] != previous["ETag"]
            )):
                raise ClientError({"Error": {"Code": "PreconditionFailed"},
                                   "ResponseMetadata": {"HTTPStatusCode": 412}}, "PutObject")
            self.serial += 1
            head = {"ETag": '"' + hashlib.md5(data, usedforsecurity=False).hexdigest() + '"',
                    "VersionId": str(self.serial), "ContentLength": len(data),
                    "ChecksumSHA256": base64.b64encode(hashlib.sha256(data).digest()).decode()}
            self.values[key] = data, head
        if key == self.lose_response:
            self.lose_response = None
            raise OSError("simulated lost response after successful write")
        return dict(head)


class PublisherTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.store = MemoryStore()
        self.one = self.snapshot("one", "one")
        self.two = self.snapshot("two", "two")
        self.publisher = Publisher(self.store, CONFIG)

    def snapshot(self, directory, generation, payload=b"same signed package"):
        directory = self.root / directory
        root = directory / "files"
        files = {
            "apt/pool/vector.deb": payload,
            "apt/dists/stable/InRelease": generation.encode(),
            "apt/dists/stable/vector-0/binary-amd64/Packages": generation.encode(),
            "yum/stable/vector-0/x86_64/repodata/repomd.xml": generation.encode(),
            "yum/stable/vector-0/x86_64/repodata/repomd.xml.asc": (generation + " signature").encode(),
        }
        for name, data in files.items():
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        base = self.one.id if hasattr(self, "one") else None
        snapshot = Snapshot({k: byte_entry(v) for k, v in files.items()}, base=base)
        (directory / "manifest.json").write_bytes(snapshot.data)
        return Snapshot.local(root)

    def live(self):
        return {key: value for key, value in self.store.values.items() if "/live/" in key}

    def test_retry_does_not_create_new_live_versions(self):
        self.publisher.publish(self.one)
        before = self.live()
        self.publisher.publish(self.one)
        self.assertEqual(self.live(), before)

    def test_incremental_snapshot_reuses_inherited_payload_without_a_local_copy(self):
        self.publisher.publish(self.one)
        (self.two.source / "apt/pool/vector.deb").unlink()
        snapshot = Snapshot.local(self.two.source)
        self.publisher.publish(snapshot)
        self.assertEqual(self.publisher.load_state()[0]["active"], snapshot.id)

    def test_stale_preparation_and_history_removal_fail_before_live_writes(self):
        self.publisher.publish(self.one)
        stale = self.snapshot("stale", "stale")
        self.publisher.publish(self.two)
        before = self.live()
        with self.assertRaises(Conflict):
            self.publisher.publish(stale)
        entries = {k: v for k, v in self.two.entries.items() if not k.endswith(".deb")}
        removal = Snapshot(entries, self.two.source, base=self.two.id)
        with self.assertRaises(Conflict):
            self.publisher.publish(removal)
        self.assertEqual(before, self.live())

    def test_missing_inherited_payload_fails_before_roots_change(self):
        self.publisher.publish(self.one)
        del self.store.values[self.publisher.live + "apt/pool/vector.deb"]
        (self.two.source / "apt/pool/vector.deb").unlink()
        before = self.live()
        with self.assertRaises(Conflict):
            self.publisher.publish(self.two)
        self.assertEqual(before, self.live())

    def test_conflict_preflight_leaves_metadata_unchanged(self):
        self.publisher.publish(self.one)
        before = self.live()
        conflict = self.snapshot("conflict", "two", b"different package at same URL")
        with self.assertRaises(Conflict):
            self.publisher.publish(conflict)
        self.assertEqual(before, self.live())
        self.assertFalse(self.publisher.load_state()[0]["held"])

    def test_racing_lock_acquisitions_have_exactly_one_winner(self):
        self.publisher.acquire()
        self.publisher.release()
        self.store.barrier = threading.Barrier(2)

        def acquire(_):
            try:
                Publisher(self.store, CONFIG).acquire()
                return "winner"
            except Busy:
                return "busy"

        with ThreadPoolExecutor(max_workers=2) as executor:
            result = list(executor.map(acquire, range(2)))
        self.assertCountEqual(result, ["winner", "busy"])

    def test_interrupted_publish_can_only_resume_or_roll_back(self):
        self.publisher.publish(self.one)

        def stop(phase):
            if phase.endswith("repomd.xml"):
                raise Interrupted("torn pair")

        with self.assertRaises(Interrupted):
            Publisher(self.store, CONFIG, stop).publish(self.two)
        state, _ = self.publisher.load_state()
        self.assertEqual(state["active"], self.one.id)
        self.assertEqual(state["pending"], self.two.id)
        with self.assertRaises(Conflict):
            self.publisher.publish(self.snapshot("three", "three"))
        self.publisher.publish(self.publisher.archived(self.one.id), rollback=True)
        self.assertIsNone(self.publisher.load_state()[0]["pending"])
        self.publisher.publish(self.two)

    def test_lost_write_response_retains_lock_and_blocks_early_recovery(self):
        self.publisher.publish(self.one)
        self.store.lose_response = self.publisher.live + "apt/dists/stable/InRelease"
        with self.assertRaises(OSError):
            self.publisher.publish(self.two)
        _, head = self.publisher.load_state()
        with self.assertRaises(Busy):
            Publisher(self.store, CONFIG).publish(self.two)
        with self.assertRaises(Busy):
            Publisher(self.store, CONFIG).recover_lock(head["VersionId"], True)
        self.store.server_time += timedelta(minutes=66)
        with self.assertRaises(PublicationError):
            self.publisher.recover_lock(head["VersionId"], False)
        self.publisher.recover_lock(head["VersionId"], True)
        Publisher(self.store, CONFIG).publish(self.two)
        with self.assertRaises(Conflict):
            self.publisher.recover_lock(head["VersionId"], True)

    def test_corrupt_archived_metadata_fails_before_any_live_change(self):
        self.publisher.publish(self.one)
        self.publisher.publish(self.two)
        before = self.live()
        key = self.publisher.archives + self.one.id + "/files/apt/dists/stable/InRelease"
        self.store.put(key, b"bad", byte_entry(b"bad"), previous=self.store.head(key))
        with self.assertRaises(Conflict):
            self.publisher.publish(self.publisher.archived(self.one.id), rollback=True)
        self.assertEqual(before, self.live())

    def test_manifest_rejects_extra_file_changed_bytes_symlink_and_traversal(self):
        extra = self.one.source / "apt/pool/extra.deb"
        extra.write_bytes(b"unexpected")
        with self.assertRaises(PublicationError):
            Snapshot.local(self.one.source)
        extra.unlink()
        package = self.one.source / "apt/pool/vector.deb"
        package.write_bytes(b"changed")
        with self.assertRaises(PublicationError):
            Snapshot.local(self.one.source)
        for name in ("../escape", "/apt/root", "apt/a/../root", "apt//root"):
            with self.assertRaises(PublicationError):
                Snapshot({name: byte_entry(b"test")})
        linked = self.two.source / "apt/pool/linked.deb"
        linked.symlink_to(self.two.source / "apt/pool/vector.deb")
        with self.assertRaises(PublicationError):
            Snapshot.local(self.two.source)

    def test_head_403_requires_authorized_listing_to_prove_absence(self):
        store = S3Store.__new__(S3Store)
        store.bucket = "test"
        store.client = Mock()
        store.response_time = Mock()
        denial = ClientError({"Error": {"Code": "AccessDenied"},
                              "ResponseMetadata": {"HTTPStatusCode": 403}}, "HeadObject")
        store.client.head_object.side_effect = denial
        store.client.list_objects_v2.return_value = {"Contents": []}
        self.assertIsNone(store.head("repository/control/test.json"))
        store.client.list_objects_v2.return_value = {"Contents": [{"Key": "repository/control/test.json"}]}
        with self.assertRaises(ClientError):
            store.head("repository/control/test.json")
        store.client.list_objects_v2.side_effect = denial
        with self.assertRaises(ClientError):
            store.head("repository/control/test.json")


if __name__ == "__main__":
    unittest.main()
