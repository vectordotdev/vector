"""Native package validation and incremental apt/yum metadata construction."""

import gzip
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import time
from datetime import datetime, timezone
import xml.etree.ElementTree as ET

from inputs import DEB_ARCHES, RPM_ARCHES, require, verify_input
from storage import Snapshot, byte_entry, mutable, safe_relative, sha256_file


def run(args, *, cwd=None, data=None, env=None):
    result = subprocess.run([str(arg) for arg in args], cwd=cwd, input=data,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env, check=False)
    require(result.returncode == 0, f"{args[0]} failed: " + result.stderr.decode(errors="replace")[-4000:])
    return result.stdout


def fields(paragraph):
    result = {}
    for line in paragraph.splitlines():
        if line.startswith((" ", "\t")):
            continue
        key, separator, value = line.partition(":")
        require(separator and key not in result, "Malformed or duplicate package metadata field")
        result[key] = value.strip()
    return result


def merge_packages(previous, incoming):
    packages = {}
    for content in (previous, incoming):
        for paragraph in content.decode().strip().split("\n\n"):
            if not paragraph:
                continue
            item = fields(paragraph)
            require(item.get("Package") == "vector", "Non-Vector Debian package")
            identity = (item["Package"], item["Version"], item["Architecture"])
            old = packages.get(identity)
            require(old is None or fields(old) == item, "Conflicting Debian package identity")
            packages[identity] = paragraph
    return ("\n\n".join(packages[key] for key in sorted(packages)) + ("\n\n" if packages else "")).encode()


def lint_package(path, identity):
    if identity["format"] == "deb":
        for field, expected in (("Package", "vector"), ("Version", identity["version"]), ("Architecture", identity["arch"])):
            require(run(["dpkg-deb", "-f", path, field]).decode().strip() == expected, "Debian native identity mismatch")
        process = subprocess.Popen(["dpkg-deb", "--fsys-tarfile", str(path)], stdout=subprocess.PIPE,
                                   stderr=subprocess.DEVNULL)
        try:
            size = 0
            with tarfile.open(fileobj=process.stdout, mode="r|") as archive:
                for member in archive:
                    size += member.size
                    require(size < 8 * 1024**3, "Unreasonably large package payload")
                    require(member.issym() or member.islnk() or not member.mode & 0o002,
                            "World-writable Debian package entry: " + member.name)
            require(process.wait() == 0, "Cannot inspect Debian payload")
        finally:
            process.stdout.close()
            if process.poll() is None:
                process.kill()
                process.wait()
    else:
        actual = run(["rpm", "-qp", "--qf", "%{NAME} %{VERSION}-%{RELEASE} %{ARCH}", path]).decode()
        require(actual == f"vector {identity['version']} {identity['arch']}", "RPM native identity mismatch")
        listing = run(["rpm", "-qp", "--qf", "[%{FILEMODES:perms} %{FILENAMES}\\n]", path]).decode()
        for line in listing.splitlines():
            require(len(line) >= 10, "Invalid RPM file listing")
            require(line[0] == "l" or line[8] != "w", "World-writable RPM package entry: " + line[11:])


def key_fingerprints(listing, primary_type, subkey_type):
    result = []
    pending = None
    for line in listing.decode().splitlines():
        parts = line.split(":")
        if parts[0] in (primary_type, subkey_type):
            pending = parts
        elif parts[0] == "fpr" and pending is not None:
            result.append((pending, parts[9]))
            pending = None
    return result


