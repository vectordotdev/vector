"""Container implementation for the typed `vdev release repositories` commands."""

import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import sys
import tarfile
from urllib.request import urlopen

from inputs import fetch, require, verify_input
from repository import PublicKey, lint_package, prepare, repomd_locations, run, verify_metadata
from storage import (Busy, Conflict, Publisher, S3Store, Snapshot, byte_entry,
                     canonical, matches, mutable, safe_relative, sha256_file)


def configuration(path):
    config = json.loads(path.read_text())
    require(config["schema"] == 1, "Unsupported publisher configuration")
    require(config["repository"] == "vectordotdev/vector" and config["repository_id"] == 146327667, "Wrong source repository")
    require(re.fullmatch(r"[0-9]{12}", config["account"]), "Invalid AWS account")
    require(re.fullmatch(r"[a-z0-9][a-z0-9.-]{1,61}[a-z0-9]", config["bucket"]), "Invalid bucket")
    paths = [config[field] for field in ("live_prefix", "archive_prefix", "bundle_prefix", "control_key")]
    for name in paths:
        require(re.fullmatch(r"[a-zA-Z0-9_./-]+", name) and not name.startswith("/")
                and all(part not in ("", ".", "..") for part in name.rstrip("/").split("/")), "Invalid S3 path")
    require(all(config[field].endswith("/") for field in ("live_prefix", "archive_prefix", "bundle_prefix")), "Prefixes must end in / ")
    require(all(not a.startswith(b) and not b.startswith(a) for i, a in enumerate(paths) for b in paths[i+1:]),
            "Live, archive, bundle and control paths must be disjoint")
    if config["account"] == "486234852809":
        require(config["bucket"] == "dd-cose-releases" and config["live_prefix"] == "vector/repositories/",
                "Unexpected production repository destination")
        require(all(not value.startswith("vector/") for value in paths[1:]), "Internal state must be outside the public vector/ prefix")
    for name in ("primary_fingerprint", "signing_fingerprint"):
        require(re.fullmatch(r"[0-9A-F]{40}", config[name]), "Invalid pinned key fingerprint")
    require(Path(config["public_key"]).name == config["public_key"], "Public key must be beside configuration")
    config["public_key_path"] = str(path.parent / config["public_key"])
    config["role"] = os.environ.get("PACKAGE_REPOSITORIES_ROLE_ARN", "")
    return config


def store_for(config):
    require(re.fullmatch(r"arn:aws:iam::" + config["account"] + r":role/[A-Za-z0-9_+=,.@-]+", config["role"]),
            "Set the configured publishing role ARN explicitly")
    return S3Store(config)


def bundle_key(config, snapshot):
    require(snapshot.input_id is not None, "Snapshot has no input identity")
    return config["bundle_prefix"] + "bundles/" + snapshot.input_id + "/" + (snapshot.base or "initial") + ".tar.gz"


def rpm_cache_key(config, asset):
    return config["bundle_prefix"] + "rpms/" + asset["sha256"] + ".rpm"


def checked_download(store, key, path, entry=None):
    path.parent.mkdir(parents=True, exist_ok=True)
    head = store.head(key)
    require(head is not None and head["ContentLength"] < 5 * 1024**3, "Missing or oversized S3 input")
    actual = store.download(key, path)
    require(matches(head, actual) and (entry is None or actual == entry), "Downloaded S3 object failed checksum verification")
    return actual


def unpack_bundle(path, destination):
    destination.mkdir()
    total = 0
    names = set()
    with tarfile.open(path, "r:gz") as archive:
        for member in archive:
            require(member.isfile() and member.name not in names, "Bundle has a duplicate or nonregular member")
            names.add(member.name)
            require(member.name in {"input.json", "manifest.json"} or member.name.startswith("files/"), "Unexpected bundle member")
            if member.name.startswith("files/"):
                safe_relative(member.name[6:])
            total += member.size
            require(total < 5 * 1024**3, "Bundle exceeds the extraction budget")
            target = destination / member.name
            target.parent.mkdir(parents=True, exist_ok=True)
            with archive.extractfile(member) as source, target.open("xb") as output:
                shutil.copyfileobj(source, output)
    require({"input.json", "manifest.json"}.issubset(names), "Incomplete signed bundle")
    return Snapshot.local(destination / "files")


