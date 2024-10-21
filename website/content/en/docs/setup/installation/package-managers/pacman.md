---
title: Install Vector using pacman
short: pacman
weight: 7
---

{{< info title="Community Maintained" >}}
The Vector pacman package is supported and maintained by the open source community.
{{< /info >}}

[pacman] is a utility which manages software packages in Linux, primarily on [Arch Linux] and its derivates. This covers installing and managing Vector through the Arch Linux [extra] package repository.

## Installation

```shell
sudo pacman -Syu vector
```

## Other actions

{{< tabs default="Uninstall Vector" >}}
{{< tab title="Uninstall Vector" >}}

```shell
sudo pacman -Rs vector
```

{{< /tab >}}
{{< /tabs >}}

## Management

{{< jump "/docs/administration/management" "apt-dpkg-rpm-yum-pacman" >}}

[pacman]: https://archlinux.org/pacman/
[Arch Linux]: https://archlinux.org/
[extra]: https://archlinux.org/packages/extra/x86_64/vector/

