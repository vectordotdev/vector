"""Restore an archived package selection as a new, freshly signed generation."""

from datetime import datetime, timedelta, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import shutil

from bundles import checked_download, pack_bundle, unpack_bundle
from inputs import require
from repository import (PublicKey, Signer, dated_release, next_release_date,
                        release_date, repomd_locations, verify_metadata)
from storage import Publisher, Snapshot, byte_entry, canonical, mutable, sha256_file


def plan_bytes(target, active, pending):
    data = canonical({"schema": 1, "operation": "rollback", "target": target,
                      "active": active, "pending": pending})
    decode_plan(data)
    return data


def decode_plan(data):
    plan = json.loads(data)
    require(set(plan) == {"schema", "operation", "target", "active", "pending"}
            and plan["schema"] == 1 and plan["operation"] == "rollback"
            and canonical(plan) == data, "Invalid rollback request")
    for name in ("target", "active", "pending"):
        value = plan[name]
        require((name == "pending" and value is None) or
                (isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value)), "Invalid rollback snapshot identity")
    require(plan["pending"] is None or plan["target"] == plan["active"],
            "Abandoning a pending publication must restore the last completed package selection")
    return plan, hashlib.sha256(data).hexdigest()


def request_key(config, input_id):
    return config["bundle_prefix"] + "rollback-requests/" + input_id + ".json"


def bundle_key(config, input_id):
    return config["bundle_prefix"] + "rollbacks/" + input_id + ".tar.gz"


def source_ids(plan):
    return sorted({plan[key] for key in ("target", "active", "pending") if plan[key]})


def download_sources(publisher, work, plan):
    for identity in source_ids(plan):
        snapshot = publisher.archived(identity)
        directory = work / "sources" / identity
        root = directory / "files"
        root.mkdir(parents=True)
        (directory / "manifest.json").write_bytes(snapshot.data)
        for name, entry in snapshot.entries.items():
            if mutable(name):
                checked_download(publisher.store, publisher.archives + identity + "/files/" + name, root / name, entry)
        # A pending generation may have stopped before uploading immutable RPM
        # indexes. Its signed root dates still constrain the replacement.
        if identity in {plan["active"], plan["target"]}:
            for package_root in root.glob("yum/*/vector-0/*"):
                for relative in repomd_locations(package_root):
                    name = (package_root / relative).relative_to(root).as_posix()
                    require(name in snapshot.entries, "RPM metadata is missing from the archived manifest")
                    checked_download(publisher.store, publisher.live + name, root / name, snapshot.entries[name])


def base(config, work, target, store):
    publisher = Publisher(store, config)
    state, _ = publisher.load_state()
    require(not state["held"] and state["active"], "Rollback requires an unlocked, initialized repository")
    data = None
    retry = False
    candidate = publisher.archived(state["pending"] or state["active"])
    if candidate.input_id and store.head(request_key(config, candidate.input_id)):
        saved, _ = store.read(request_key(config, candidate.input_id))
        plan, identity = decode_plan(saved)
        require(identity == candidate.input_id, "Saved rollback request failed checksum verification")
        if plan["target"] == target:
            data = saved
            retry = True
    data = data or plan_bytes(target, state["active"], state["pending"])
    plan, input_id = decode_plan(data)
    (work / "input.json").write_bytes(data)
    download_sources(publisher, work, plan)
    key = bundle_key(config, input_id)
    if store.head(key):
        archive = work / "cached.tar.gz"
        checked_download(store, key, archive)
        snapshot = unpack_bundle(archive, work / "result")
        require(snapshot.input_id == input_id and snapshot.base == plan["active"]
                and (work / "result/input.json").read_bytes() == data, "Saved rollback belongs to another request")
        if retry:
            require(snapshot.id == candidate.id, "Saved rollback differs from the pending/completed generation")
        archive.unlink()
        print("Restored the saved signed rollback; signing is skipped")
    else:
        require(candidate.input_id != input_id, "Required rollback retry bundle is missing")
        print("Downloaded archived selections and all potentially exposed root dates")


def check_sources(config, work):
    plan, input_id = decode_plan((work / "input.json").read_bytes())
    sources = {}
    public = PublicKey(config, "/run/keys/rollback-sources")
    try:
        for identity in source_ids(plan):
            directory = work / "sources" / identity
            snapshot = Snapshot.decode((directory / "manifest.json").read_bytes(), identity)
            root = directory / "files"
            require(root.is_dir() and not root.is_symlink(), "Invalid rollback source directory")
            present = set()
            for path in root.rglob("*"):
                require(not path.is_symlink(), "Rollback source contains a symlink")
                if path.is_dir():
                    continue
                name = path.relative_to(root).as_posix()
                require(path.is_file() and snapshot.entries.get(name) ==
                        {"sha256": sha256_file(path), "size": path.stat().st_size}, "Rollback source differs from its manifest")
                require(not name.endswith((".deb", ".rpm")), "Rollback does not require local package payloads")
                present.add(name)
            require({name for name in snapshot.entries if mutable(name)} <= present, "Rollback source is missing mutable metadata")
            snapshot.source = root
            sources[identity] = snapshot
            if identity in {plan["active"], plan["target"]}:
                verify_metadata(snapshot, root, public)
            else:
                for name in present:
                    if name.endswith("/Release"):
                        suite = (root / name).parent
                        public.verify(suite / "InRelease")
                        public.verify(suite / "Release.gpg", suite / "Release")
                        require(public.gpg("--decrypt", str(suite / "InRelease")) == (root / name).read_bytes(),
                                "Pending apt roots disagree")
                    elif name.endswith("/repomd.xml"):
                        public.verify(Path(str(root / name) + ".asc"), root / name)
        selected = sources[plan["target"]]
        mutable_names = {name for name in selected.entries if mutable(name)}
        for snapshot in sources.values():
            require({name for name in snapshot.entries if mutable(name)} == mutable_names,
                    "Rollback across repository layout changes needs a separate migration")
        # Completed generations preserve the immutable history. Never resurrect
        # bytes absent from that history or allow two identities at the same URL.
        active = sources[plan["active"]]
        require(all(active.entries.get(name) == entry for name, entry in selected.entries.items() if not mutable(name)),
                "Selected snapshot is not contained in the active immutable history")
    finally:
        public.close()
    return plan, input_id, sources


