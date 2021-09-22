---
title: Install Vector using APT
short: APT
weight: 1
---

[Advanced Package Tool][apt], or **APT**, is a free package manager that handles the installation and removal of software on [Debian], [Ubuntu], and other [Linux] distributions.

Our APT repositories are provided by [Cloudsmith] and you can find [instructions][repos] for manually adding the repositories. This page covers installing and managing Vector through the [APT package repository][apt].

## Supported architectures

* x86_64
* ARM64
* ARMv7

## Installation

First, add the Vector repo:

```shell
curl -1sLf \
  'https://repositories.timber.io/public/vector/cfg/setup/bash.deb.sh' \
| sudo -E bash
```

Then you can install the `vector` package:

```shell
sudo apt-get install vector
```

## Other actions

{{< tabs default="Upgrade Vector" >}}
{{< tab title="Upgrade Vector" >}}

```bash
sudo apt-get upgrade vector
```

{{< /tab >}}
{{< tab title="Uninstall Vector" >}}

```bash
sudo apt remove vector
```

{{< /tab >}}
{{< /tabs >}}

## Management

{{< jump "/docs/administration/management" "apt-dpkg-rpm-yum" >}}

[apt]: https://en.wikipedia.org/wiki/APT_(software)
[cloudsmith]: https://cloudsmith.io/~timber/repos/vector/packages
[debian]: https://debian.org
[linux]: https://linux.org
[repos]: https://cloudsmith.io/~timber/repos/vector/setup/#formats-deb
[ubuntu]: https://ubuntu.com
