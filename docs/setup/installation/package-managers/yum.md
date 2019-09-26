---
description: Install Vector through the YUM package manager
---

# YUM Package Manager

Vector can be installed through the [YUM package manager][urls.yum] which is
generally used on CentOS.

## Install

{% tabs %}
{% tab title="yum" %}
Start by adding the Timber GPG key and repository:

```bash
curl -s <%= metadata.links.fetch("urls.vector_rpm_repository_setup_script") %> | sudo bash
```

Install Vector:

```bash
sudo yum install vector
```

Start Vector:

```bash
sudo systemctl start vector
```

That's it! Proceed to [configure](#configuring) Vector for your use case.
{% endtab %}
{% tab title="rpm" %}
Download the [Vector `.rpm file`][urls.vector_downloads/latest/vector-x86_64.rpm]

```bash
curl -O <%= metadata.links.fetch("urls.vector/latest/vector-x86_64.rpm") %>
```

Then install the Vector `.rpm` package directly:

```bash
sudo rpm -i vector-x86_64.rpm
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
yum remove vector
```

## Updating

```bash
sudo yum upgrade vector
```

## Versions

Historical Vector versions can be found in the [releases][urls.vector_releases].
Once you've found the version you'd like to install you can specify it with:

```bash
sudo yum install vector-X.X.X
```


[docs.configuration]: ../../../usage/configuration
[urls.systemd]: https://www.freedesktop.org/wiki/Software/systemd/
[urls.vector_downloads/latest/vector-x86_64.rpm]: https://packages.timber.io/vector/latest/vector-x86_64.rpm
[urls.vector_releases]: https://github.com/timberio/vector/releases
[urls.yum]: http://yum.baseurl.org/
