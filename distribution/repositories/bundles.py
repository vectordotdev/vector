"""Checksum-verified transport of signed publication workspaces."""

import gzip
import shutil
import tarfile

from inputs import require
from storage import Snapshot, matches, safe_relative


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
