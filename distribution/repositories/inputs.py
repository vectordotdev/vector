"""Unprivileged admission and download of already-built GitHub release assets."""

import hashlib
import json
import os
from pathlib import Path
import re
from urllib.parse import quote
from urllib.request import HTTPRedirectHandler, Request, build_opener, urlopen

from storage import PublicationError, canonical, sha256_file

DEB_ARCHES = ("amd64", "arm64", "armhf")
RPM_ARCHES = ("x86_64", "aarch64", "armv7hl")
TAG = re.compile(r"v0\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)(?:-([0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*))?")


def require(condition, message):
    if not condition:
        raise PublicationError(message)


def versions(tag, channel):
    match = TAG.fullmatch(tag)
    require(match is not None, "Expected a Vector 0.x semantic release tag without build metadata")
    require(channel in {"stable", "beta"}, "Invalid repository channel")
    require(channel != "stable" or match[1] is None, "Stable cannot publish a prerelease")
    raw = tag[1:]
    native = raw.replace("-", "~", 1)
    return raw, native + "-1", native.replace("-", ".") + "-1"


def expected_packages(tag, channel):
    _, deb, rpm = versions(tag, channel)
    return {**{f"vector_{deb}_{arch}.deb": {"format": "deb", "arch": arch, "version": deb}
               for arch in DEB_ARCHES},
            **{f"vector-{rpm}.{arch}.rpm": {"format": "rpm", "arch": arch, "version": rpm}
               for arch in RPM_ARCHES}}


def validate_run(config, run, tag, commit):
    require(run.get("repository", {}).get("id") == config["repository_id"], "Wrong source repository")
    require(run.get("head_repository", {}).get("id") == config["repository_id"], "Fork runs cannot publish")
    require(run.get("path") == config["workflow"], "Wrong source workflow")
    require(run.get("event") == "push" and run.get("head_branch") == tag, "Source must be a tag push")
    require(run.get("status") == "completed" and run.get("conclusion") == "success", "Source run did not succeed")
    require(run.get("head_sha") == commit and re.fullmatch(r"[0-9a-f]{40}", commit), "Run and tag commit differ")


def select_assets(release, tag, channel):
    raw, deb, _ = versions(tag, channel)
    require(release.get("tag_name") == tag and not release.get("draft") and release.get("published_at"),
            "Release must be published and match the tag")
    require(channel != "stable" or not release.get("prerelease"), "Stable release is marked prerelease")
    expected = expected_packages(tag, channel)
    checksums = f"vector-{raw}-SHA256SUMS"
    wanted = set(expected) | {checksums}
    allowed_packages = set(expected) | {f"vector_{deb}_armel.deb"}
    selected = {}
    seen = set()
    for asset in release.get("assets", []):
        name = asset["name"]
        require(name not in seen, "Duplicate release asset name")
        seen.add(name)
        require(not name.endswith((".deb", ".rpm")) or name in allowed_packages, "Unexpected Linux package: " + name)
        if name not in wanted:
            continue
        digest = asset.get("digest", "")
        require(isinstance(digest, str) and re.fullmatch(r"sha256:[0-9a-f]{64}", digest), "Asset lacks a GitHub SHA-256 digest")
        require(type(asset.get("size")) is int and 0 < asset["size"] < 1024**3, "Invalid asset size")
        require(type(asset.get("id")) is int and asset["id"] > 0, "Invalid asset ID")
        selected[name] = {"id": asset["id"], "size": asset["size"], "sha256": digest[7:], **expected.get(name, {})}
    require(set(selected) == wanted, "Release is missing required packages or SHA256SUMS")
    return selected


def parse_checksums(data):
    result = {}
    for line in data.decode("utf-8").splitlines():
        match = re.fullmatch(r"([0-9a-f]{64}) [ *]([^/\\\s]+)", line)
        require(match is not None, "Malformed SHA256SUMS entry")
        require(match[2] not in result, "Duplicate SHA256SUMS entry")
        result[match[2]] = match[1]
    return result


def github(endpoint):
    class NoRedirect(HTTPRedirectHandler):
        def redirect_request(self, request, response, code, message, headers, url):
            raise PublicationError("GitHub API redirects are not allowed")

    request = Request("https://api.github.com/" + endpoint,
                      headers={"Accept": "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28",
                               **({"Authorization": "Bearer " + os.environ["GH_TOKEN"]} if os.environ.get("GH_TOKEN") else {})})
    with build_opener(NoRedirect).open(request, timeout=90) as response:
        require(response.geturl().startswith("https://api.github.com/"), "Unexpected API redirect")
        return json.load(response)


