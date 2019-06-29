---
description: Install Vector through the YUM package manager
---

# YUM Package Manager

Vector can be installed through the [YUM package manager][url.yum] which is
generally used on CentOS.

## Install

Start by adding the Timber GPG key and repository (Timber is the company behind Vector):

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

Vector installs with a [`vector.service` Systemd file][url.vector_systemd_file].
See the [Administration guide][docs.administration]] for more info.

## Uninstalling

```bash
yum remove vector
```

## Updating

```bash
sudo yum upgrade vector
```


[docs.administration]: ../../..docs/usage/administration
[docs.configuration]: ../../..docs/usage/configuration
[url.vector_systemd_file]: https://github.com/timberio/vector/blob/master/distribution/systemd/vector.service
[url.yum]: http://yum.baseurl.org/
