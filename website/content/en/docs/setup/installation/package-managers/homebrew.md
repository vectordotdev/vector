---
title: Install Vector using Homebrew
short: Homebrew
weight: 4
---

[Homebrew] is a free and open source package management system for Apple's macOS operating system and some supported Linux systems. This page covers installing and managing Vector using the Homebrew package repository.


## Installation

```shell
brew tap timberio/brew && brew install vector
```

## Deployment

Vector is an end-to-end observability data pipeline designed to deploy under various roles. You mix and match these roles to create topologies. The intent is to make Vector as flexible as possible, allowing you to fluidly integrate Vector into your infrastructure over time. The deployment section demonstrates common Vector pipelines:

{{< jump "/docs/setup/deployment/topologies" >}}

## Administration

### Start

```shell
brew services start vector
```

### Stop

```shell
brew services stop vector
```

### Reload

```shell
killall -s SIGHUP vector
```

### Restart

```shell
brew services restart vector
```

### Upgrade

```shell
brew update && brew upgrade vector
```

### Uninstall

```shell
brew remove vector
```

[homebrew]: https://brew.sh
