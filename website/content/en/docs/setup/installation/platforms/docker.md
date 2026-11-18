---
title: Install Vector on Docker
short: Docker
weight: 1
aliases: ["/docs/setup/installation/containers/docker"]
---

[Docker] is an open platform for developing, shipping, and running applications and services. With Docker, you can manage your infrastructure in the same ways you manage your services. By taking advantage of Docker's methodologies for shipping, testing, and deploying code quickly, you can significantly reduce the delay between writing code and running it in production. This page covers installing and managing Vector on the Docker platform.

## Installation

Pull the Vector image:

```shell
docker pull timberio/vector:{{< version >}}-debian
```

Vector images are also available from the [GitHub Container Registry]:

```shell
docker pull ghcr.io/vectordotdev/vector:{{< version >}}-debian
```

{{< success >}}
Other available distributions (beyond `debian`):

* `alpine`
* `distroless-libc`
* `distroless-static`
{{< /success >}}

### Collecting journals from previous boots

The official `debian` image uses Debian 13, which provides systemd 257. The
[`journald` source](/docs/reference/configuration/sources/journald/) works with
this image using the default `current_boot_only: true`, which excludes entries
from previous boots.

Setting `current_boot_only: false` requires systemd 258 or later, or a version
older than 250. Vector rejects this setting with systemd versions 250 through
257 because those versions cannot follow entries from all boots without changing
journal selection. The `alpine` and `distroless` variants do not include
`journalctl`.

For previous-boot collection, install Vector natively on a host with systemd 258
or later, or build a custom image that supplies a compatible `journalctl` and its
required libraries. Vector uses the `journalctl` installed inside the container;
upgrading systemd on the host does not change that version. Check the version
inside the image with:

```shell
docker run --rm --entrypoint /usr/bin/journalctl timberio/vector:{{< version >}}-debian --version
```

Mount the host's journal directory into the container and set `journal_directory`
to the mounted path. Persist Vector's `data_dir` to retain checkpoints across
container restarts. Reading previous boots also requires the host to retain those
journal entries.

## Deployment

Vector is an end-to-end observability data pipeline designed to deploy under various roles. You mix and match these roles to create topologies. The intent is to make Vector as flexible as possible, allowing you to fluidly integrate Vector into your infrastructure over time. The deployment section demonstrates common Vector pipelines:

{{< jump "/docs/setup/deployment/topologies" >}}

## Administration

### Configure

Create a new Vector configuration. The below will output dummy logs to stdout.

```shell
cat <<-EOF > $PWD/vector.yaml
api:
  enabled: true
  address: 0.0.0.0:8686
sources:
  demo_logs:
    type: demo_logs
    interval: 1
    format: json
sinks:
  console:
    inputs:
      - demo_logs
    target: stdout
    type: console
    encoding:
      codec: json
EOF
```

### Start

```shell
docker run \
  -d \
  -v $PWD/vector.yaml:/etc/vector/vector.yaml:ro \
  -p 8686:8686 \
  --name vector \
  timberio/vector:{{< version >}}-debian
```

Make sure to substitute out `debian` if you're using a different distribution.

### Stop

```shell
docker stop vector
```

### Reload

```shell
docker kill --signal=HUP vector
```

### Restart

```shell
docker restart $(docker ps -aqf "name=vector")
```

### Observe

To tail the logs from your Vector image:

```shell
docker logs -f $(docker ps -aqf "name=vector")
```

To access metrics from your Vector image:

```shell
docker exec -ti $(docker ps -aqf "name=vector") vector top
```

### Uninstall

```shell
docker rm vector
```

[docker]: https://docker.com
[GitHub Container Registry]: https://github.com/vectordotdev/vector/pkgs/container/vector
