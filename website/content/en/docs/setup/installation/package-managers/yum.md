---
title: Install Vector using YUM
short: YUM
weight: 9
---

The [Yellowdog Updater, Modified][yum] (YUM) is a free and open-source command-line package-manager for Linux operating system using the RPM Package Manager.

Our Yum repositories are provided by [Datadog]. This page covers installing and managing Vector
through the YUM package repository.

**Signing key change notice:** On November 30, 2026, Vector's package repositories
will switch to a new signing key. Existing installations must [add the new
key](/deprecations/#package-repository-signing-key) before this date to continue
receiving package updates.

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
