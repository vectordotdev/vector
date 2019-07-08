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

```bash
docker run timberio/vector:latest
```

Once logged in, [start][docs.starting] Vector with:

```bash
vector --config=/etc/vector/vector.toml
```

See the "Custom Config" tab for how to use your own Vector confirmation file.
{% endtab %}

## Configuring

The default Vector configuration file is placed at:

```
etc/vector/vector.toml
```

You an customize Vector's configuration by mounting your custom `vector.toml`
file in the expected location:

```bash
docker run -v $PWD/vector.toml:/etc/vector/vector.toml:ro timberio/vector:latest
```

Modify `$PWD` to the directory where you store your local `vector.toml` file.

Once logged in, [start][docs.starting] Vector with:

```bash
vector --config=/etc/vector/vector.toml
```

You can learn more about configuring Vector in the
[Configuration][docs.configuration] section.

## Administering

Vector can be managed through the [Systemd][url.systemd] service manager:

{% page-ref page="../../../usage/administration" %}

## Image Variants

### timberio/vector:<version>

This is the defacto image. If you are unsure about what your needs are, you
probably want to use this one. It is designed to be used both as a throw away
container (mount your source code and start the container to start your app),
as well as the base to build other images off of.

### timberio/vector-slim:<version>

This image is based on `debian:9-slim` which is much smaller (up to 30x), and
thus leads to much slimmer images in general.

This variant is highly recommended when final image size being as small as
possible is desired. To minimize image size, it's uncommon for additional
related tools (such as git or bash) to be included. Using this image as a
base, add the things you need in your own Dockerfile.

## Versions

Timber's Docker images include a special `latest` version that will be updated
whenever Timber is [released][url.releases]. All other [releases][url.releases]
are available via the `X.X.X` tag:

```bash
docker run timberio/vector:latest
docker run timberio/vector:X.X.X
```

## Updating

Simply run the with the `latest` tag:

```bash
docker run timberio/vector:latest
```


[docs.configuration]: ../../../usage/configuration
[docs.starting]: ../../../usage/administration/starting.md
[url.default_configuration]: https://github.com/timberio/vector/blob/master/config/vector.toml
[url.docker_hub_vector]: https://hub.docker.com/r/timberio/vector
[url.releases]: https://github.com/timberio/vector/releases
[url.systemd]: https://www.freedesktop.org/wiki/Software/systemd/
