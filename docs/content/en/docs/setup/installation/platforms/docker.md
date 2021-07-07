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

{{< success >}}
Other available distributions (beyond `debian`):

* `alpine`
* `distroless`
{{< /success >}}

## Deployment

Vector is an end-to-end observability data pipeline designed to deploy under various roles. You mix and match these roles to create topologies. The intent is to make Vector as flexible as possible, allowing you to fluidly integrate Vector into your infrastructure over time. The deployment section demonstrates common Vector pipelines:

{{< jump "/docs/setup/deployment/topologies" >}}

## Administration

### Start

```shell
docker run \
  -d \
  -v ~/vector.toml:/etc/vector/vector.toml:ro \
  -p 8383:8383 \
  timberio/vector:{{< version >}}-debian
```

Make sure to substitute out `debian` if you're using a different distribution.

### Stop

```shell
docker stop timberio/vector
```

### Reload

```shell
docker kill --signal=HUP timberio/vector
```

### Restart

```shell
docker restart -f $(docker ps -aqf "name=vector")
```

### Observe

To tail the logs from your Vector image:

```shell
docker logs -f $(docker ps -aqf "name=vector")
```

To access metrics from your Vector image:

```shell
vector top
```

### Uninstall

```shell
docker rm timberio/vector
```

[docker]: https://docker.com
