---
title: Install Vector using dpkg
short: dpkg
weight: 2
---

[dpkg] is the software that powers the package management system in the Debian operating system and its derivatives. dpkg is used to install and manage software via `.deb` packages. This page covers installing and managing Vector through the DPKG package repository.

## Installation

Install Vector:

```shell
curl --proto '=https' --tlsv1.2 -O https://packages.timber.io/vector/{{< version >}}/vector-{{< version >}}-amd64.deb && \
  sudo dpkg -i vector-{{< version >}}-amd64.deb
```

## Deployment

Vector is an end-to-end observability data pipeline designed to deploy under various roles. You mix and match these roles to create topologies. The intent is to make Vector as flexible as possible, allowing you to fluidly integrate Vector into your infrastructure over time. The deployment section demonstrates common Vector pipelines:

{{< jump "/docs/setup/deployment/topologies" >}}

## Administration

### Start

```shell
sudo systemctl start vector
```

### Stop

```shell
sudo systemctl stop vector
```

### Reload

```shell
systemctl kill -s HUP --kill-who=main vector.service
```

### Restart

```shell
sudo systemctl restart vector
```

[dpkg]: https://wiki.debian.org/dpkg
