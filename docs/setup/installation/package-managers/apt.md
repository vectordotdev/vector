---
description: Install Vector through the APT package manager
---

# APT Package Manager

Vector can be installed through the [APT package manager][urls.apt] which is
generally used on Debian and Ubuntu systems.

## Install

{% tabs %}
{% tab title="apt-get" %}
Start by adding the Timber GPG key and repository:

```bash
curl -s <%= metadata.links.fetch("urls.vector_deb_repository_setup_script") %> | sudo bash
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
{% endtab %}
{% tab title="dpkg" %}
Download the [Vector `.deb file`][urls.urls.vector/latest/vector-amd64.deb]:

```bash
curl -O <%= metadata.links.fetch("urls.vector/latest/vector-amd64.deb") %>
```

Then install the Vector `.deb` package directly:

```bash
sudo dpkg -i vector-amd64.deb
```

Start Vector:

```bash
sudo systemctl start vector
```

That's it! Proceed to [configure](#configuring) Vector for your use case.
{% endtabs %}
{% endtab %}


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

## Versions

Historical Vector versions can be found in the [releases][urls.vector_releases].
Once you've found the version you'd like to install you can specify it with:

```bash
sudo apt-get install vector=X.X.X
```


[docs.configuration]: ../../../usage/configuration
[urls.apt]: https://wiki.debian.org/Apt
[urls.systemd]: https://www.freedesktop.org/wiki/Software/systemd/
[urls.vector_releases]: https://github.com/timberio/vector/releases
