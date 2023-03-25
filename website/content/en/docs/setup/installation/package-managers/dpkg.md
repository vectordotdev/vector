---
title: Install Vector using dpkg
short: dpkg
weight: 2
---

[dpkg] is the software that powers the package management system in the Debian operating system and its derivatives. dpkg is used to install and manage software via `.deb` packages. This page covers installing and managing Vector through the DPKG package repository.

## Installation

```shell
curl \
  --proto '=https' \
  --tlsv1.2 -O \
  https://packages.timber.io/vector/{{< version >}}/vector_{{< version >}}-1_amd64.deb

sudo dpkg -i vector_{{< version >}}-1_amd64.deb
```

## Other actions

{{< tabs default="Upgrade Vector" >}}
{{< tab title="Upgrade Vector" >}}

```shell
dpkg -i vector-{{< version >}}-amd64
```

{{< /tab >}}
{{< tab title="Uninstall Vector" >}}

```shell
dpkg -r vector-{{< version >}}-amd64
```

{{< /tab >}}
{{< /tabs >}}

## Management

{{< jump "/docs/administration/management" "apt-dpkg-rpm-yum-pacman" >}}

[dpkg]: https://wiki.debian.org/dpkg