def pack_bundle(source, target):
    with target.open("xb") as raw, gzip.GzipFile(fileobj=raw, mode="wb", mtime=0, filename="") as compressed:
        with tarfile.open(fileobj=compressed, mode="w|") as archive:
            for path in sorted(source.rglob("*")):
                require(not path.is_symlink(), "Bundle contains a symlink")
                if path.is_file():
                    info = archive.gettarinfo(str(path), arcname=path.relative_to(source).as_posix())
                    info.uid = info.gid = info.mtime = 0
                    info.uname = info.gname = ""
                    info.mode = 0o644
                    with path.open("rb") as data:
                        archive.addfile(info, data)


def base(config, work):
    manifest, input_id = verify_input(work)
    store = store_for(config)
    publisher = Publisher(store, config)
    state, _ = publisher.load_state()
    require(not state["held"], "Publisher is locked; inspect status before retrying")
    active = publisher.archived(state["active"]) if state["active"] else None
    require(active is not None or not config.get("require_existing_repository", True),
            "Historical repository backfill is required before release publication")
    if state["pending"]:
        pending = publisher.archived(state["pending"])
        require(pending.input_id == input_id, "Resume or roll back the pending publication first")
        candidate = pending
    elif active and active.input_id == input_id:
        candidate = active
    else:
        candidate = Snapshot({"apt/dists/stable/InRelease": byte_entry(b"")},
                             base=active.id if active else None, input_id=input_id)
    key = bundle_key(config, candidate)
    if store.head(key):
        archive = work / "cached.tar.gz"
        checked_download(store, key, archive)
        snapshot = unpack_bundle(archive, work / "result")
        require(snapshot.input_id == input_id and snapshot.base == candidate.base, "Cached bundle belongs to another input or base")
        require((work / "result/input.json").read_bytes() == (work / "input.json").read_bytes(), "Cached source manifest differs")
        archive.unlink()
        print("Restored the saved signed publication; signing is skipped")
        return
    require(not state["pending"] and not (active and active.input_id == input_id), "Required retry bundle is missing")
    (work / "base.json").write_bytes(active.data if active else b"null\n")
    destination = work / "base"
    destination.mkdir()
    if active:
        for name, entry in active.entries.items():
            if mutable(name):
                key = publisher.archives + active.id + "/files/" + name
                checked_download(store, key, destination / name, entry)
        for root in destination.glob("yum/*/vector-0/*"):
            for relative in repomd_locations(root):
                name = (root / relative).relative_to(destination).as_posix()
                require(name in active.entries, "RPM metadata is missing from the active manifest")
                checked_download(store, publisher.live + name, destination / name, active.entries[name])
    reuse = work / "reuse"
    reuse.mkdir()
    for name, asset in manifest["assets"].items():
        if asset.get("format") == "rpm" and store.head(rpm_cache_key(config, asset)):
            checked_download(store, rpm_cache_key(config, asset), reuse / name)
    print("Downloaded current metadata and any previously signed RPMs")


def check_result(config, work):
    manifest, input_id = verify_input(work)
    snapshot = Snapshot.local(work / "result/files")
    require(snapshot.input_id == input_id and (work / "result/input.json").read_bytes() == (work / "input.json").read_bytes(),
            "Signed output belongs to a different immutable input")
    public = PublicKey(config, "/run/keys/check-result")
    try:
        verify_metadata(snapshot, snapshot.source, public)
        rpmdb = public.home / "rpmdb"
        run(["rpm", "--dbpath", rpmdb, "--import", public.key])
        for name, asset in manifest["assets"].items():
            if asset.get("format") == "deb":
                require(snapshot.entries[f"apt/pool/v/ve/{name}"] == {"size": asset["size"], "sha256": asset["sha256"]},
                        "Debian package changed after input verification")
            elif asset.get("format") == "rpm":
                filename = f"yum/{manifest['channel']}/vector-0/{asset['arch']}/{name}"
                lint_package(snapshot.source / filename, asset)
                require("digests signatures OK" in run(["rpm", "--dbpath", rpmdb, "--checksig", snapshot.source / filename]).decode(),
                        "RPM signature verification failed")
    finally:
        public.close()
    return manifest, snapshot


