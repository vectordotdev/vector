"""Private installer fixtures, with a separate ephemeral legacy signing identity."""

import json
import os
from datetime import timedelta
from email.utils import format_datetime, parsedate_to_datetime
from pathlib import Path
import shutil
import tarfile

from inputs import require
from repository import Signer, key_fingerprints, prepare
from storage import Snapshot, canonical, sha256_file


def installer_fixture(config, work, gpg, password):
    gpg("--quick-generate-key", "Vector simulated legacy repository", "rsa4096", "cert", "2d", protected=True)
    primary = key_fingerprints(gpg("--with-colons", "--list-keys", "Vector simulated legacy repository"), "pub", "sub")[0][1]
    gpg("--quick-add-key", primary, "rsa4096", "sign", "2d", protected=True)
    subkey = key_fingerprints(gpg("--with-colons", "--list-keys", primary), "pub", "sub")[1][1]
    private = gpg("--armor", "--export-secret-subkeys", subkey + "!", protected=True).decode()
    public = work / "installer/public"
    public.mkdir(parents=True)
    (public / "legacy.asc").write_bytes(gpg("--armor", "--export", primary))
    (public / "legacy.gpg").write_bytes(gpg("--export", primary))
    shutil.copyfile(config["public_key_path"], public / "cose.asc")
    (public / "fingerprint").write_text(config["primary_fingerprint"] + "\n")
    legacy_config = {**config, "primary_fingerprint": primary, "signing_fingerprint": subkey,
                     "public_key_path": str(public / "legacy.asc")}
    legacy = work / "installer-legacy"
    legacy.mkdir()
    shutil.copyfile(work / "one/input.json", legacy / "input.json")
    shutil.copytree(work / "one/input", legacy / "input", copy_function=os.link)
    (legacy / "base").mkdir()
    (legacy / "base.json").write_bytes(b"null\n")
    os.environ.update(PACKAGE_SIGNING_SUBKEY=private, PACKAGE_SIGNING_PASSPHRASE=password)
    prepare(legacy_config, legacy)
    # Model the observed Datadog Release fields, without pretending that this
    # fixture has Datadog's signing key or its non-Vector package payloads.
    observed = json.loads((Path(__file__).parent / "legacy-apt-identity.json").read_bytes())
    signer = Signer(legacy_config, "/run/keys/legacy-metadata", private, password)
    try:
        for channel in ("stable", "beta"):
            suite = legacy / "result/files/apt/dists" / channel
            release = suite / "Release"
            lines = [line for line in release.read_text().splitlines()
                     if not line.startswith(("Origin:", "Label:", "Suite:", "Components:", "Acquire-By-Hash:", "Date:"))]
            first = (work / "one/result/files/apt/dists" / channel / "Release").read_text()
            date = next(line[6:] for line in first.splitlines() if line.startswith("Date: "))
            # Apt rejects Release date regressions while retaining its cached
            # indexes. This simulated legacy publication must precede cutover.
            lines.insert(0, "Date: " + format_datetime(parsedate_to_datetime(date) - timedelta(hours=1)))
            lines.insert(0, ("Suite: " + observed["Suite"]).rstrip())
            lines.insert(1, "Components: " + observed["Components"])
            release.write_text("\n".join(lines) + "\n")
            signer.sign(release, clear=True, destination=suite / "InRelease")
            signer.sign(release, destination=suite / "Release.gpg")
    finally:
        signer.close()
    private = None
    original = Snapshot.decode((legacy / "result/manifest.json").read_bytes())
    entries = {p.relative_to(legacy / "result/files").as_posix(): {"size": p.stat().st_size, "sha256": sha256_file(p)}
               for p in (legacy / "result/files").rglob("*") if p.is_file()}
    (legacy / "result/manifest.json").write_bytes(Snapshot(entries, input_id=original.input_id).data)

    views = work / "installer/views"
    for view, sources in (
        ("legacy", [legacy / "result/files"]),
        ("cose", [work / "one/result/files"]),
        ("upgraded", [work / "one/result/files", work / "two/result/files"]),
    ):
        for source in sources:
            for path in source.rglob("*"):
                if path.is_file():
                    target = views / view / path.relative_to(source)
                    target.parent.mkdir(parents=True, exist_ok=True)
                    # Copies permit client-side tamper tests without modifying
                    # another view through a shared inode.
                    shutil.copyfile(path, target)
    for channel in ("stable", "beta"):
        release = (views / "cose/apt/dists" / channel / "Release").read_text()
        require("Origin:" not in release and "Label:" not in release and "Suite:\n" in release,
                "The new apt repository would require release-information approval")
    output = work / "installer-bundles"
    output.mkdir()
    for deb_arch, rpm_arch in (("amd64", "x86_64"), ("arm64", "aarch64")):
        with tarfile.open(output / (deb_arch + ".tar"), "w") as archive:
            for path in sorted((work / "installer").rglob("*")):
                if not path.is_file():
                    continue
                if path.name.endswith(".deb") and not path.name.endswith("_" + deb_arch + ".deb"):
                    continue
                if path.name.endswith(".rpm") and not path.name.endswith("." + rpm_arch + ".rpm"):
                    continue
                archive.add(path, arcname=path.relative_to(work / "installer").as_posix())
    (work / "installer-report.json").write_bytes(canonical({
        "primary_fingerprint": config["primary_fingerprint"],
        "legacy_fingerprint": primary,
        "checks": ["independent ephemeral legacy signing key", "legacy apt identity fields captured from live metadata",
                   "unchanged apt Origin, Label, Suite and Codename", "legacy metadata predates COSE publications",
                   "three signed repository views for native HTTPS tests"],
    }))
    print("PASS: installer fixtures preserve the observed apt identity with independent legacy and COSE keys", flush=True)
