"""Opt-in real-package fixture. Its ephemeral private keys never leave tmpfs."""

import json
import os
from pathlib import Path
import secrets
import shutil

from inputs import require
from repository import Signer, key_fingerprints, prepare, run
from storage import PublicationError, Snapshot, canonical


def prepare_fixture(configuration, work):
    require(configuration["account"] == "092620891642" and configuration["bucket"] == "pront-ci-sandbox-092620891642-us-east-1",
            "Real-package fixture generation is restricted to the private COSE sandbox")
    config = dict(configuration)
    home = Path("/run/keys/generate")
    home.mkdir(mode=0o700)
    password = secrets.token_urlsafe(48)
    env = {**os.environ, "GNUPGHOME": str(home)}
    def gpg(*args, protected=False):
        command = ["gpg", "--batch", "--no-tty", "--rfc4880", "--cert-digest-algo", "SHA256"]
        if protected:
            command += ["--pinentry-mode", "loopback", "--passphrase-fd", "0"]
        return run([*command, *args], env=env, data=(password + "\n").encode() if protected else None)
    try:
        gpg("--quick-generate-key", "Vector ephemeral repository test", "rsa4096", "cert", "2d", protected=True)
        primary = key_fingerprints(gpg("--with-colons", "--list-keys"), "pub", "sub")[0][1]
        gpg("--quick-add-key", primary, "rsa4096", "sign", "2d", protected=True)
        subkey = key_fingerprints(gpg("--with-colons", "--list-keys"), "pub", "sub")[1][1]
        key = gpg("--armor", "--export-secret-subkeys", subkey + "!", protected=True).decode()
        private_primary = gpg("--armor", "--export-secret-keys", primary, protected=True).decode()
        public = gpg("--armor", "--export", primary)
        directory = work / "config"
        directory.mkdir()
        (directory / "public.asc").write_bytes(public)
        (directory / "public.gpg").write_bytes(gpg("--export", primary))
        config.update(primary_fingerprint=primary, signing_fingerprint=subkey, public_key="public.asc",
                      public_key_path=str(directory / "public.asc"), require_existing_repository=False)
        rejected = False
        try:
            Signer(config, "/run/keys/reject-primary", private_primary, password)
        except PublicationError:
            rejected = True
        finally:
            run(["gpgconf", "--kill", "all"], env={**os.environ, "GNUPGHOME": "/run/keys/reject-primary"})
            shutil.rmtree("/run/keys/reject-primary")
        require(rejected, "The signer accepted a primary secret key")
        private_primary = None
        print("PASS: production signer rejects a usable primary secret key", flush=True)
        one, two = work / "one", work / "two"
        (one / "base").mkdir()
        (one / "base.json").write_bytes(b"null\n")
        os.environ.update(PACKAGE_SIGNING_SUBKEY=key, PACKAGE_SIGNING_PASSPHRASE=password)
        first = prepare(config, one)
        print("PASS: encrypted subkey prepared the first six-package release", flush=True)
        (two / "base").mkdir()
        (two / "base.json").write_bytes((one / "result/manifest.json").read_bytes())
        for source in (one / "result/files").rglob("*"):
            if source.is_file() and not source.name.endswith((".deb", ".rpm")):
                target = two / "base" / source.relative_to(one / "result/files")
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(source, target)
        os.environ.update(PACKAGE_SIGNING_SUBKEY=key, PACKAGE_SIGNING_PASSPHRASE=password)
        second = prepare(config, two)
        snapshot = Snapshot.decode((two / "result/manifest.json").read_bytes())
        require(snapshot.base == first, "Incremental generation lost its base")
        for path in (two / "result/files/apt/dists/stable/vector-0").glob("binary-*/Packages"):
            if path.parent.name not in {"binary-all", "binary-i386"}:
                require(path.read_text().count("Package: vector\n") == 2, "Incremental apt history lost a release")
        for name in Snapshot.decode((one / "result/manifest.json").read_bytes()).entries:
            if name.endswith((".deb", ".rpm")):
                require(not (two / "result/files" / name).exists(), "Second generation copied historical payloads")
        # A final release may first be selected for beta. Both channels share the
        # apt pool, and the source-digest cache must reuse RPM signatures.
        beta = work / "beta-check"
        beta.mkdir()
        source = json.loads((two / "input.json").read_bytes())
        source["channel"] = "beta"
        (beta / "input.json").write_bytes(canonical(source))
        shutil.copytree(two / "input", beta / "input", copy_function=os.link)
        (beta / "base.json").write_bytes(snapshot.data)
        (beta / "base").mkdir()
        for path in (two / "result/files").rglob("*"):
            if path.is_file() and not path.name.endswith((".deb", ".rpm")):
                target = beta / "base" / path.relative_to(two / "result/files")
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(path, target)
        (beta / "reuse").mkdir()
        for name, asset in source["assets"].items():
            if asset.get("format") == "rpm":
                shutil.copyfile(two / f"result/files/yum/stable/vector-0/{asset['arch']}" / name, beta / "reuse" / name)
        os.environ.update(PACKAGE_SIGNING_SUBKEY=key, PACKAGE_SIGNING_PASSPHRASE=password)
        prepare(config, beta)
        promoted = Snapshot.decode((beta / "result/manifest.json").read_bytes())
        for name, asset in source["assets"].items():
            if asset.get("format") == "rpm":
                relative = f"vector-0/{asset['arch']}/{name}"
                require(promoted.entries["yum/beta/" + relative] == snapshot.entries["yum/stable/" + relative],
                        "Selecting another channel changed RPM signatures")
        shutil.rmtree(beta)
        from tests.rollback_fixture import prepare_fixture as prepare_rollback_fixture
        prepare_rollback_fixture(config, work, key, password)
        from tests.installer_fixture import installer_fixture
        installer_fixture(config, work, gpg, password)
        config.pop("public_key_path")
        config.pop("role", None)
        (directory / "staging.json").write_bytes(canonical(config))
        (work / "fixture-report.json").write_bytes(canonical({"first": first, "second": second,
            "primary_fingerprint": primary, "signing_fingerprint": subkey,
            "checks": ["primary secret rejected", "encrypted subkey signing", "SHA-256 RPM signatures",
                       "apt/dnf metadata signatures", "all six architecture packages", "incremental retained history",
                       "historical payloads not copied", "channel selection reuses shared Debian bytes and RPM signatures",
                       "fresh rollback and recovery roots", "pending root date covered", "corrupt source rejected before key import"]}))
        print("PASS: incremental metadata retains both versions without copying old packages", flush=True)
        print("PASS: explicit beta selection reuses the existing package bytes and RPM signatures", flush=True)
    finally:
        os.environ.pop("PACKAGE_SIGNING_SUBKEY", None)
        os.environ.pop("PACKAGE_SIGNING_PASSPHRASE", None)
        run(["gpgconf", "--kill", "all"], env=env)
        shutil.rmtree(home)
