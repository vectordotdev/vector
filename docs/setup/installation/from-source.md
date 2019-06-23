---
description: Install Vector from the Vector source code
---

# Install From Source

{% hint style="info" %}
Before proceeding, please make sure Vector does not support your
[platform][platforms], [package manager][package_managers], or provide a
[pre-built archive][from_archives]. These are
generally recommended before installing from source.
{% endhint %}

Because Vector is [open source][vector_repo] you can download the code and
compile it from source. Vector is written in [Rust][rust], which means it
compiles to a single static binary. There is no runtime and there are no
dependencies.

## Installation

{% hint style="info" %}
This guide does not cover cross compiling Vector. This guide is intended
to be followed on your target machine.
{% endhint %}

Start by installing Rust:

```bash
curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain stable
```

Download and unarchive the [Vector source](https://github.com/timberio/vector):

{% code-tabs %}
{% code-tabs-item title="master" %}
```bash
curl -o vector.zip https://github.com/timberio/vector/archive/master.zip | tar -xzf
```
{% endcode-tabs-item %}
{% code-tabs-item title="latest release" %}
```bash
curl -o vector.zip https://github.com/timberio/vector/releases/latest/download/source.zip | tar -xzf
```
{% endcode-tabs %}

Change into the `vector` directory:

```bash
cd vector
```

And build the project via the `Makefile`:

```bash
make build
```

The vector binary will be placed in `target/<target>/release/vector`.
For example, if you are building Vector on your Mac, your target triple
is `x86_64-apple-darwin`, and the Vector binary will be located at
`target/x86_64-apple-darwin/release/vector`.

Finally, go ahead and start vector:

```bash
target/<target>/release/vector --config config/vector.toml
```

Vector is ready for your system! You'll most likely want to move this
binary to somewhere in your `$PATH`, such as the `/usr/bin` folder.
Additionally, you'll need to configure the `config/vector.toml` file.
The [Configuration][configuration] section covers this in
great detail.

## Administration

### Configuring

The Vector configuration file is located at:

```
config/vector.toml
```

A full spec is located at `config/vector.spec.toml` and examples are
located in `config/vector/examples/*`. You can learn more about configuring
Vector in the [Configuration][configuration] section.

#### Data Directory

We highly recommend creating a [data directory][data_directory] that Vector
can use:

```
mkdir /var/lib/vector
```

And in your `vector.toml` file:

```toml
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