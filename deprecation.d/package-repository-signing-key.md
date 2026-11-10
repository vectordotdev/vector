---
what: "APT/YUM repository configuration that trusts only Datadog signing keys"
deprecated_since: "0.62.0"
---

On **November 30, 2026**, Vector's APT and YUM repositories switch to a new
signing key. To continue installing or updating packages after that date, add
the new key.

On each host that you configured with `setup.vector.dev`, run the following
commands before November 30, 2026. The script adds the new key and keeps your
repository URL, channel, and existing trusted keys:

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
