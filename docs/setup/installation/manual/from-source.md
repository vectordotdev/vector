---
description: Install Vector from the Vector source code
---

# Install From Source

Installing Vector from source should be a last resort if Vector does not
support your [platform][docs.platforms],
[operating system][docs.operating_systems], or provide a pre-built
[archive][docs.from_archives]. Because Vector is written in [Rust][url.rust]
it can compile to a single static binary. You can view an example of this
in the [musl builder Docker image][url.musl_builder_docker_image].

## Installation

{% hint style="info" %}
This guide does _not_ cover cross compiling Vector. This guide is intended
to be followed on your target machine.
{% endhint %}

### 1. Install Rust

```bash
curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain stable
```

### 2. Download Vector's Source

Create a directory to unpack the Vector source into:

```bash
mkdir vector
```

Download and unarchive the [Vector source](https://github.com/timberio/vector):

{% code-tabs %}
{% code-tabs-item title="master" %}
```bash
curl -OL https://github.com/timberio/vector/archive/master.tar.gz | tar -xzf - --directory="vector"
```
{% endcode-tabs-item %}
{% code-tabs-item title="latest" %}
```bash
curl -OL https://github.com/timberio/vector/releases/latest/download/source.tar.gz | tar -xzf --directory="vector"
```
{% endcode-tabs-item %}
{% endcode-tabs %}

Change into the `vector` directory:

```bash
cd vector
```

### 3. Compile Vector

And build the project via the `build` Make target:

```bash
make build
```

The vector binary will be placed in `target/<target>/release/vector`.
For example, if you are building Vector on your Mac, your target triple
is `x86_64-apple-darwin`, and the Vector binary will be located at
`target/x86_64-apple-darwin/release/vector`.

### 4. Start Vector

Finally, go ahead and start vector:

```bash
target/<target>/release/vector --config config/vector.toml
```

## Next Steps

### Adding To Your $PATH

You'll most likely want to move the `vector` binary in your `$PATH`, such as
the `/usr/local/bin` folder.

### Configuring

The Vector configuration file is located at:

```
config/vector.toml
```

A full spec is located at `config/vector.spec.toml` and examples are
located in `config/vector/examples/*`. You can learn more about configuring
Vector in the [Configuration][docs.configuration] section.

### Data Directory

We highly recommend creating a [data directory][docs.data_directory] that Vector
can use:

```
mkdir /var/lib/vector
```

And in your `vector.toml` file:

```coffeescript
data_dir = "/var/lib/vector"
```

{% hint style="warning" %}
If you plan to run Vector under a separate user, be sure that the directory
is writable by the `vector` process.
{% endhint %}

### Service Managers

Vector includes service files in case you need them:

#### Init.d

To install Vector into Init.d run:

```bash
cp -a distribution/init.d/vector /etc/init.d
```

#### Systemd

To install Vector into Systemd run:

```bash
cp -a distribution/systemd/vector /etc/systemd/system
```

### Updating

Simply follow the same [installation instructions above](#installation).


[docs.configuration]: ../../../usage/configuration
[docs.data_directory]: ../../../usage/configuration/README.md#data-directory
[docs.from_archives]: ../../../setup/installation/manual/from-archives.md
[docs.operating_systems]: ../../../setup/installation/operating-systems
[docs.platforms]: ../../../setup/installation/platforms
[url.musl_builder_docker_image]: https://github.com/timberio/vector/blob/master/scripts/ci-docker-images/builder-x86_64-unknown-linux-musl/Dockerfile
[url.rust]: https://www.rust-lang.org/
