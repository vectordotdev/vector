---
title: Install Vector using YUM
short: YUM
weight: 9
---

The [Yellowdog Updater, Modified][yum] (YUM) is a free and open-source command-line package-manager for Linux operating system using the RPM Package Manager.

Our Yum repositories are provided by [Datadog]. This page covers installing and managing Vector
through the YUM package repository.

## Installation

Add the repo:

```shell
bash -c "$(curl -L https://setup.vector.dev)"
```

Then you can install Vector:

```shell
sudo yum install vector
```

## Other actions

{{< tabs default="Upgrade Vector" >}}
{{< tab title="Upgrade Vector" >}}

```shell
sudo yum upgrade vector
```

{{< /tab >}}
{{< tab title="Uninstall Vector" >}}

```shell
sudo yum remove vector
```

{{< /tab >}}
{{< /tabs >}}

## Management

{{< jump "/docs/administration/management" "apt-dpkg-rpm-yum-pacman" >}}

[Datadog]: https://www.datadoghq.com/
[yum]: https://en.wikipedia.org/wiki/Yum_(software)
