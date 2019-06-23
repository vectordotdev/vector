---
description: Install Vector from pre-built archives
---

# Install From Archives

{% hint style="info" %}
Before proceeding, please make sure Vector does not support your
[platform][platforms] or [package manager][package_managers]. These are
generally recommended before installing from archives.
{% endhint %}

Vector provides [pre-built archives][releases] for popular target
architectures. If you don't see an architecture, then we recommend
[building Vector from source][from_source].

## Installation

Start by changing into your home directory:

```bash
cd ~
```

Next, copy the appropriate `*.tar.gz` download URL for your environment.
This can be found on the [Vector releases page][releases]. Proceed to download
it:

```bash
curl -o <release-download-url> | tar -xzf
```

This will produce a directory called `vector`. Let's change into
that directory:

```bash
cd vector
```

The `vector` directory has the following structure:

```
$ ls vector
LICENSE
README.md
bin/vector - The vector binary
config/vector.toml - Default Vector configuration
config/vector.spec.toml - Full Vector configuration specification
config/examples/* - A variety of configuration examples
etc/systemd/vector.service - Systemd service file
etc/init.d/vector - Init.d service file
```

You can start vector with:

```bash
bin/vector --config config/vector.toml
```

It works!

To make sure the `vector` binary is available, lets add it to your path:

```bash
export PATH="$(pwd)/bin:$PATH"
```

And finally, you'll want to edit the `config/vector.toml` file to suit
your use case. The [Configuration][configuration] section covers this in
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

Vector archives ship with service files in case you need them:

#### Init.d

To install Vector into Init.d run:

```bash
cp -a etc/init.d/vector /etc/init.d
```

#### Systemd

To install Vector into Systemd run:

```bash
cp -a etc/systemd/vector /etc/systemd/system
```


[configuration]: ../../usage/configuration/README.md
[data_directory]: ../../usage/configuration/README.md#data-directory
[from_source]: from-source.md
[package_managers]: package_managers/README.md
[platforms]: platforms/README.md
[releases]: https://github.com/timberio/vector/releases

