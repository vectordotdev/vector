---
title: Management
description: How to start, stop, reload, and restart your Vector instance
short: Management
weight: 1
tags: ["process", "admin", "reload", "start", "stop", "restart"]
---

The sections below show you how to administer your Vector instance—start, stop, reload, etc.—in a variety of settings:

* [Vector executable](#vector-executable) (no process manager)
* [Linux](#linux) (systemctl process manager)
* [macOS](#macos) (Homebrew service manager)
* [Windows](#windows)
* [Docker](#docker)
* [Kubernetes with Helm](#helm)

### Vector executable

To manage the Vector executable directly, without a process manager:

{{< tabs default="Start" >}}
{{< tab title="Start" >}}

```bash
vector --config /etc/vector/vector.yaml

# Or supply a JSON or YAML config file
```

{{< /tab >}}
{{< tab title="Reload" >}}

```bash
killall -s SIGHUP vector
```

{{< /tab >}}
{{< /tabs >}}

### Linux

#### APT, dpkg, RPM, YUM, pacman

If you've installed Vector using [APT], [dpkg], [RPM], [YUM] or [pacman], you can manage it using [systemctl].

{{< tabs default="Start" >}}
{{< tab title="Start" >}}

```bash
sudo systemctl start vector
```

{{< /tab >}}
{{< tab title="Stop" >}}

```bash
sudo systemctl stop vector
```

{{< /tab >}}
{{< tab title="Reload" >}}

```bash
systemctl kill -s HUP --kill-who=main vector.service
```

{{< /tab >}}
{{< tab title="Restart" >}}

```bash
sudo systemctl restart vector
```

{{< /tab >}}
{{< /tabs >}}

#### Nix

If you've installed Vector using [Nix], you can manage it using the commands laid out in the [Vector
executable](#vector-executable) section.

### macOS

If you're running Vector on macOS, you can manage it using either the [executable](#vector-executable) commands or
[Homebrew](#homebrew).

#### Homebrew

If you've installed Vector using [Homebrew], you can manage it using Homebrew's [services][brew_services] utility.

{{< tabs default="Start" >}}
{{< tab title="Start" >}}

```bash
brew services start vector
```

{{< /tab >}}
{{< tab title="Stop" >}}

```bash
brew services stop vector
```

{{< /tab >}}
{{< tab title="Reload" >}}

```bash
killall -S SIGHUP vector
```

{{< /tab >}}
{{< tab title="Restart" >}}

```bash
brew services restart vector
```

{{< /tab >}}
{{< /tabs >}}

### Windows

If you're running Vector on Windows (perhaps you installed it using [MSI]), you can manage it using these commands:

{{< tabs default="Start" >}}
{{< tab title="Start" >}}

```powershell
C:\Program Files\Vector\bin\vector \
  --config C:\Program Files\Vector\config\vector.yaml

# Or supply a TOML or JSON config file
```

{{< /tab >}}
{{< /tabs >}}

### Docker

If you're running Vector using [Docker], the command interface is the same across all platforms.

{{< tabs default="Start" >}}
{{< tab title="Start" >}}

```bash
docker run \
  -d \
  -v ~/vector.yaml:/etc/vector/vector.yaml:ro \
  -p 8686:8686 \
  timberio/vector:{{< version >}}-alpine
```

{{< /tab >}}
{{< tab title="Stop" >}}

```bash
docker stop timberio/vector
```

{{< /tab >}}
{{< tab title="Reload" >}}

```bash
docker kill --signal=HUP timberio/vector
```

{{< /tab >}}
{{< tab title="Restart" >}}

```bash
docker restart -f $(docker ps -aqf "name=vector")
```

{{< /tab >}}
{{< /tabs >}}

The commands above involve configuring Vector using TOML, but you can also use JSON or YAML. You can also use one of
three image variants (the commands assume `alpine`):

Variant | Image basis
:-------|:-----------
`alpine` | [Alpine](https://hub.docker.com/_/alpine), a Linux distro built around [musl libc](https://www.musl-libc.org) and [BusyBox](https://busybox.net)
`debian` | The [`debian-slim`](https://hub.docker.com/_/debian) image, which is a smaller and more compact version of the standard `debian` image
`distroless` | The [Distroless](https://github.com/GoogleContainerTools/distroless) project, which provides extremely lean images with no package managers, shells, or other inessential utilities

### Helm

To get Vector running on [Kubernetes] using the [Helm] package manager:

{{< jump "/docs/setup/installation/package-managers/helm" >}}

Once Vector is running in Kubernetes, you can manage it using [kubectl]:

{{< tabs default="Restart Agent" >}}
{{< tab title="Restart Agent" >}}

```shell
kubectl rollout restart --namespace vector daemonset/vector-agent
```

{{< /tab >}}
{{< tab title="Restart Aggregator" >}}

```shell
kubectl rollout restart --namespace vector statefulset/vector-aggregator
```

{{< /tab >}}
{{< /tabs >}}

## Reloading

As you can see above, many administrative interfaces for Vector enable you to trigger a restart of a Vector instance while it's running. There are a few things that you should know about reloading.

### Automatic reloading on configuration change

You can make Vector automatically reload itself when its [configuration file][configuration] changes by setting the `--watch-config` or `-w` [flag][watch_config] when you first start your Vector instance.

## How it works

Running Vector instances accept the IPC [signals](#signals) and produce the [exit codes](#exit-codes) listed below.

{{< administration/process >}}

[apt]: /docs/setup/installation/package-managers/apt
[brew_services]: https://github.com/Homebrew/homebrew-services
[bug]: https://github.com/vectordotdev/vector/issues/new?labels=type%3A+bug
[configuration]: /docs/reference/configuration
[docker]: /docs/setup/installation/platforms/docker
[dpkg]: /docs/setup/installation/package-managers/dpkg
[helm]: https://helm.sh
[homebrew]: /docs/setup/installation/package-managers/homebrew
[kubectl]: https://kubernetes.io/docs/reference/kubectl
[kubernetes]: https://kubernetes.io
[msi]: /docs/setup/installation/package-managers/msi
[nix]: /docs/setup/installation/package-managers/nix
[rpm]: /docs/setup/installation/package-managers/rpm
[pacman]: /docs/setup/installation/package-managers/pacman
[sources]: /docs/reference/configuration/sources
[systemctl]: https://man7.org/linux/man-pages//man1/systemctl.1.html
[watch_config]: /docs/reference/cli/#vector-watch-config
[yum]: /docs/setup/installation/package-managers/yum
