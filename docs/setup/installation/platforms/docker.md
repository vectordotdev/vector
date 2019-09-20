---
description: Using Vector on Docker
---

# Docker

Vector maintains the [`timberio/vector` Docker image][url.docker_hub_vector]
available on [Docker Hub][url.docker_hub_vector].

## Installation

Vector ships with a [default `vector.toml` file][url.default_configuration]
as a proof of concept. This is used to test Vector and ensure it is installed
and working:

{% tabs %}
{% tab title="alpine (recommended)" %}
```bash
docker run \
  -v $PWD/vector.toml:/etc/vector/vector.toml:ro \
  timberio/vector:latest-alpine
```

Modify `$PWD` to the directory where you store your local `vector.toml` file.
A couple of things to note:

1. The `vector` binary is located at `/usr/local/bin/vector`, which should be in
your `$PATH`.
2. Configuration is located in `/etc/vector`.
{% endtab %}
{% tab title="debian-slim" %}
```bash
docker run \
  -v $PWD/vector.toml:/etc/vector/vector.toml:ro \
  timberio/vector:latest-debian-slim
```

Modify `$PWD` to the directory where you store your local `vector.toml` file.
A few things to note:

1. The `vector` binary is located at `/usr/local/bin/vector`, which should be in
your `$PATH`.
2. A Systemd script is also installed; you can start vector via `systemctl start vector`.
3. Configuration is located in `/etc/vector`.
{% endtab %}
{% tab title="debian" %}
```bash
docker run \
  -v $PWD/vector.toml:/etc/vector/vector.toml:ro \
  timberio/vector:latest-debian
```

Modify `$PWD` to the directory where you store your local `vector.toml` file.
A few things to note:

1. The `vector` binary is located at `/usr/local/bin/vector`, which should be in
your `$PATH`.
2. A Systemd script is also installed; you can start vector via `systemctl start vector`.
3. Configuration is located in `/etc/vector`.
{% endtab %}
{% endtabs %}

## Configuring

You'll notice in the [Installation](#installation) section a custom
configuration is mounted with the `-v` flag. This is how we recommend
customizing configuration in the Docker environment. Alternatively,
you can extend the `timberio/vector` images and `COPY` your custom config
into your own image.

You can learn more about configuring Vector in the
[Configuration][docs.configuration] section.

## Image Variants

### timberio/vector:&lt;version&gt;-alpine

This image is based on [`alpine:latest`][url.docker_alpine] which is a Linux
distribution built around musl libc and BusyBox. It is considerably smaller in
size than other Docker images and statically links libraries. This is the image
we recommend due to it's small size and reliability.

### timberio/vector:&lt;version&gt;-debian-slim

This image is based on `debian:9-slim` which is much smaller (up to 30x), and
thus leads to much slimmer images in general.

This variant is highly recommended when final image size being as small as
possible is desired. To minimize image size, it's uncommon for additional
related tools (such as git or bash) to be included. Using this image as a
base, add the things you need in your own Dockerfile.

### timberio/vector:&lt;version&gt;-debian

This is the defacto image. If you are unsure about what your needs are, you
probably want to use this one. It is designed to be used both as a throw away
container (mount your source code and start the container to start your app),
as well as the base to build other images off of.

## Versions

Timber's Docker images include a special `latest` version that will be updated
whenever Timber is [released][url.releases]. All other [releases][url.releases]
are available via the `X.X.X` tag:

```bash
docker run timberio/vector:latest-alpine
docker run timberio/vector:X.X.X-alpine
```

## Updating

Simply run the with the `latest` tag:

```bash
docker run timberio/vector:latest-alpine
```


[docs.configuration]: ../../../usage/configuration/README.md
[url.default_configuration]: https://github.com/timberio/vector/blob/master/config/vector.toml
[url.docker_alpine]: https://hub.docker.com/_/alpine
[url.docker_hub_vector]: https://hub.docker.com/r/timberio/vector
[url.releases]: https://github.com/timberio/vector/releases
