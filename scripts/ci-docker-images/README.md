# Docker Distribution

This folder contains Docker images used to test, build, and release Vector
across various targets. The purpose of these images is to:

1. Completely contain our CI process.
2. Decouple our CI process from any specific vendor.
3. Make it easier to debug and develop locally.

## Building Images

Building images is simple:

```bash
./build.sh
```

## Adding Images

To add an image:

1. Create a folder representing the new name of your image (note the [image types](#image-types) below).
2. Add a `Dockerfile` within that folder.
3. Add your image to the bottom of `build.sh`.

## Image Types

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
