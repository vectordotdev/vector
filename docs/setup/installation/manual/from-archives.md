---
description: Install Vector from pre-built archives
---

# Install From Archives

{% hint style="info" %}
Before proceeding, please make sure Vector does not support your
[platform][docs.platforms] or [package manager][docs.package_managers]. These
are recommended before installing from archives.
{% endhint %}

## Downloads

Vector provides [pre-built archives][url.releases] for popular target
architectures. If you don't see an architecture, then we recommend
[building Vector from source][docs.from_source].

{% tabs %}
{% tab title="Latest" %}
"Latest" represents the latest [released version][url.releases].

| Architecture | Channel | Notes |
| :------------| :-----: | :---- |
| [`x86_64-apple-darwin`][url.vector_latest_x86_64-apple-darwin] | `latest` | 64-bit OSX (10.7+, Lion+) |
| [`uknown-linux-musl`][url.vector_latest_x86_64-unknown-linux-musl] | `latest` | 64-bit Linux with MUSL. Recommended for Linux. |
| [`uknown-linux-gnu`][url.vector_latest_x86_64-unknown-linux-gnu] | `latest` | 64-bit Linux (2.6.18+) |
| [`armv7-unknown-linux-gnueabihf`][url.vector_latest_armv7-unknown-linux-gnueabihf] ⚠️ | `latest` | ARMv7 Linux |

{% endtab %}
{% tab title="Edge" %}
"Edge" builds are continuous and built after every change merged into
the [`master` repo branch][url.vector_repo].

{% hint style="warning" %}
This release could have bugs or other issues. Please think carefully before
using them over the "latest" alternatives.
{% endhint %}

| Architecture | Channel | Notes |
| :------------| :-----: | :---- |
| [`x86_64-apple-darwin`][url.vector_edge_x86_64-apple-darwin] | `edge` | 64-bit OSX (10.7+, Lion+) |
| [`uknown-linux-musl`][url.vector_edge_x86_64-unknown-linux-musl] | `edge` | 64-bit Linux with MUSL. Recommended for Linux. |
| [`uknown-linux-gnu`][url.vector_edge_x86_64-unknown-linux-gnu] | `edge` | 64-bit Linux (2.6.18+) |
| [`armv7-unknown-linux-gnueabihf`][url.vector_edge_armv7-unknown-linux-gnueabihf] ⚠️ | `edge` | ARMv7 Linux |
{% endtab %}
{% endtabs %}

⚠️ = This release is limited, it does not support on-disk buffers or the [`kafka` sink][docs.kafka_sink]. See the [Limited Releases](#limited-releases) section for more info.


## Installation

Change into the directory you want to install Vector, such as your home dir:

```bash
cd ~
```

Then copy the appropriate download link above and then proceed to download it:

```bash
curl -o <release-download-url> | tar -xzf --directory="vector" --strip-components=1
```

This will create a directory called `vector`. Let's change into that directory:

```bash
cd vector
```

Issuing the `ls` command shows the following directory structure:

```
LICENSE
README.md
bin/vector - The vector binary
config/vector.toml - Default Vector configuration
config/vector.spec.toml - Full Vector configuration specification
config/examples/* - A variety of configuration examples
etc/systemd/vector.service - Systemd service file
etc/init.d/vector - Init.d service file
```

To ensure `vector` is in your `$PATH` let's add it to your profile:

```bash
echo "export PATH=\"$(pwd)/vector/bin:\$PATH\"" >> $HOME/.profile
source $HOME/.profile
```

That's it! You can start vector with:

```bash
vector --config config/vector.toml
```

Proceed to [configure](#configuring) Vector for your use case.


## Configuring

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

## Limited Releases

For certain release targets Vector releases limited builds. These builds lack
the following features:

1. On-disk buffers.
2. The [`kafka` sink][docs.kafka_sink].

The reason these features are not included is due to the libraries required
to power them. Specifically, we use [`leveldb`][url.leveldb] and
[`rdkafka`][url.rdkafka] to power these features. Unforutnately, compiling
and/or statically linking these libraries has proven to be a challenge. This
is something we are working to resolve. You can track progress on
[issue 546][url.issue_661].

## Service Managers

Vector archives ship with service files in case you need them:

### Init.d

To install Vector into Init.d run:

```bash
cp -a etc/init.d/vector /etc/init.d
```

### Systemd

To install Vector into Systemd run:

```bash
cp -a etc/systemd/vector /etc/systemd/system
```

## Updating

Simply follow the same [installation instructions above](#installation).


[docs.configuration]: ../../../usage/configuration
[docs.data_directory]: ../../../usage/configuration/README.md#data-directory
[docs.from_source]: ../../../setup/installation/manual/from-source.md
[docs.kafka_sink]: ../../../usage/configuration/sinks/kafka.md
[docs.package_managers]: ../../../setup/installation/package-managers
[docs.platforms]: ../../../setup/installation/platforms
[url.issue_661]: https://github.com/timberio/vector/issues/661
[url.leveldb]: https://github.com/google/leveldb
[url.rdkafka]: https://github.com/edenhill/librdkafka
[url.releases]: https://github.com/timberio/vector/releases
[url.vector_edge_armv7-unknown-linux-gnueabihf]: https://packages.timber.io/vector/edge/vector-edge-armv7-unknown-linux-gnueabihf.tar.gz
[url.vector_edge_x86_64-apple-darwin]: https://packages.timber.io/vector/edge/vector-edge-x86_64-apple-darwin.tar.gz
[url.vector_edge_x86_64-unknown-linux-gnu]: https://packages.timber.io/vector/edge/vector-edge-x86_64-unknown-linux-gnu.tar.gz
[url.vector_edge_x86_64-unknown-linux-musl]: https://packages.timber.io/vector/edge/vector-edge-x86_64-unknown-linux-musl.tar.gz
[url.vector_latest_armv7-unknown-linux-gnueabihf]: https://packages.timber.io/vector/latest/vector-latest-armv7-unknown-linux-gnueabihf.tar.gz
[url.vector_latest_x86_64-apple-darwin]: https://packages.timber.io/vector/latest/vector-latest-x86_64-apple-darwin.tar.gz
[url.vector_latest_x86_64-unknown-linux-gnu]: https://packages.timber.io/vector/latest/vector-latest-x86_64-unknown-linux-gnu.tar.gz
[url.vector_latest_x86_64-unknown-linux-musl]: https://packages.timber.io/vector/latest/vector-latest-x86_64-unknown-linux-musl.tar.gz
[url.vector_repo]: https://github.com/timberio/vector
