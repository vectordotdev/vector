---
what: "APT/YUM repository configuration that trusts only Datadog signing keys"
deprecated_since: "0.62.0"
---

## APT/YUM signing key migration {#package-repository-signing-key}

Vector's APT and YUM repositories will switch to a new signing key on
**November 30, 2026**; existing users must add this key to continue installing or
updating packages.

On each host configured through `setup.vector.dev`, run these commands before
the switch to add the new key while retaining your repository URL, channel and
existing Datadog trust:

```sh
curl -fsSL --proto '=https' --proto-redir '=https' \
  https://install.datadoghq.com/vector/setup.sh \
  -o vector-setup.sh &&
bash vector-setup.sh --migrate
```

After the switch, refresh repository metadata and upgrade Vector:

```sh
# Debian / Ubuntu
sudo apt-get update && sudo apt-get install --only-upgrade vector
```

```sh
# RPM systems
sudo dnf --refresh upgrade vector
# If dnf is unavailable: sudo yum clean metadata && sudo yum update vector
```

For custom source layouts, add the [new public key][package-key] to the keyrings
referenced by APT's `Signed-By` or to YUM/DNF's `gpgkey` configuration, keeping
existing trusted keys.

[package-key]: https://install.datadoghq.com/vector/keys/vector-packages.asc
