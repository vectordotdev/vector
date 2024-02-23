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
* `distroless-libc`
* `distroless-static`
{{< /success >}}

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
