---
title: Install Vector using Homebrew
short: Homebrew
weight: 4
---

[Homebrew] is a free and open source package management system for Apple's macOS operating system and some supported Linux systems. This page covers installing and managing Vector using the Homebrew package repository.

## Installation

```shell
brew tap vectordotdev/brew && brew install vector
```

## Other actions

{{< tabs default="Upgrade Vector" >}}
{{< tab title="Upgrade Vector" >}}

```shell
brew update && brew upgrade vector
```

{{< /tab >}}
{{< tab title="Uninstall Vector" >}}

```shell
brew remove vector
```

{{< /tab >}}
{{< /tabs >}}

## Management

{{< jump "/docs/administration/management" "homebrew" >}}

[homebrew]: https://brew.sh
