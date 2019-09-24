---
description: Install Vector through the APT package manager
---

# APT Package Manager

Vector can be installed through the [APT package manager][urls.apt] which is
generally used on Debian and Ubuntu systems.

## Install

Start by adding the Timber GPG key and repository:

```bash
curl -s https://packagecloud.io/install/repositories/timberio/packages/script.deb.sh | sudo bash
```

Install Vector:

```bash
sudo apt-get install vector
```

Start Vector:

```bash
sudo systemctl start vector
```

That's it! Proceed to [configure](#configuring) Vector for your use case.

## Configuring

The Vector configuration file is placed in:

```
etc/vector/vector.toml
```

A full spec is located at `/etc/vector/vector.spec.toml` and examples are
located in `/etc/vector/examples/*`. You can learn more about configuring
Vector in the [Configuration][docs.configuration] section.

## Administering

Vector can be managed through the [Systemd][urls.systemd] service manager:

{% page-ref page="../../../usage/administration" %}

## Uninstalling

```bash
apt-get remove vector
```

## Updating

Simply run the same `apt-get install` command

```bash
sudo apt-get install vector
```


[docs.configuration]: ../../../usage/configuration
[urls.apt]: https://wiki.debian.org/Apt
[urls.systemd]: https://www.freedesktop.org/wiki/Software/systemd/
