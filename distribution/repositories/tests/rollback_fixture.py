"""Pre-sign recovery cases without exposing a private key to S3 publication."""

import os
from pathlib import Path
import shutil
from unittest.mock import patch

from inputs import require
from repository import repomd_locations
import rollback
from storage import PublicationError, Snapshot, mutable


def seed(work, target, active, pending=None):
    work.mkdir()
    data = rollback.plan_bytes(target.id, active.id, pending.id if pending else None)
    (work / "input.json").write_bytes(data)
    for snapshot in {item.id: item for item in (target, active, pending) if item}.values():
        directory = work / "sources" / snapshot.id
        root = directory / "files"
        root.mkdir(parents=True)
        (directory / "manifest.json").write_bytes(snapshot.data)
        names = {name for name in snapshot.entries if mutable(name)}
        if snapshot.id in {target.id, active.id}:
            for package_root in snapshot.source.glob("yum/*/vector-0/*"):
                names.update((package_root / name).relative_to(snapshot.source).as_posix()
                             for name in repomd_locations(package_root))
        for name in names:
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(snapshot.source / name, path)


def prepare_fixture(config, work, key, password):
    first = Snapshot.local(work / "one/result/files")
    second = Snapshot.local(work / "two/result/files")
    back = work / "rollback"
    seed(back, first, second)
    os.environ.update(PACKAGE_SIGNING_SUBKEY=key, PACKAGE_SIGNING_PASSPHRASE=password)
    rollback.prepare(config, back)
    restored = Snapshot.local(back / "result/files")
    forward = work / "recovered"
    seed(forward, second, restored)
    os.environ.update(PACKAGE_SIGNING_SUBKEY=key, PACKAGE_SIGNING_PASSPHRASE=password)
    rollback.prepare(config, forward)
    interrupted = work / "rollback-pending"
    seed(interrupted, first, first, second)
    os.environ.update(PACKAGE_SIGNING_SUBKEY=key, PACKAGE_SIGNING_PASSPHRASE=password)
    rollback.prepare(config, interrupted)
    for name, entry in second.entries.items():
        if not mutable(name):
            require(restored.entries[name] == entry, "Rollback removed immutable history")
    require(not any(path.name.endswith((".deb", ".rpm")) for path in back.rglob("*")),
            "Rollback copied package payloads")

    # A corrupt downloaded source must fail before importing the private key.
    broken = work / "rollback-corrupt"
    seed(broken, first, second)
    path = broken / "sources" / first.id / "files/apt/dists/stable/Release"
    path.write_bytes(path.read_bytes() + b"Corruption\n")
    with patch.object(rollback, "Signer") as signer:
        try:
            rollback.prepare(config, broken)
        except PublicationError:
            pass
        else:
            raise AssertionError("Corrupt rollback source was accepted")
        signer.assert_not_called()
    shutil.rmtree(broken)

    # A valid signed bundle cannot be relabeled as another selection.
    original = (back / "input.json").read_bytes()
    (back / "input.json").write_bytes(rollback.plan_bytes(second.id, second.id, None))
    try:
        rollback.check_result(config, back)
    except PublicationError:
        pass
    else:
        raise AssertionError("Rollback accepted a signed bundle for another selection")
    (back / "input.json").write_bytes(original)
    print("PASS: fresh signed rollback preserves package bytes, validates sources before key import, and binds the selected archive", flush=True)
