import copy
import io
import json
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from engine import unpack_bundle
from inputs import expected_packages, parse_checksums, select_assets, validate_run, versions
from repository import apt_indexes, merge_packages
from storage import PublicationError


class InputTests(unittest.TestCase):
    def test_stable_and_prerelease_native_versions(self):
        self.assertEqual(versions("v0.61.0", "stable"), ("0.61.0", "0.61.0-1", "0.61.0-1"))
        self.assertEqual(versions("v0.61.0-rc.1-test", "beta"),
                         ("0.61.0-rc.1-test", "0.61.0~rc.1-test-1", "0.61.0~rc.1.test-1"))
        for tag in ("v0.61.0-rc.1", "v0.61.0+build", "v0.61.0;sh", "main", "v1.0.0", "v0.061.0"):
            with self.subTest(tag=tag), self.assertRaises(PublicationError):
                versions(tag, "stable")

    def test_release_admission_rejects_fork_wrong_workflow_ref_commit_and_failure(self):
        config = {"repository_id": 146327667, "workflow": ".github/workflows/release.yml"}
        run = {"repository": {"id": 146327667}, "head_repository": {"id": 146327667},
               "path": config["workflow"], "event": "push", "head_branch": "v0.60.0",
               "head_sha": "a" * 40, "status": "completed", "conclusion": "success"}
        validate_run(config, run, "v0.60.0", "a" * 40)
        for key, value in (("head_repository", {"id": 7}), ("repository", {"id": 7}),
                           ("path", ".github/workflows/nightly.yml"), ("event", "pull_request"),
                           ("head_branch", "master"), ("head_sha", "b" * 40), ("conclusion", "failure")):
            with self.subTest(field=key), self.assertRaises(PublicationError):
                validate_run(config, {**run, key: value}, "v0.60.0", "a" * 40)

    def test_asset_selection_rejects_missing_duplicate_unexpected_and_unhashed(self):
        names = list(expected_packages("v0.60.0", "stable")) + ["vector-0.60.0-SHA256SUMS"]
        release = {"tag_name": "v0.60.0", "draft": False, "prerelease": False, "published_at": "today",
                   "assets": [{"id": i + 1, "name": name, "size": 10, "digest": "sha256:" + "a" * 64}
                              for i, name in enumerate(names)]}
        self.assertEqual(len(select_assets(release, "v0.60.0", "stable")), 7)
        cases = []
        missing = copy.deepcopy(release); missing["assets"].pop(); cases.append(missing)
        duplicate = copy.deepcopy(release); duplicate["assets"].append(duplicate["assets"][0]); cases.append(duplicate)
        unknown = copy.deepcopy(release); unknown["assets"][0]["name"] = "vector-unexpected.rpm"; cases.append(unknown)
        unhashed = copy.deepcopy(release); unhashed["assets"][0]["digest"] = None; cases.append(unhashed)
        for case in cases:
            with self.assertRaises(PublicationError):
                select_assets(case, "v0.60.0", "stable")

    def test_checksums_reject_duplicate_and_traversal(self):
        line = "a" * 64 + "  vector.deb\n"
        self.assertEqual(parse_checksums(line.encode()), {"vector.deb": "a" * 64})
        for data in (line + line, "a" * 64 + "  ../vector.deb\n", "invalid"):
            with self.assertRaises(PublicationError):
                parse_checksums(data.encode())

    def test_incremental_apt_merge_keeps_historical_native_versions(self):
        old = b"Package: vector\nVersion: 0.10.0\nArchitecture: amd64\nFilename: pool/v/ve/vector_0.10.0-1_amd64.deb\n\n"
        new = b"Package: vector\nVersion: 0.60.0-1\nArchitecture: amd64\nFilename: pool/v/ve/vector_0.60.0-1_amd64.deb\n\n"
        self.assertEqual(merge_packages(old, new), old + new)
        self.assertEqual(merge_packages(old, old), old)
        with self.assertRaises(PublicationError):
            merge_packages(old, old.replace(b"pool/v/ve", b"pool/replaced"))

    def test_apt_identity_matches_observed_legacy_repository(self):
        # Identity fields are cached by apt and changes require explicit user
        # approval. Compare with observed input rather than the desired output.
        observed = json.loads((Path(__file__).parent / "legacy-apt-identity.json").read_bytes())
        class UnsignedFixture:
            def sign(self, source, **kwargs):
                pass
            def verify(self, *args):
                pass
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            apt_indexes(root, "stable", {}, root / "previous", UnsignedFixture())
            release = {key: value.strip() for line in
                       (root / "apt/dists/stable/Release").read_text().splitlines()
                       if ":" in line and not line.startswith(" ")
                       for key, value in [line.split(":", 1)]}
            for field in ("Origin", "Label", "Suite", "Codename"):
                self.assertEqual(release.get(field), observed[field], field)

    def test_signed_bundle_rejects_links_traversal_and_duplicates(self):
        for names, linked in ((["files/apt/../escape"], False), (["files/apt/pool/x"], True),
                              (["input.json", "input.json"], False)):
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                archive = root / "bundle.tar.gz"
                with tarfile.open(archive, "w:gz") as target:
                    for name in names:
                        member = tarfile.TarInfo(name)
                        member.size = 1
                        if linked:
                            member.type = tarfile.SYMTYPE
                            member.linkname = "/etc/passwd"
                        target.addfile(member, io.BytesIO(b"x"))
                with self.assertRaises(PublicationError):
                    unpack_bundle(archive, root / "output")


if __name__ == "__main__":
    unittest.main()
