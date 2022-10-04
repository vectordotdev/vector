---
title: Install Vector using pacman
short: pacman
weight: 7
---

[pacman] is a utility which manages software packages in Linux, primarily on [Arch Linux] and its derivates. This covers installing and managing Vector through the Arch Linux [community] package repository.

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
[community]: https://archlinux.org/packages/community/x86_64/vector/