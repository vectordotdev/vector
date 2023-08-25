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

Install Rust:

```shell
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
```

Install compilation dependencies, specifically C and C++ compilers (GCC or Clang) and GNU `make` if they aren't pre-installed on your system.

Download Vector's source:

```shell
# Latest ({{< version >}})
mkdir -p vector && \
  curl -sSfL --proto '=https' --tlsv1.2 https://api.github.com/repos/vectordotdev/vector/tarball/v{{< version >}} | \
  tar xzf - -C vector --strip-components=1

# Master
mkdir -p vector && \
  curl -sSfL --proto '=https' --tlsv1.2 https://github.com/vectordotdev/vector/archive/master.tar.gz | \
  tar xzf - -C vector --strip-components=1
```

Change into your Vector directory:

```shell
cd vector
```

Compile Vector:

```shell
[FEATURES="<flag1>,<flag2>,..."] make build
```

The `FEATURES` environment variable is optional. You can override the default features using this variable. See [feature flags](#feature-flags) for more info.

When finished, the Vector binary is placed in `target/<target>/release/vector`. If you're building Vector on your Mac, for example, the target triple is `x86_64-apple-darwin` and the Vector binary will be located at `target/x86_64-apple-darwin/release/vector`.

Finally, you can start Vector:

```shell
target/<target>/release/vector --config config/vector.yaml
```

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
.\target\release\vector --config config\vector.toml
```

### Docker

You can build statically linked binaries of Vector for Linux using [cross][] in Docker. If you do so, the dependencies listed in the previous section aren't needed, as all of them would be automatically pulled by Docker.

First, download Vector's source:

```shell
# Latest ({{< version >}})
mkdir -p vector && \
  curl -sSfL --proto '=https' --tlsv1.2 https://api.github.com/repos/vectordotdev/vector/tarball/v{{< version >}} | \
  tar xzf - -C vector --strip-components=1

# Master
mkdir -p vector && \
  curl -sSfL --proto '=https' --tlsv1.2 https://github.com/vectordotdev/vector/archive/master.tar.gz | \
  tar xzf - -C vector --strip-components=1
```

Second, [install cross][cross].

And then build Vector using [cross]:

```shell
# Linux (x86_64)
make package-x86_64-unknown-linux-musl-all

# Linux (ARM64)
make package-aarch64-unknown-linux-musl-all

# Linux (ARMv7)
make package-armv7-unknown-linux-muslueabihf-all
```

The command above builds a Docker image with a Rust toolchain for a Linux target for the corresponding architecture using `musl` as the C library, then starts a container from this image, and then builds inside the container. The target binary is located at `target/<target triple>/release/vector` as in the previous case.

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
