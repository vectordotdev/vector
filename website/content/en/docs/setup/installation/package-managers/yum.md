---
title: Install Vector using YUM
short: YUM
weight: 8
---

The [Yellowdog Updater, Modified][yum] (YUM) is a free and open-source command-line package-manager for Linux operating system using the RPM Package Manager.

Our Yum repositories are provided by [Cloudsmith] and you can find [instructions for manually adding the repositories][add_repo]. This page covers installing and managing Vector through the YUM package repository.

## Installation

Add the repo:

```shell
curl -1sLf 'https://repositories.timber.io/public/vector/cfg/setup/bash.rpm.sh' \
  sudo -E bash
```

Then you can install Vector:

```shell
sudo yum install vector
```

## Deployment

Vector is an end-to-end observability data pipeline designed to deploy under various roles. You mix and match these roles to create topologies. The intent is to make Vector as flexible as possible, allowing you to fluidly integrate Vector into your infrastructure over time. The deployment section demonstrates common Vector pipelines:

{{< jump "/docs/setup/deployment/topologies" >}}

## Administration

### Upgrade

```shell
sudo yum upgrade vector
```

### Uninstall

```shell
sudo yum remove vector
```

[add_repo]: https://cloudsmith.io/~timber/repos/vector/setup/#formats-rpm
[cloudsmith]: https://cloudsmith.io/~timber/repos/vector/packages/
[yum]: https://en.wikipedia.org/wiki/Yum_(software)
