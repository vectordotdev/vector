---
title: Install Vector from source
short: From source
weight: 2
---

This page covers installing Vector from source using the native toolchain for the host.

Vector can also be compiled to a static binary for Linux for x86_64, ARM64, and ARMv7 architectures. See [compiling using Docker][docker] for details.

{{< warning >}}
We recommend installing Vector through a supported platform, package manager, or pre-built archive if possible. These handle permissions, directory creation, and other intricacies covered in the [Next Steps](#next-steps) section.
{{< /warning >}}

[docker]: /docs/setup/installation/manual/from-source/#docker

## Installation

### Linux

Install compilation dependencies for your distribution, if they aren't pre-installed on your system:

```shell
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable

# Install protoc
./scripts/environment/install-protoc.sh

# Install other dependencies, example for Ubuntu/Debian:
sudo apt-get update
sudo apt-get install -y build-essential cmake curl git
```

Clone Vector's source:

```shell
git clone https://github.com/vectordotdev/vector
cd vector

# (Optional) Check out a specific version
# git checkout v0.51.1

# Or use the latest release tag
# git checkout $(git describe --tags --abbrev=0)
```

Compile and run Vector:

```shell
make build

# Or specify with custom features
# FEATURES="<flag1>,<flag2>,..." make build

# Run your custom build
target/release/vector --config config/vector.yaml
```

The `FEATURES` environment variable is optional. You can override the default features using this variable.
See [feature flags](#feature-flags) for more info.

### Windows

Install Rust using [`rustup`][rustup]. If you don't have VC++ build tools, the install will prompt you to install them.

Install and add [CMake][cmake] to `PATH`.

Install and add [Protoc][protoc] to `PATH`.

Install [Perl for Windows][perl].

Add Perl to your `PATH`. In a Rust/MSVC environment (for example using `x64 Native Tools Command Prompt`) add the binary directory of Perl installed on the previous step to `PATH`. For example, for default installation of Strawberry Perl it is

```powershell
set PATH=%PATH%;C:\Strawberry\perl\bin
```

Get Vector's source using Git:

```shell
# Latest
git clone https://github.com/vectordotdev/vector
git checkout v{{< version >}}
cd vector

# Master
git clone https://github.com/vectordotdev/vector
cd vector
```

Build Vector in release mode:

```shell
set RUSTFLAGS=-Ctarget-feature=+crt-static
cargo build --no-default-features --features default-msvc --release
```

Start Vector. After these steps, a binary `vector.exe` in `target\release` would be created. It can be started by running:

```powershell
.\target\release\vector --config config\vector.yaml
```

### Docker

You can build statically linked binaries of Vector for Linux using [cross][] in Docker. If you do so, the dependencies listed in the
previous section aren't needed, as all of them would be automatically pulled by Docker.

First, clone Vector's source:

```shell
git clone https://github.com/vectordotdev/vector
cd vector

# (Optional) Check out a specific version
# git checkout v{{< version >}}

# Alternative: Download tarball
# mkdir -p vector && \
#   curl -sSfL --proto '=https' --tlsv1.2 https://api.github.com/repos/vectordotdev/vector/tarball/v{{< version >}} | \
#   tar xzf - -C vector --strip-components=1 && cd vector
```

Second, [install cross][cross].

Then build Vector using cross for your target architecture:

```shell
# Linux x86_64 (musl - fully static)
make package-x86_64-unknown-linux-musl-all

# Linux x86_64 (glibc - standard)
make package-x86_64-unknown-linux-gnu-all

# Linux ARM64 (musl)
make package-aarch64-unknown-linux-musl-all

# Linux ARM64 (glibc)
make package-aarch64-unknown-linux-gnu-all

# Linux ARMv7
make package-armv7-unknown-linux-musleabihf-all
```

These commands build a Docker image with a Rust toolchain for the target architecture, start a container from this image, and build Vector
inside the container. The musl targets create fully static binaries, while gnu targets link against glibc.

The compiled packages will be located in `target/artifacts/`.

#### Building Custom Docker Images

You can build custom Docker images with Vector. The repository includes Dockerfiles for different base images in the `distribution/docker/`
directory.

**Using the Alpine Dockerfile (smallest image, musl-based):**

```shell
# First build the musl binary
make package-x86_64-unknown-linux-musl-all

# Then build the Docker image
cd distribution/docker/alpine
docker build -t my-vector:alpine .
```

**Using the Debian Dockerfile (glibc-based):**

```shell
# First build the deb package
make package-x86_64-unknown-linux-gnu-all

# Then build the Docker image
cd distribution/docker/debian
docker build -t my-vector:debian .
```

## Next steps

### Configuring

The Vector configuration file is located at:

```shell
config/vector.yaml
```

Example configurations are located in `config/vector/examples/*`. You can learn more about configuring Vector in the [Configuration] documentation.

### Data directory

We recommend creating a [data directory][data_dir] that Vector can use:

```shell
mkdir /var/lib/vector
```

{{< warning >}}
Make sure that this directory is writable by the `vector` process.
{{< /warning >}}

Vector offers a global [`data_dir` option][data_dir] that you can use to specify the path of your directory:

```shell
data_dir = "/var/lib/vector" # default
```

### Service managers

Vector archives ship with service files in case you need them:

#### Init.d

To install Vector into Init.d, run:

```shell
cp -av etc/init.d/vector /etc/init.d
```

#### Systemd

To install Vector into Systemd, run:

```shell
cp -av etc/systemd/vector.service /etc/systemd/system
```

### Updating

To update Vector, follow the same [installation](#installation) instructions above.

## How it works

### Feature flags

Vector supports many feature flags to customize which features are included in a build. By default,
all sources, transforms, and sinks are enabled. To view a complete list of features, they are listed
under "[features]" [here](https://github.com/vectordotdev/vector/blob/master/Cargo.toml).

Example of building with only specific components:

```shell
# Build with only file source, remap transform, and console sink
FEATURES="api,sources-file,transforms-remap,sinks-console" make build
```

[buffer]: /docs/reference/glossary/#buffer
[cmake]: https://cmake.org/
[configuration]: /docs/reference/configuration
[cross]: https://github.com/rust-embedded/cross
[data_dir]: /docs/reference/configuration/global-options/#data_dir
[docker_logs]: /docs/reference/configuration/sources/docker_logs
[jemalloc]: https://github.com/jemalloc/jemalloc
[kafka_sink]: /docs/reference/configuration/sinks/kafka
[kafka_source]: /docs/reference/configuration/sources/kafka
[librdkafka]: https://github.com/edenhill/librdkafka
[openssl]: https://www.openssl.org
[perl]: https://www.perl.org/get.html#win32
[protoc]: https://github.com/protocolbuffers/protobuf
[rustup]: https://rustup.rs
[zlib]: https://www.zlib.net
