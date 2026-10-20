"""Opt-in private S3 integration test; exports verified views for native clients."""

import base64
import os
from pathlib import Path
import shutil
import tarfile
from unittest.mock import patch

import engine
import rollback
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
    # This historical view reproduces apt's rejection of an older Release date.
    shutil.copytree(views / "initial", views / "stale-rollback", copy_function=os.link)

    def exercise_rollback(configuration, fixture, target, label, interrupt=False):
        request = work / label
        request.mkdir()
        rollback.base(configuration, request, target, store)
        require((request / "input.json").read_bytes() == (fixture / "input.json").read_bytes(),
                "S3 preparation differs from the independently signed recovery request")
        require(not (request / "result").exists(), "Unexpected rollback cache hit")
        shutil.copytree(fixture / "result", request / "result", copy_function=os.link)
        signed = Snapshot.local(request / "result/files")
        if interrupt:
            def factory(store, configuration):
                def hook(phase):
                    if phase == "written:apt/dists/stable/InRelease":
                        raise Interrupted("intentional rollback interruption")
                return Publisher(store, configuration, hook)
            with patch.object(rollback, "Publisher", factory):
                try:
                    rollback.publish(configuration, request, store)
                except Interrupted:
                    pass
                else:
                    raise AssertionError("Expected rollback interruption was not reached")
            resumed = work / (label + "-resume")
            resumed.mkdir()
            rollback.base(configuration, resumed, target, store)
            require(Snapshot.local(resumed / "result/files").id == signed.id, "Resume changed the signed rollback")
            rollback.publish(configuration, resumed, store)
        else:
            rollback.publish(configuration, request, store)
        live = Publisher(store, configuration).live
        before = {name: store.head(live + name)["VersionId"] for name in signed.entries}
        retry = work / (label + "-retry")
        retry.mkdir()
        rollback.base(configuration, retry, target, store)
        require(Snapshot.local(retry / "result/files").id == signed.id, "Completed retry changed the signed rollback")
        rollback.publish(configuration, retry, store)
        require(before == {name: store.head(live + name)["VersionId"] for name in signed.entries},
                "Rollback retry replaced a live object")
        return signed

    restored = exercise_rollback(config, work / "rollback", first.id, "restore", interrupt=True)
    capture("rollback")
    recovered = exercise_rollback(config, work / "recovered", second.id, "forward")
    capture("recovered")
    reports.append("fresh signed rollback and forward recovery preserve immutable history and advance apt dates")
    reports.append("interrupted and completed rollback retries restore exact signed bytes without a key or extra live versions")

    pending_config = dict(config)
    for field in ("live_prefix", "archive_prefix", "bundle_prefix"):
        pending_config[field] += "pending-test/"
    pending_config["control_key"] = config["control_key"].removesuffix(".json") + "-pending.json"
    engine.publish(pending_config, one)
    def pending_factory(store, configuration):
        def hook(phase):
            if phase == "pending":
                raise Interrupted("stopped before any new payload or index upload")
        return Publisher(store, configuration, hook)
    with patch.object(engine, "Publisher", pending_factory):
        try:
            engine.publish(pending_config, two)
        except Interrupted:
            pass
        else:
            raise AssertionError("Expected early publication interruption was not reached")
    exercise_rollback(pending_config, work / "rollback-pending", first.id, "abandon-pending")
    reports.append("pending publication abandoned safely even when its immutable RPM indexes were never uploaded")
    before = live_versions(recovered)
    try:
        publisher.publish(first)
    except Conflict:
        pass
    else:
        raise AssertionError("An old preparation replaced newer repository metadata")
    require(live_versions(recovered) == before, "Stale input modified live data")
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
    report = {"first": first.id, "second": second.id, "rollback": restored.id, "recovered": recovered.id, "checks": reports,
              "state": publisher.load_state()[0]}
    (work / "publication-report.json").write_bytes(canonical(report))
    for item in reports:
        print("PASS: " + item, flush=True)
