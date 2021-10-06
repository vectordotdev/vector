---
title: Install Vector using RPM
short: RPM
weight: 7
---

RPM Package Manager is a free and open source package management system for installing and managing software on Fedora, CentOS, OpenSUSE, OpenMandriva, Red Hat Enterprise Linux, and related Linux-based systems. This covers installing and managing Vector through the RPM package repository.

## Installation

```shell
sudo rpm -i https://packages.timber.io/vector/{{< version >}}/vector-{{< version >}}-1.{arch}.rpm
```

Make sure to replace `{arch}` with one of the following:

* `x86_64`
* `arm64`
* `armv7`

## Other actions

{{< tabs default="Uninstall Vector" >}}
{{< tab title="Uninstall Vector" >}}

```shell
sudo rpm -e vector
```

{{< /tab >}}
{{< /tabs >}}

## Management

{{< jump "/docs/administration/management" "apt-dpkg-rpm-yum" >}}

[rpm]: https://rpm.org/
