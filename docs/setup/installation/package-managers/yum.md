---
description: Install Vector through the YUM package manager
---

# YUM Package Manager

Vector can be installed through the [YUM package manager][urls.yum] which is
generally used on CentOS.

## Install

Start by adding the Timber GPG key and repository:

```bash
curl -s https://packagecloud.io/install/repositories/timberio/packages/script.rpm.sh | sudo bash
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


[docs.configuration]: ../../../usage/configuration
[urls.systemd]: https://www.freedesktop.org/wiki/Software/systemd/
[urls.yum]: http://yum.baseurl.org/