def publish(config, work):
    manifest, snapshot = check_result(config, work)
    store = store_for(config)
    publisher = Publisher(store, config)
    require(publisher.load_state()[0]["active"] is not None or not config.get("require_existing_repository", True),
            "Historical repository backfill is required before release publication")
    # Freeze signed bytes before any live write. Conditional single PUTs are atomic;
    # an ambiguous response is resolved by reading the same immutable key on retry.
    for name, asset in manifest["assets"].items():
        if asset.get("format") == "rpm":
            relative = f"yum/{manifest['channel']}/vector-0/{asset['arch']}/{name}"
            with (snapshot.source / relative).open("rb") as data:
                publisher.ensure_immutable(rpm_cache_key(config, asset), data, snapshot.entries[relative])
    archive = work / "prepared.tar.gz"
    if archive.exists():
        archive.unlink()
    pack_bundle(work / "result", archive)
    require(archive.stat().st_size < 5 * 1024**3, "Prepared bundle exceeds the single-PUT limit")
    with archive.open("rb") as data:
        publisher.ensure_immutable(bundle_key(config, snapshot), data,
                                   {"size": archive.stat().st_size, "sha256": sha256_file(archive)})
    print("Published snapshot " + publisher.publish(snapshot))


def verify(config, work):
    manifest, snapshot = check_result(config, work)
    names = {name for name in snapshot.entries if mutable(name)}
    names.update(path.relative_to(snapshot.source).as_posix() for path in snapshot.source.rglob("*") if path.is_file())
    for name in sorted(names):
        repository, relative = name.split("/", 1)
        url = config[repository + "_url"]
        require(url.startswith("https://") and url.endswith("/"), "Verification requires HTTPS repository roots")
        expected = snapshot.entries[name]
        digest = hashlib.sha256()
        size = 0
        with urlopen(url + relative, timeout=90) as response:
            require(response.geturl().startswith("https://"), "Repository redirected away from HTTPS")
            while data := response.read(1024 * 1024):
                size += len(data)
                require(size <= expected["size"], "Endpoint returned an oversized object")
                digest.update(data)
        require({"sha256": digest.hexdigest(), "size": size} == expected, "Endpoint differs from published snapshot: " + name)
    print(f"Verified {len(names)} endpoint objects for {manifest['tag']}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path, required=True)
    commands = parser.add_subparsers(dest="command", required=True)
    download = commands.add_parser("fetch")
    download.add_argument("--run", type=int, required=True)
    download.add_argument("--tag", required=True)
    download.add_argument("--channel", choices=("stable", "beta"), required=True)
    for name in ("base", "prepare", "publish", "verify", "status"):
        commands.add_parser(name)
    test_options = commands.add_parser("test").add_mutually_exclusive_group()
    test_options.add_argument("--packages", action="store_true")
    test_options.add_argument("--s3", action="store_true")
    commands.add_parser("rollback").add_argument("snapshot")
    recovery = commands.add_parser("recover-lock")
    recovery.add_argument("--expected-version", required=True)
    recovery.add_argument("--confirm-owner-stopped", action="store_true")
    args = parser.parse_args()
    config = configuration(args.config)
    work = Path("/work")
    if args.command == "fetch":
        fetch(config, work, args.run, args.tag, args.channel)
        print("Verified six repository packages and their published checksums")
    elif args.command == "base":
        base(config, work)
    elif args.command == "prepare":
        print("Prepared snapshot " + prepare(config, work))
    elif args.command == "publish":
        publish(config, work)
    elif args.command == "verify":
        verify(config, work)
    elif args.command == "test":
        import unittest
        suite = unittest.defaultTestLoader.discover("tests")
        require(unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful(), "Publisher regressions failed")
        if args.packages:
            from tests.package_fixture import prepare_fixture
            prepare_fixture(config, work)
        elif args.s3:
            from tests.publication_fixture import publish_fixture
            publish_fixture(config, work)
    else:
        publisher = Publisher(store_for(config), config)
        if args.command == "status":
            state, head = publisher.load_state()
            print(json.dumps({"state": state, "version": head["VersionId"] if head else None}, indent=2))
        elif args.command == "rollback":
            print(publisher.publish(publisher.archived(args.snapshot), rollback=True))
        else:
            publisher.recover_lock(args.expected_version, args.confirm_owner_stopped)
            print("Lock released; resume the pending snapshot or roll back to the last completed snapshot")


if __name__ == "__main__":
    main()
