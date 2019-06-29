# Docker Distribution

This folder contains Docker images used to test, build, and release Vector
across various targets. The purpose of these images is to:

1. Completely contain our CI process.
2. Decouple our CI process from any specific vendor.
3. Make it easier to debug and develop locally.

## Building Images

Building images is simple:

```
./build.sh
```

## Adding Images

To add an image:

1. Create a folder representing the new name of your image (note the
   [image types](#image-types) below).
2. Add a `Dockerfile` within that folder.
3. Add your image to the bottom of `build.sh`.

## Image Types

### `builder-base-*` Images

You'll notice that some of our `builder-*` images extend `builde-base-*`
images. Ex:

```
FROM timberiodev/vector-builder-base-x86_64-unknown-linux-musl:latest
```

These "base" images are built from the [Cross Docker images][cross_docker].
We do this so that we can maintain separate from our own images making it
easy to sync with upstream changes. If cross makes changes we simply rebuild
and do not have to deal with conflicts.

The purpose of these base images is to build fresh images with updated
dependencies since we cannot control with cross will release images. In some
cases this resolved build issues, and in other cases it made things worse.
For example, building a fresh image for the `x86_64-unknown-linux-gnu` target
made Vector less portable because it upp'd `libc` requirement.

## `builder-*` Images

The `builder-*` images are our own target builder images. They represent
the finaly environment where the build actually happens. In some cases they
extend the `builde-base-*` images described above.

## `releaser-*` Images

These images are responsible for releasing and uploading Vector to the
appropriate channels (S3, PackageCloud, Github, etc).

## `verifier-*` Images

These images verify build artifacts, ensuring they work on the appropriate
environments.


[cross_docker]: https://github.com/rust-embedded/cross/tree/master/docker