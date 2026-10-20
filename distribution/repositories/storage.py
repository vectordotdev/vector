"""Conditional S3 publication protocol, invoked through vdev release repositories."""

import base64
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timedelta, timezone
from email.utils import parsedate_to_datetime
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import sys
import uuid

import boto3
from botocore.config import Config
from botocore.exceptions import ClientError


class PublicationError(RuntimeError):
    pass


class Busy(PublicationError):
    pass


class Conflict(PublicationError):
    pass


class Interrupted(PublicationError):
    """A deliberate test interruption after all preceding S3 requests finished."""


MUTABLE_NAMES = {"Packages", "Packages.gz", "Release", "Release.gpg", "InRelease",
                 "repomd.xml", "repomd.xml.asc"}
ROOT_ORDER = {"Release": 0, "Release.gpg": 1, "InRelease": 2,
              "repomd.xml": 3, "repomd.xml.asc": 4}


def canonical(value):
    return (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode()


def sha256_file(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def safe_relative(name):
    if not isinstance(name, str) or not re.fullmatch(r"[A-Za-z0-9_./+~\-]+", name):
        raise PublicationError("Invalid manifest path")
    path = PurePosixPath(name)
    if path.is_absolute() or str(path) != name or ".." in path.parts:
        raise PublicationError("Noncanonical manifest path")
    if len(path.parts) < 3 or path.parts[0] not in {"apt", "yum"}:
        raise PublicationError("Manifest path is outside the repository roots")
    return path


def mutable(name):
    return safe_relative(name).name in MUTABLE_NAMES


class Snapshot:
    def __init__(self, entries, source=None, *, base=None, input_id=None):
        if not isinstance(entries, dict) or not entries:
            raise PublicationError("Empty or invalid snapshot manifest")
        for name, entry in entries.items():
            safe_relative(name)
            if not isinstance(entry, dict) or set(entry) != {"sha256", "size"}:
                raise PublicationError("Invalid manifest entry")
            if not re.fullmatch(r"[0-9a-f]{64}", entry["sha256"]):
                raise PublicationError("Invalid manifest checksum")
            if type(entry["size"]) is not int or not 0 <= entry["size"] <= 5 * 1024**3:
                raise PublicationError("Only single-PUT objects up to 5 GiB are supported")
        if not any(mutable(name) for name in entries):
            raise PublicationError("Snapshot has no repository metadata")
        self.entries = entries
        self.source = source
        for value in (base, input_id):
            if value is not None and not re.fullmatch(r"[0-9a-f]{64}", value):
                raise PublicationError("Invalid snapshot identity")
        self.base = base
        self.input_id = input_id
        self.data = canonical({"schema": 2, "base": base, "input": input_id, "files": entries})
        self.id = hashlib.sha256(self.data).hexdigest()

    @classmethod
    def decode(cls, data, expected_id=None):
        value = json.loads(data)
        if set(value) != {"schema", "base", "input", "files"} or value["schema"] != 2:
            raise PublicationError("Unsupported snapshot schema")
        snapshot = cls(value["files"], base=value["base"], input_id=value["input"])
        if data != snapshot.data or (expected_id is not None and snapshot.id != expected_id):
            raise PublicationError("Snapshot manifest is not canonical or has the wrong digest")
        return snapshot

    @classmethod
    def local(cls, source):
        source = Path(source)
        if source.is_symlink() or not source.is_dir():
            raise PublicationError("Snapshot must be a regular directory")
        snapshot = cls.decode((source.parent / "manifest.json").read_bytes())
        entries = {}
        for path in sorted(source.rglob("*")):
            if path.is_symlink():
                raise PublicationError("Snapshot symlinks are not allowed")
            if path.is_dir():
                continue
            if not path.is_file():
                raise PublicationError("Snapshot contains a nonregular file")
            name = path.relative_to(source).as_posix()
            safe_relative(name)
            entries[name] = {"sha256": sha256_file(path), "size": path.stat().st_size}
        for name, entry in entries.items():
            if snapshot.entries.get(name) != entry:
                raise PublicationError("Snapshot content differs from recorded manifest: " + name)
        for name in snapshot.entries:
            if name not in entries and (mutable(name) or snapshot.base is None):
                raise PublicationError("Snapshot is missing a required local file: " + name)
        snapshot.source = source
        return snapshot


class S3Store:
    def __init__(self, config):
        # Capture one fixed STS session. Never refresh credentials while owning a lock.
        required = ("AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", "AWS_SESSION_TOKEN")
        if not all(os.environ.get(name) for name in required):
            raise PublicationError("A fixed temporary AWS session is required")
        session = boto3.Session(
            aws_access_key_id=os.environ[required[0]],
            aws_secret_access_key=os.environ[required[1]],
            aws_session_token=os.environ[required[2]],
            region_name=config["region"],
        )
        # Disable automatic retries: an ambiguous mutable PUT must retain its lock.
        sdk = Config(retries={"total_max_attempts": 1}, max_pool_connections=16,
                     connect_timeout=10, read_timeout=90,
                     s3={"us_east_1_regional_endpoint": "regional"})
        identity = session.client("sts", config=sdk).get_caller_identity()
        role = config["role"].rsplit("/", 1)[1]
        expected = f'arn:aws:sts::{config["account"]}:assumed-role/{role}/'
        if identity["Account"] != config["account"] or not identity["Arn"].startswith(expected):
            raise PublicationError("Wrong AWS account or role")
        actual_role = session.client("iam", config=sdk).get_role(RoleName=role)["Role"]
        if actual_role["MaxSessionDuration"] > 3600:
            raise PublicationError("Role exceeds the one-hour recovery credential bound")
        self.client = session.client("s3", config=sdk.merge(Config(signature_version="s3v4")))
        self.bucket = config["bucket"]
        self.live_prefix = config["live_prefix"]
        self.owner = identity["Arn"]
        self.server_time = parsedate_to_datetime(identity["ResponseMetadata"]["HTTPHeaders"]["date"])

    def response_time(self, response):
        self.server_time = parsedate_to_datetime(response["ResponseMetadata"]["HTTPHeaders"]["date"])

    def head(self, key):
        try:
            result = self.client.head_object(Bucket=self.bucket, Key=key, ChecksumMode="ENABLED")
        except ClientError as exc:
            self.response_time(exc.response)
            status = exc.response["ResponseMetadata"]["HTTPStatusCode"]
            if status == 404:
                return None
            if status == 403:
                # Prefix-scoped ListBucket may hide missing keys behind HEAD 403.
                # An authorized exact-prefix listing distinguishes absence from denial.
                listed = self.client.list_objects_v2(Bucket=self.bucket, Prefix=key, MaxKeys=1)
                self.response_time(listed)
                if not any(item["Key"] == key for item in listed.get("Contents", [])):
                    return None
            raise
        self.response_time(result)
        return result

    def read(self, key):
        result = self.client.get_object(Bucket=self.bucket, Key=key, ChecksumMode="ENABLED")
        self.response_time(result)
        with result["Body"] as stream:
            data = stream.read()
        return data, result

    def put(self, key, body, entry, *, previous=None, immutable=False):
        options = {"IfNoneMatch": "*"} if previous is None else {"IfMatch": previous["ETag"]}
        result = self.client.put_object(
            Bucket=self.bucket, Key=key, Body=body, ContentLength=entry["size"],
            ChecksumSHA256=base64.b64encode(bytes.fromhex(entry["sha256"])).decode(),
            ServerSideEncryption="AES256", ContentType=content_type(key),
            CacheControl=(("public" if key.startswith(self.live_prefix) else "private")
                          + (",max-age=31536000,immutable" if immutable
                             else ",max-age=0,no-cache,must-revalidate")), **options,
        )
        self.response_time(result)
        return result

    def download(self, key, destination):
        result = self.client.get_object(Bucket=self.bucket, Key=key, ChecksumMode="ENABLED")
        self.response_time(result)
        digest = hashlib.sha256()
        with result["Body"] as source, destination.open("wb") as target:
            for block in iter(lambda: source.read(1024 * 1024), b""):
                digest.update(block)
                target.write(block)
        return {"sha256": digest.hexdigest(), "size": destination.stat().st_size}


def content_type(key):
    if key.endswith(".deb"):
        return "application/vnd.debian.binary-package"
    if key.endswith(".rpm"):
        return "application/x-rpm"
    if key.endswith(".json"):
        return "application/json"
    if key.endswith(".gz"):
        return "application/gzip"
    if key.endswith(".xml"):
        return "application/xml"
    return "application/octet-stream"


def matches(head, entry):
    return bool(head and head.get("ChecksumSHA256") == base64.b64encode(
        bytes.fromhex(entry["sha256"])).decode() and head["ContentLength"] == entry["size"])


def byte_entry(data):
    return {"sha256": hashlib.sha256(data).hexdigest(), "size": len(data)}


class Publisher:
    def __init__(self, store, config, hook=None):
        self.store = store
        self.live = config["live_prefix"]
        self.archives = config["archive_prefix"]
        self.lock_key = config["control_key"]
        self.hook = hook or (lambda phase: None)
        self.lock_head = None
        self.state = None

    def load_state(self):
        head = self.store.head(self.lock_key)
        if head is None:
            return {"schema": 1, "held": False, "active": None, "pending": None}, None
        data, current = self.store.read(self.lock_key)
        state = json.loads(data)
        if state.get("schema") != 1 or type(state.get("held")) is not bool:
            raise PublicationError("Invalid publication control object")
        return state, current

    def write_state(self):
        self.state["nonce"] = uuid.uuid4().hex  # Prevent an ABA match on lock state.
        data = canonical(self.state)
        self.lock_head = self.store.put(self.lock_key, data, byte_entry(data), previous=self.lock_head)

    def acquire(self):
        self.state, self.lock_head = self.load_state()
        if self.state["held"]:
            raise Busy("Publisher busy; locks are never stolen on timeout")
        self.state.update(held=True, owner=self.store.owner,
                          recover_after=(self.store.server_time + timedelta(minutes=65)).isoformat())
        try:
            self.write_state()
        except ClientError as exc:
            if exc.response["ResponseMetadata"]["HTTPStatusCode"] in (409, 412):
                raise Busy("Another publisher acquired the lock") from None
            raise
        self.hook("locked")

    def release(self):
        self.state["held"] = False
        self.write_state()

    def recover_lock(self, expected_version, owner_stopped):
        self.state, self.lock_head = self.load_state()
        if not owner_stopped:
            raise PublicationError("Confirm that the previous workflow/process has stopped")
        if not self.state["held"] or self.lock_head["VersionId"] != expected_version:
            raise Conflict("Lock version changed or is already released")
        if self.store.server_time < datetime.fromisoformat(self.state["recover_after"]):
            raise Busy("Recovery waits 65 minutes so the previous fixed STS session expires")
        # Conditional replacement cannot unlock a different owner's newer lock.
        self.release()

    def ensure_immutable(self, key, body, entry):
        old = self.store.head(key)
        if old is not None:
            if not matches(old, entry):
                raise Conflict("Immutable object conflict: " + key)
            return
        self.store.put(key, body, entry, immutable=True)

    def archive(self, snapshot):
        prefix = self.archives + snapshot.id + "/"
        for name, entry in sorted(snapshot.entries.items()):
            if mutable(name):
                with (snapshot.source / name).open("rb") as stream:
                    self.ensure_immutable(prefix + "files/" + name, stream, entry)
        self.ensure_immutable(prefix + "manifest.json", snapshot.data, byte_entry(snapshot.data))

    def archived(self, snapshot_id):
        if not re.fullmatch(r"[0-9a-f]{64}", snapshot_id):
            raise PublicationError("Invalid snapshot ID")
        data, _ = self.store.read(self.archives + snapshot_id + "/manifest.json")
        return Snapshot.decode(data, snapshot_id)

    def preflight(self, snapshot):
        active = self.state["active"]
        if active != snapshot.id:
            if active != snapshot.base:
                raise Conflict("Repository advanced after preparation; regenerate against the current base")
            if active:
                previous = self.archived(active)
                for name, entry in previous.entries.items():
                    if not mutable(name) and snapshot.entries.get(name) != entry:
                        raise Conflict("Publication would remove or replace historical data: " + name)
        names = sorted(snapshot.entries)
        with ThreadPoolExecutor(max_workers=16) as workers:
            heads = list(workers.map(self.store.head, (self.live + name for name in names)))
        for name, old in zip(names, heads):
            entry = snapshot.entries[name]
            if not mutable(name):
                if old is not None and not matches(old, entry):
                    raise Conflict("Immutable object conflict: " + name)
                if old is None and (snapshot.source is None or not (snapshot.source / name).is_file()):
                    raise Conflict("Inherited payload/index no longer exists: " + name)

    def publish(self, snapshot, *, recovery=None):
        self.acquire()
        try:
            active, pending = self.state["active"], self.state["pending"]
            if recovery is not None:
                # Recovery may abandon a pending generation only from the exact
                # state whose signed metadata was checked during preparation.
                expected_active, expected_pending = recovery
                allowed = {(expected_active, expected_pending),
                           (expected_active, snapshot.id), (snapshot.id, None)}
                if snapshot.base != expected_active or (active, pending) not in allowed:
                    raise Conflict("Repository changed after rollback preparation; inspect and prepare again")
            elif pending and snapshot.id != pending:
                raise Conflict("Recover the pending snapshot or roll back to the last completed one")
            self.preflight(snapshot)
            # Nothing above changes client-visible metadata.
            self.archive(snapshot)
            metadata = {}
            for name, entry in snapshot.entries.items():
                if mutable(name):
                    data, _ = self.store.read(self.archives + snapshot.id + "/files/" + name)
                    if byte_entry(data) != entry:
                        raise Conflict("Archived metadata failed digest verification: " + name)
                    metadata[name] = data
            self.state["pending"] = snapshot.id
            self.write_state()
            self.hook("pending")
            for name, entry in sorted(snapshot.entries.items()):
                if not mutable(name):
                    # Preflight verified inherited immutable objects under the lock.
                    if not (snapshot.source / name).exists():
                        continue
                    if matches(self.store.head(self.live + name), entry):
                        continue
                    with (snapshot.source / name).open("rb") as stream:
                        self.ensure_immutable(self.live + name, stream, entry)
            self.hook("payloads")
            indexes = sorted(name for name in snapshot.entries
                             if PurePosixPath(name).name in {"Packages", "Packages.gz"})
            roots = sorted((name for name in snapshot.entries if mutable(name) and name not in indexes),
                           key=lambda name: (ROOT_ORDER[PurePosixPath(name).name], name))
            for name in indexes + roots:
                entry = snapshot.entries[name]
                key = self.live + name
                previous = self.store.head(key)
                if matches(previous, entry):
                    continue
                self.store.put(key, metadata[name], entry, previous=previous)
                self.hook("written:" + name)
            names = sorted(snapshot.entries)
            with ThreadPoolExecutor(max_workers=16) as workers:
                heads = list(workers.map(self.store.head, (self.live + name for name in names)))
            for name, head in zip(names, heads):
                if not matches(head, snapshot.entries[name]):
                    raise PublicationError("Published object failed final verification: " + name)
            self.state.update(active=snapshot.id, pending=None)
            self.write_state()
        except (Conflict, Interrupted):
            # These are deliberate, synchronous stops with no outstanding AWS request.
            self.release()
            raise
        except BaseException:
            # A lost response can hide a completed PUT. Never unlock automatically.
            print("Publication stopped; the lock remains held for operator recovery", file=sys.stderr)
            raise
        else:
            self.release()
        return snapshot.id
