"""Opt-in private S3 integration test; exports verified views for native clients."""

import base64
import os
from pathlib import Path
import shutil
import tarfile
from unittest.mock import patch

import engine
from inputs import require
from storage import Conflict, Interrupted, Publisher, Snapshot, canonical, mutable


def publish_fixture(config, work):
    require(config["account"] == "092620891642" and config["bucket"] == "pront-ci-sandbox-092620891642-us-east-1",
            "S3 fixture is restricted to the private COSE sandbox")
    require(config["live_prefix"].startswith("repository/publications/vdev-")
            and config["control_key"].startswith("repository/control/vdev-"), "Invalid integration namespace")
    store = engine.store_for(config)
    publisher = Publisher(store, config)
    state, _ = publisher.load_state()
    require(state["active"] is None and not state["held"], "Fixture namespace is not empty; choose a new run ID")
    one, two = work / "one", work / "two"
    first = Snapshot.local(one / "result/files")
    second = Snapshot.local(two / "result/files")
    shared = work / "verified-objects"
    shared.mkdir()
    views = work / "views"
    reports = []

    def capture(name):
        view = views / name
        view.mkdir(parents=True)
        # Objects not uploaded yet are absent from the initial view. Every object
        # included in a client bundle is fetched from S3 and checked independently.
        for relative in second.entries:
            key = publisher.live + relative
            head = store.head(key)
            if head is None:
                continue
            digest = base64.b64decode(head["ChecksumSHA256"]).hex()
            content = shared / digest
            if not content.exists():
                engine.checked_download(store, key, content,
                                        {"sha256": digest, "size": head["ContentLength"]})
            target = view / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            os.link(content, target)

    def live_versions(snapshot):
        return {name: store.head(publisher.live + name)["VersionId"] for name in snapshot.entries}

    engine.publish(config, one)
    capture("initial")
    reports.append("first six-package release published and downloaded from S3")
    before = live_versions(first)
    retry = work / "retry"
    retry.mkdir()
    shutil.copyfile(one / "input.json", retry / "input.json")
    shutil.copytree(one / "input", retry / "input", copy_function=os.link)
    engine.base(config, retry)
    require(Snapshot.local(retry / "result/files").id == first.id, "Retry changed the signed snapshot")
    engine.publish(config, retry)
    require(live_versions(first) == before, "Retry replaced a live object")
    reports.append("S3 bundle restored without a private key; retry created no live object versions")

    def interrupt_at(phase, view):
        def factory(store, configuration):
            def hook(actual):
                if actual == phase:
                    capture(view)
                    raise Interrupted("intentional fixture interruption")
            return Publisher(store, configuration, hook)
        with patch.object(engine, "Publisher", factory):
            try:
                engine.publish(config, two)
            except Interrupted:
                return
        raise AssertionError("Expected publication interruption was not reached")

    interrupt_at("payloads", "payloads")
    interrupt_at("written:yum/stable/vector-0/x86_64/repodata/repomd.xml", "torn")
    engine.publish(config, two)
    capture("current")
    reports.append("incremental publication resumed after payload and RPM root/signature interruptions")
    publisher.publish(publisher.archived(first.id), rollback=True)
    capture("rollback")
    engine.publish(config, two)
    capture("recovered")
    reports.append("archived metadata rollback and re-publication preserved immutable history")
    before = live_versions(second)
    try:
        publisher.publish(first)
    except Conflict:
        pass
    else:
        raise AssertionError("An old preparation replaced newer repository metadata")
    require(live_versions(second) == before, "Stale input modified live data")
    reports.append("stale prepared generation rejected before any live object changed")
    public = work / "public"
    public.mkdir()
    shutil.copyfile(Path(config["public_key_path"]), public / "cose.asc")
    shutil.copyfile(Path(config["public_key_path"]).with_suffix(".gpg"), public / "cose.gpg")
    output = work / "client-bundles"
    output.mkdir()
    for deb_arch, rpm_arch in (("amd64", "x86_64"), ("arm64", "aarch64")):
        with tarfile.open(output / (deb_arch + ".tar"), "w") as archive:
            for path in sorted(public.iterdir()):
                archive.add(path, arcname="public/" + path.name)
            for path in sorted(views.rglob("*")):
                if not path.is_file():
                    continue
                if path.name.endswith(".deb") and not path.name.endswith("_" + deb_arch + ".deb"):
                    continue
                if path.name.endswith(".rpm") and not path.name.endswith("." + rpm_arch + ".rpm"):
                    continue
                archive.add(path, arcname="views/" + path.relative_to(views).as_posix())
    report = {"first": first.id, "second": second.id, "checks": reports,
              "state": publisher.load_state()[0]}
    (work / "publication-report.json").write_bytes(canonical(report))
    for item in reports:
        print("PASS: " + item, flush=True)