def fetch(config, work, run_id, tag, channel):
    expected_packages(tag, channel)
    repo = config["repository"]
    require(repo == "vectordotdev/vector" and config["repository_id"] == 146327667,
            "Release inputs must come from vectordotdev/vector")
    run = github(f"repos/{repo}/actions/runs/{run_id}")
    ref = github(f"repos/{repo}/git/ref/tags/{quote(tag, safe='')}")["object"]
    for _ in range(4):
        if ref["type"] == "commit":
            break
        require(ref["type"] == "tag", "Release ref does not identify a commit")
        ref = github(f"repos/{repo}/git/tags/{ref['sha']}")["object"]
    require(ref["type"] == "commit", "Too many annotated tag indirections")
    validate_run(config, run, tag, ref["sha"])
    endpoint = f"repos/{repo}/releases/tags/{quote(tag, safe='')}"
    release = github(endpoint)
    assets = select_assets(release, tag, channel)
    manifest = {"schema": 1, "repository": repo, "repository_id": config["repository_id"],
                "run": run_id, "commit": ref["sha"], "tag": tag, "channel": channel,
                "release": release["id"], "assets": assets}
    destination = work / "input"
    destination.mkdir(exist_ok=True)
    require({p.name for p in destination.iterdir()} <= set(assets), "Unexpected files in the input directory")
    if (work / "input.json").exists():
        require((work / "input.json").read_bytes() == canonical(manifest), "Previously frozen inputs have changed")
    for name, asset in assets.items():
        if (destination / name).exists():
            require((destination / name).is_file() and not (destination / name).is_symlink()
                    and (destination / name).stat().st_size == asset["size"]
                    and sha256_file(destination / name) == asset["sha256"], "Previously downloaded asset differs")
            continue
        # Public downloads deliberately carry no GitHub credential through redirects.
        url = f"https://github.com/{repo}/releases/download/{quote(tag, safe='')}/{quote(name, safe='')}"
        temporary = work / "download.partial"
        require(not temporary.is_symlink(), "Invalid temporary download file")
        with urlopen(url, timeout=90) as source, temporary.open("wb") as target:
            size = 0
            while data := source.read(1024 * 1024):
                size += len(data)
                require(size <= asset["size"], "Asset exceeded its declared size")
                target.write(data)
        require(size == asset["size"] and sha256_file(temporary) == asset["sha256"], "Asset digest/size mismatch")
        temporary.replace(destination / name)
    after = github(endpoint)
    require(after["id"] == release["id"] and select_assets(after, tag, channel) == assets,
            "Release assets changed during download")
    (work / "input.json").write_bytes(canonical(manifest))
    verify_input(work)
    return manifest


def verify_input(work):
    data = (work / "input.json").read_bytes()
    manifest = json.loads(data)
    require(canonical(manifest) == data and manifest["schema"] == 1, "Invalid input manifest")
    require(manifest["repository"] == "vectordotdev/vector" and manifest["repository_id"] == 146327667, "Wrong input repository")
    expected = expected_packages(manifest["tag"], manifest["channel"])
    checksum_name = f"vector-{manifest['tag'][1:]}-SHA256SUMS"
    require(set(manifest["assets"]) == set(expected) | {checksum_name}, "Input package set changed")
    require({p.name for p in (work / "input").iterdir()} == set(manifest["assets"]), "Unexpected input files")
    for name, asset in manifest["assets"].items():
        path = work / "input" / name
        require(path.is_file() and not path.is_symlink(), "Input must be a regular file")
        require(path.stat().st_size == asset["size"] and sha256_file(path) == asset["sha256"], "Input asset changed: " + name)
        for field, value in expected.get(name, {}).items():
            require(asset.get(field) == value, "Input native identity changed")
    sums = parse_checksums((work / "input" / checksum_name).read_bytes())
    for name in expected:
        require(sums.get(name) == manifest["assets"][name]["sha256"], "SHA256SUMS disagrees with GitHub digest")
    return manifest, hashlib.sha256(data).hexdigest()