def expected_entries(plan, sources):
    entries = dict(sources[plan["target"]].entries)
    entries.update({name: entry for name, entry in sources[plan["active"]].entries.items() if not mutable(name)})
    return entries


def prepare(config, work):
    private_key = os.environ.pop("PACKAGE_SIGNING_SUBKEY", "")
    passphrase = os.environ.pop("PACKAGE_SIGNING_PASSPHRASE", "")
    plan, input_id, sources = check_sources(config, work)
    selected = sources[plan["target"]]
    root = work / "result/files"
    shutil.copytree(selected.source, root)
    entries = expected_entries(plan, sources)
    signer = Signer(config, "/run/keys/rollback-signing", private_key, passphrase)
    try:
        for name in selected.entries:
            if name.endswith("/Release"):
                release = root / name
                date = next_release_date([(snapshot.source / name).read_bytes() for snapshot in sources.values()])
                release.write_bytes(dated_release(release.read_bytes(), date))
                signer.sign(release, clear=True, destination=release.parent / "InRelease")
                signer.sign(release, destination=release.parent / "Release.gpg")
        for path in root.rglob("*"):
            if path.is_file():
                entries[path.relative_to(root).as_posix()] = byte_entry(path.read_bytes())
        snapshot = Snapshot(entries, root, base=plan["active"], input_id=input_id)
        (work / "result/manifest.json").write_bytes(snapshot.data)
        (work / "result/input.json").write_bytes((work / "input.json").read_bytes())
    finally:
        signer.close()
    check_result(config, work)
    return snapshot.id


def check_result(config, work):
    plan, input_id, sources = check_sources(config, work)
    snapshot = Snapshot.local(work / "result/files")
    require(snapshot.input_id == input_id and snapshot.base == plan["active"]
            and (work / "result/input.json").read_bytes() == (work / "input.json").read_bytes(),
            "Signed rollback belongs to a different request")
    selected = sources[plan["target"]]
    expected = expected_entries(plan, sources)
    require(snapshot.entries.keys() == expected.keys(), "Rollback changed the repository file set")
    refreshed = {name for name in expected if name.endswith(("/Release", "/InRelease", "/Release.gpg"))}
    require(all(snapshot.entries[name] == entry for name, entry in expected.items() if name not in refreshed),
            "Rollback changed package bytes or the selected indexes")
    for name in expected:
        if name.endswith("/Release"):
            data = (snapshot.source / name).read_bytes()
            date = release_date(data)
            require(all(date > release_date((source.source / name).read_bytes()) for source in sources.values()),
                    "Rollback date is not newer than every potentially exposed root")
            require(date <= datetime.now(timezone.utc) + timedelta(seconds=5), "Rollback root is dated too far into the future")
            require(dated_release(data, "DATE") == dated_release((selected.source / name).read_bytes(), "DATE"),
                    "Rollback changed apt identity or index checksums")
    public = PublicKey(config, "/run/keys/rollback-result")
    try:
        verify_metadata(snapshot, snapshot.source, public)
    finally:
        public.close()
    return plan, snapshot


def publish(config, work, store):
    plan, snapshot = check_result(config, work)
    publisher = Publisher(store, config)
    for identity in source_ids(plan):
        require(publisher.archived(identity).data == (work / "sources" / identity / "manifest.json").read_bytes(),
                "Rollback source is not the saved archive")
    archive = work / "prepared.tar.gz"
    archive.unlink(missing_ok=True)
    pack_bundle(work / "result", archive)
    require(archive.stat().st_size < 5 * 1024**3, "Rollback bundle exceeds the single-PUT limit")
    with archive.open("rb") as stream:
        publisher.ensure_immutable(bundle_key(config, snapshot.input_id), stream,
                                   {"sha256": sha256_file(archive), "size": archive.stat().st_size})
    data = (work / "input.json").read_bytes()
    publisher.ensure_immutable(request_key(config, snapshot.input_id), data, byte_entry(data))
    identity = publisher.publish(snapshot, recovery=(plan["active"], plan["pending"]))
    print("Published fresh rollback snapshot " + identity)
    return identity
