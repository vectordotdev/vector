---
title: Use Vector On Docker
sidebar_label: Docker
description: Using Vector on Docker
---

Vector maintains the [`timberio/vector` Docker images][urls.docker_hub_vector]
available on [Docker Hub][urls.docker_hub_vector] which come pre-installed
with Vector and any recommended system dependencies.

## Running

```bash
docker run timberio/vector:latest-alpine
```

* The `vector` binary is located at `/usr/local/bin/vector`, which should be in your `$PATH`.
* The default [configuration file][docs.configuration] is located at `/etc/vector/vector.toml`.

## Configuring

The Vector Docker images ship with a [default `/etc/vector/vector.toml` configuration file][urls.default_configuration].
To use your own configuration file:

1. Create your own [Vector configuration file][docs.configuration] and save it
   as `vector.toml`.

2. Run the Vector Docker image with the following command:

   ```bash
   docker run -v $PWD/vector.toml:/etc/vector/vector.toml:ro timberio/vector:latest-alpine
   ```

   Modify `$PWD` to the directory where you store your local `vector.toml` file.

## Image Variants

### alpine

This image is based on the [`alpine` Docker image][urls.docker_alpine], which is
a Linux distribution built around musl libc and BusyBox. It is considerably
smaller in size than other Docker images and statically links libraries. This
is the image we recommend due to it's small size and reliability.

```bash
docker run timberio/vector:latest-alpine
```

### debian

This image is based on the [`debian-slim` image][urls.docker_debian],
which is a smaller, more compact variant of the [`debian` image][urls.docker_debian].

```bash
docker run timberio/vector:latest-debian
```

## Versions

### Latest Version

Vector's Docker images include a special `latest` version that will be updated
whenever Vector is [released][urls.vector_releases]:

```bash
docker run timberio/vector:latest-alpine
```

### Previous Versions

Previous versions can be accessed by their direct tag:

```bash
docker run timberio/vector:<X.X.X>-alpine
```

### Nightlies

Vector's releases nightly versions that contain the latest changes.

```bash
docker run timberio/vector:nightly-alpine
```

## Updating

Simply run the with the `latest` tag:

```bash
docker run timberio/vector:latest-alpine
```

Or specify the exact version:

```bash
docker run timberio/vector:X.X.X-alpine
```

Or specify the nigtly version:

```bash
docker run timberio/vector:nightly-alpine
```


[docs.configuration]: /docs/setup/configuration
[urls.default_configuration]: https://github.com/timberio/vector/blob/master/config/vector.toml
[urls.docker_alpine]: https://hub.docker.com/_/alpine
[urls.docker_debian]: https://hub.docker.com/_/debian
[urls.docker_hub_vector]: https://hub.docker.com/r/timberio/vector
[urls.vector_releases]: https://github.com/timberio/vector/releases