class PublicKey:
    def __init__(self, config, directory):
        self.config = config
        self.home = Path(directory)
        self.home.mkdir(mode=0o700, parents=True)
        self.env = {**os.environ, "GNUPGHOME": str(self.home)}
        self.key = Path(config["public_key_path"])
        listing = self.gpg("--with-colons", "--show-keys", str(self.key))
        keys = key_fingerprints(listing, "pub", "sub")
        require([fpr for parts, fpr in keys if parts[0] == "pub"] == [config["primary_fingerprint"]], "Wrong public primary key")
        require(config["signing_fingerprint"] in [fpr for parts, fpr in keys if parts[0] == "sub"], "Wrong public signing subkey")
        self.gpg("--import", str(self.key))

    def gpg(self, *args, data=None):
        return run(["gpg", "--batch", "--no-tty", *args], data=data, env=self.env)

    def verify(self, signature, data=None):
        args = ["--status-fd", "1", "--verify", str(signature)]
        if data is not None:
            args.append(str(data))
        status = self.gpg(*args).decode()
        require(any(line.startswith("[GNUPG:] VALIDSIG " + self.config["signing_fingerprint"] + " ")
                    for line in status.splitlines()), "Signature did not use the pinned signing subkey")

    def close(self):
        run(["gpgconf", "--kill", "all"], env=self.env)
        shutil.rmtree(self.home)


class Signer(PublicKey):
    def __init__(self, config, directory, private_key, passphrase):
        super().__init__(config, directory)
        require(private_key and passphrase and "\n" not in passphrase and "\r" not in passphrase,
                "An encrypted signing subkey and a single-line passphrase are required")
        self.password = self.home / "passphrase"
        self.password.write_text(passphrase + "\n")
        self.password.chmod(0o600)
        self.gpg("--import", data=private_key.encode())
        keys = key_fingerprints(self.gpg("--with-colons", "--list-secret-keys"), "sec", "ssb")
        require(len(keys) == 2, "Expected exactly one primary stub and one signing subkey")
        primary, subkey = keys
        require(primary[1] == config["primary_fingerprint"] and primary[0][0] == "sec"
                and len(primary[0]) > 14 and primary[0][14] == "#", "Usable primary secret key is forbidden in CI")
        require(subkey[1] == config["signing_fingerprint"] and subkey[0][0] == "ssb"
                and len(subkey[0]) > 14 and subkey[0][14] == "+" and "s" in subkey[0][11],
                "Wrong or unavailable signing subkey")
        for parts, _ in keys:
            require(parts[1] not in ("r", "e", "d") and parts[6] and int(parts[6]) > time.time(), "Signing identity is expired or revoked")
        challenge = self.home / "challenge"
        challenge.write_text("Vector repository signing capability check\n")
        run(["gpgconf", "--kill", "gpg-agent"], env=self.env)
        wrong = subprocess.run(["gpg", "--batch", "--no-tty", "--pinentry-mode", "loopback",
                                "--passphrase-fd", "0", "--local-user", config["signing_fingerprint"] + "!",
                                "--output", str(self.home / "wrong-passphrase.asc"), "--detach-sign", str(challenge)],
                               input=os.urandom(32).hex().encode() + b"\n", env=self.env,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
        require(wrong.returncode != 0, "Signing subkey must be passphrase-protected")
        self.sign(challenge)
        self.verify(challenge.with_suffix(".asc"), challenge)
        self.rpmdb = self.home / "rpmdb"
        run(["rpm", "--dbpath", self.rpmdb, "--import", self.key])

    def sign(self, source, *, clear=False, destination=None):
        destination = destination or Path(str(source) + ".asc")
        self.gpg("--yes", "--pinentry-mode", "loopback", "--passphrase-file", str(self.password),
                 "--local-user", self.config["signing_fingerprint"] + "!", "--digest-algo", "SHA256",
                 "--armor", "--output", str(destination), "--clearsign" if clear else "--detach-sign", str(source))

    def rpm(self, package):
        run(["rpmsign", "--define", "_gpg_name " + self.config["signing_fingerprint"] + "!",
             "--define", "_gpg_path " + str(self.home), "--define", "__gpg /usr/bin/gpg",
             "--define", "_gpg_digest_algo sha256", "--define",
             "_gpg_sign_cmd_extra_args --pinentry-mode loopback --passphrase-file " + str(self.password),
             "--addsign", package], env=self.env)
        output = run(["rpm", "--dbpath", self.rpmdb, "--checksig", package]).decode()
        require("digests signatures OK" in output, "Signed RPM failed native verification")


def repomd_locations(root):
    document = ET.fromstring((root / "repodata/repomd.xml").read_bytes())
    result = []
    for entry in document.findall("{*}data"):
        location = entry.find("{*}location")
        require(location is not None and set(location.attrib) == {"href"}, "Unexpected RPM metadata base URL")
        name = location.attrib["href"]
        safe_relative("yum/metadata/" + name)
        require(name.startswith("repodata/") and len(Path(name).parts) == 2, "Invalid RPM metadata location")
        result.append(name)
    require(result, "Empty RPM root metadata")
    return result


def check_apt_index(data, snapshot):
    for paragraph in data.decode().strip().split("\n\n"):
        if not paragraph:
            continue
        item = fields(paragraph)
        require(item.get("Package") == "vector", "Foreign package in apt history")
        name = "apt/" + item["Filename"]
        safe_relative(name)
        require(snapshot.entries.get(name) == {"sha256": item["SHA256"], "size": int(item["Size"])},
                "Debian history does not match the snapshot manifest")


def verify_metadata(snapshot, root, public):
    for name in snapshot.entries:
        if name.endswith("/InRelease"):
            suite = (root / name).parent
            public.verify(suite / "InRelease")
            public.verify(suite / "Release.gpg", suite / "Release")
            release = (suite / "Release").read_bytes()
            require(public.gpg("--decrypt", str(suite / "InRelease")) == release, "Inline and detached apt roots disagree")
            algorithm = None
            checked = set()
            for line in release.decode().splitlines():
                if not line.startswith(" "):
                    algorithm = "sha256" if line == "SHA256:" else None
                elif algorithm:
                    digest, size, relative = line.split()
                    safe_relative("apt/index/" + relative)
                    entry = {"sha256": digest, "size": int(size)}
                    filename = (suite / relative).relative_to(root).as_posix()
                    require(snapshot.entries.get(filename) == entry, "Apt index differs from its signed root")
                    if relative.endswith("/Packages"):
                        require(byte_entry((suite / relative).read_bytes()) == entry, "Apt index checksum mismatch")
                        check_apt_index((suite / relative).read_bytes(), snapshot)
                        checked.add(filename)
            require(checked, "Apt root has no SHA-256 package indexes")
        elif name.endswith("/repomd.xml"):
            package_root = (root / name).parent.parent
            public.verify(Path(str(root / name) + ".asc"), root / name)
            document = ET.fromstring((root / name).read_bytes())
            for item in document.findall("{*}data"):
                location = item.find("{*}location")
                require(location is not None and set(location.attrib) == {"href"}, "RPM metadata must use relative locations")
                relative = location.attrib["href"]
                safe_relative("yum/metadata/" + relative)
                path = package_root / relative
                entry = snapshot.entries.get(path.relative_to(root).as_posix())
                checksum = item.find("{*}checksum")
                require(checksum is not None and checksum.attrib.get("type") in {"sha256", "sha512"}, "RPM metadata needs SHA-2 checksums")
                data = path.read_bytes()
                require(entry == byte_entry(data) and hashlib.new(checksum.attrib["type"], data).hexdigest() == checksum.text,
                        "RPM metadata differs from its signed root")
                if item.attrib.get("type") == "primary":
                    require(path.suffix == ".gz", "Expected gzip RPM primary metadata")
                    primary = ET.fromstring(gzip.decompress(data))
                    for package in primary.findall("{*}package"):
                        require(package.findtext("{*}name") == "vector", "Foreign package in RPM history")
                        url = package.find("{*}location")
                        checksum = package.find("{*}checksum")
                        size = package.find("{*}size")
                        require(url is not None and set(url.attrib) == {"href"}, "RPM package must use a relative location")
                        safe_relative("yum/package/" + url.attrib["href"])
                        filename = (package_root / url.attrib["href"]).relative_to(root).as_posix()
                        require(checksum is not None and checksum.attrib.get("type") == "sha256" and size is not None,
                                "RPM package needs a SHA-256 checksum and size")
                        require(snapshot.entries.get(filename) == {"sha256": checksum.text, "size": int(size.attrib["package"])},
                                "RPM package differs from its signed index")


def apt_indexes(root, channel, incoming, previous, signer):
    suite = root / "apt/dists" / channel
    architectures = (*DEB_ARCHES, "x86_64", "all", "i386")
    for arch in architectures:
        relative = f"apt/dists/{channel}/vector-0/binary-{arch}/Packages"
        old = (previous / relative).read_bytes() if (previous / relative).exists() else b""
        added = incoming.get("amd64" if arch == "x86_64" else arch, b"")
        data = merge_packages(old, added)
        index = root / relative
        index.parent.mkdir(parents=True, exist_ok=True)
        index.write_bytes(data)
        index.with_suffix(".gz").write_bytes(gzip.compress(data, mtime=0))
        for path in (index, index.with_suffix(".gz")):
            for algorithm in ("sha256", "sha512"):
                content = path.read_bytes()
                hashed = path.parent / "by-hash" / algorithm.upper() / hashlib.new(algorithm, content).hexdigest()
                hashed.parent.mkdir(parents=True, exist_ok=True)
                hashed.write_bytes(content)
    lines = ["Origin: Vector", "Label: Vector", f"Suite: {channel}", f"Codename: {channel}",
             "Date: " + datetime.now(timezone.utc).strftime("%a, %d %b %Y %H:%M:%S +0000"),
             "Architectures: " + " ".join(architectures), "Components: vector-0", "Acquire-By-Hash: yes"]
    for algorithm in ("sha256", "sha512"):
        lines.append(algorithm.upper() + ":")
        for arch in architectures:
            for filename in ("Packages", "Packages.gz"):
                relative = f"vector-0/binary-{arch}/{filename}"
                data = (suite / relative).read_bytes()
                lines.append(f" {hashlib.new(algorithm, data).hexdigest()} {len(data)} {relative}")
    release = suite / "Release"
    release.write_text("\n".join(lines) + "\n")
    signer.sign(release, clear=True, destination=suite / "InRelease")
    signer.sign(release, destination=suite / "Release.gpg")
    signer.verify(suite / "InRelease")
    signer.verify(suite / "Release.gpg", release)


def prepare(config, work):
    private_key = os.environ.pop("PACKAGE_SIGNING_SUBKEY", "")
    passphrase = os.environ.pop("PACKAGE_SIGNING_PASSPHRASE", "")
    manifest, input_id = verify_input(work)
    require(not (work / "result").exists(), "Prepared output already exists; reuse it or select a new work directory")
    base_data = (work / "base.json").read_bytes()
    previous = Snapshot.decode(base_data) if base_data != b"null\n" else None
    base_files = work / "base"
    # Check every downloaded base file before a private key is imported.
    for path in base_files.rglob("*"):
        require(not path.is_symlink(), "Base metadata cannot contain symlinks")
        if path.is_file():
            name = path.relative_to(base_files).as_posix()
            require(previous is not None and previous.entries.get(name) == byte_entry(path.read_bytes()), "Base metadata changed")
    for name, identity in manifest["assets"].items():
        if "format" in identity:
            lint_package(work / "input" / name, identity)
    public = PublicKey(config, "/run/keys/verify")
    try:
        if previous:
            verify_metadata(previous, base_files, public)
    finally:
        public.close()
    signer = None
    try:
        signer = Signer(config, "/run/keys/sign", private_key, passphrase)
        private_key = passphrase = None
        result = work / "result"
        root = result / "files"
        root.mkdir(parents=True)
        entries = dict(previous.entries) if previous else {}
        if previous:
            for source in base_files.rglob("*"):
                if source.is_file():
                    path = root / source.relative_to(base_files)
                    path.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(source, path)
        incoming = {}
        channel = manifest["channel"]
        for name, identity in manifest["assets"].items():
            if "format" not in identity:
                continue
            relative = (f"apt/pool/v/ve/{name}" if identity["format"] == "deb"
                        else f"yum/{channel}/vector-0/{identity['arch']}/{name}")
            if relative in entries:
                require(identity["format"] == "deb" and entries[relative] ==
                        {"size": identity["size"], "sha256": identity["sha256"]},
                        "Package is already present; reuse its saved signed publication")
            destination = root / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(work / "input" / name, destination)
            if identity["format"] == "rpm":
                cached = work / "reuse" / name
                if cached.exists():
                    require(cached.is_file() and not cached.is_symlink(), "Invalid saved signed RPM")
                    shutil.copyfile(cached, destination)
                    lint_package(destination, identity)
                    require("digests signatures OK" in run(["rpm", "--dbpath", signer.rpmdb, "--checksig", destination]).decode(),
                            "Saved RPM does not have a valid COSE signature")
                else:
                    signer.rpm(destination)
            else:
                incoming[identity["arch"]] = run(["apt-ftparchive", "packages", relative[4:]], cwd=root / "apt")
        for selected in ("stable", "beta"):
            if previous is None or selected == channel:
                apt_indexes(root, selected, incoming if selected == channel else {}, base_files, signer)
                for arch in RPM_ARCHES:
                    relative = f"yum/{selected}/vector-0/{arch}"
                    target = root / relative
                    target.mkdir(parents=True, exist_ok=True)
                    # Previously copied mutable roots are not input to createrepo.
                    shutil.rmtree(target / "repodata", ignore_errors=True)
                    run(["createrepo_c", "--no-database", "--checksum", "sha256", "--compress-type", "gz", target])
                    old = base_files / relative
                    if (old / "repodata/repomd.xml").exists():
                        with tempfile.TemporaryDirectory() as temporary:
                            run(["mergerepo_c", "--all", "--omit-baseurl", "--no-database", "--compress-type", "gz",
                                 "--repo", old.as_uri(), "--repo", target.as_uri(), "--outputdir", temporary])
                            shutil.rmtree(target / "repodata")
                            shutil.copytree(Path(temporary) / "repodata", target / "repodata")
                    repomd_locations(target)
                    signer.sign(target / "repodata/repomd.xml")
                    signer.verify(target / "repodata/repomd.xml.asc", target / "repodata/repomd.xml")
        for path in root.rglob("*"):
            if path.is_file():
                name = path.relative_to(root).as_posix()
                entry = {"size": path.stat().st_size, "sha256": sha256_file(path)}
                require(mutable(name) or name not in entries or entries[name] == entry, "Immutable repository object changed")
                entries[name] = entry
        snapshot = Snapshot(entries, root, base=previous.id if previous else None, input_id=input_id)
        # Metadata inherited from an untouched channel may refer to files held only
        # in the base workspace. Validate a metadata-only view without copying RPMs.
        with tempfile.TemporaryDirectory() as temporary:
            view = Path(temporary)
            for source in (base_files, root):
                for path in source.rglob("*"):
                    if path.is_file() and not path.name.endswith((".deb", ".rpm")):
                        destination = view / path.relative_to(source)
                        destination.parent.mkdir(parents=True, exist_ok=True)
                        shutil.copyfile(path, destination)
            verify_metadata(snapshot, view, signer)
        (result / "manifest.json").write_bytes(snapshot.data)
        (result / "input.json").write_bytes((work / "input.json").read_bytes())
        Snapshot.local(root)
        return snapshot.id
    finally:
        if signer is not None:
            signer.close()
        else:
            # A failed key check can leave an initialized agent; stop it before cleanup.
            if Path("/run/keys/sign").exists():
                run(["gpgconf", "--kill", "all"], env={**os.environ, "GNUPGHOME": "/run/keys/sign"})
                shutil.rmtree("/run/keys/sign")
