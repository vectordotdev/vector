---
description: Install Vector from pre-compiled archives
---

# Install From Archives

Installing Vector from a pre-built archive should be a last resort if Vector
cannot be installed through a supported [platform][docs.platforms] or
[operating system][docs.operating_systems]. Archives are built for released
versions as well as nightly builds.

## Installation

### 1. Download The Archive

{% hint style="info" %}
If you don't see an architecture, then we recommend [building Vector from \
source][docs.from_source].
{% endhint %}

{% tabs %}
{% tab title="Releases" %}
Vector retains archives for all [releases][url.releases].

#### Latest

"Latest" archive URLs point to the latest [release][url.releases]:

| Architecture                                                                                 | Notes                                                                            |
|:---------------------------------------------------------------------------------------------|:---------------------------------------------------------------------------------|
| [`latest-x86_64-apple-darwin`][url.vector_latest_release_x86_64-apple-darwin]                        | 64-bit OSX (10.7+, Lion+)                                                        |
| [`latest-x86_64-unknown-linux-musl`][url.vector_latest_release_x86_64-unknown-linux-musl]            | 64-bit Linux with MUSL. Fully static, stripped, and LTO. (Recommended for Linux) |
| [`latest-x86_64-unknown-linux-gnu`][url.vector_latest_release_x86_64-unknown-linux-gnu]              | 64-bit Linux (2.6.18+)                                                           |
| [`latest-armv7-unknown-linux-gnueabihf`][url.vector_latest_release_armv7-unknown-linux-gnueabihf] ⚠️ | ARMv7 Linux                                                                      |

#### Historical

Vector retains historical builds for all releases:

{% hint style="info" %}
Replace `X.X.X` with your preferred [release][url.releases] version.
{% endhint %}

| Architecture                                                                                | Notes                                                                            |
|:--------------------------------------------------------------------------------------------|:---------------------------------------------------------------------------------|
| [`X.X.X-x86_64-apple-darwin`][url.vector_historical_release_x86_64-apple-darwin]                        | 64-bit OSX (10.7+, Lion+)                                                        |
| [`X.X.X-x86_64-unknown-linux-musl`][url.vector_historical_release_x86_64-unknown-linux-musl]            | 64-bit Linux with MUSL. Fully static, stripped, and LTO. (Recommended for Linux) |
| [`X.X.X-x86_64-unknown-linux-gnu`][url.vector_historical_release_x86_64-unknown-linux-gnu]              | 64-bit Linux (2.6.18+)                                                           |
| [`X.X.X-armv7-unknown-linux-gnueabihf`][url.vector_historical_release_armv7-unknown-linux-gnueabihf] ⚠️ | ARMv7 Linux                                                                      |
{% endtab %}
{% tab title="Nightly" %}
"Nightly" builds are built from the [`master` repo branch][url.vector_repo]
every night. They contain the latest features but may be less stable.

#### Latest

| Architecture                                                                                   | Notes                                                                            |
|:-----------------------------------------------------------------------------------------------|:---------------------------------------------------------------------------------|
| [`nightly-x86_64-apple-darwin`][url.vector_latest_nightly_x86_64-apple-darwin]                        | 64-bit OSX (10.7+, Lion+)                                                        |
| [`nightly-x86_64-unknown-linux-musl`][url.vector_latest_nightly_x86_64-unknown-linux-musl]            | 64-bit Linux with MUSL. Fully static, stripped, and LTO. (Recommended for Linux) |
| [`nightly-x86_64-unknown-linux-gnu`][url.vector_latest_nightly_x86_64-unknown-linux-gnu]              | 64-bit Linux (2.6.18+)                                                           |
| [`nightly-armv7-unknown-linux-gnueabihf`][<url class="v"></url>ector_latest_nightly_armv7-unknown-linux-gnueabihf] ⚠️ | ARMv7 Linux                                                                      |

#### Historical

Vector retains all historical builds for nightly releases:

{% hint style="info" %}
Replace `YYYY-MM-DD` with your preferred date.
{% endhint %}

| Architecture                                                                                      | Notes                                                                            |
|:--------------------------------------------------------------------------------------------------|:---------------------------------------------------------------------------------|
| [`YYYY-MM-DD-x86_64-apple-darwin`][url.vector_historical_nightly_x86_64-apple-darwin]                        | 64-bit OSX (10.7+, Lion+)                                                        |
| [`YYYY-MM-DD-x86_64-unknown-linux-musl`][url.vector_historical_nightly_x86_64-unknown-linux-musl]            | 64-bit Linux with MUSL. Fully static, stripped, and LTO. (Recommended for Linux) |
| [`YYYY-MM-DD-x86_64-unknown-linux-gnu`][url.vector_historical_nightly_x86_64-unknown-linux-gnu]              | 64-bit Linux (2.6.18+)                                                           |
| [`YYYY-MM-DD-armv7-unknown-linux-gnueabihf`][url.vector_historical_nightly_armv7-unknown-linux-gnueabihf] ⚠️ | ARMv7 Linux                                                                      |

{% endtab %}
{% tab title="Historical" %}
Vector retains historical builds, both for 
{% endtab %}
{% endtabs %}

⚠️ = This release is limited, it does not support on-disk buffers or the [`kafka` sink][docs.kafka_sink]. See issue [issue 546][url.issue_661] for more details.

### 2. Unpack The Archive

Copy the appropriate download link above and then proceed to download it:

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

### 3. Move vector into your $PATH

To ensure `vector` is in your `$PATH` let's add it to your profile:

```bash
echo "export PATH=\"$(pwd)/vector/bin:\$PATH\"" >> $HOME/.profile
source $HOME/.profile
```

### 4. Start Vector

That's it! You can start vector with:

```bash
vector --config config/vector.toml
```

## Next Steps

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
[url.vector_latest_armv7-unknown-linux-gnueabihf]: https://packages.timber.io/vector/latest/vector-latest-armv7-unknown-linux-gnueabihf.tar.gz
[url.vector_latest_x86_64-apple-darwin]: https://packages.timber.io/vector/latest/vector-latest-x86_64-apple-darwin.tar.gz
[url.vector_latest_x86_64-unknown-linux-gnu]: https://packages.timber.io/vector/latest/vector-latest-x86_64-unknown-linux-gnu.tar.gz
[url.vector_latest_x86_64-unknown-linux-musl]: https://packages.timber.io/vector/latest/vector-latest-x86_64-unknown-linux-musl.tar.gz
[url.vector_nightly_armv7-unknown-linux-gnueabihf]: https://packages.timber.io/vector/nightly/vector-nightly-armv7-unknown-linux-gnueabihf.tar.gz
[url.vector_nightly_x86_64-apple-darwin]: https://packages.timber.io/vector/nightly/vector-nightly-x86_64-apple-darwin.tar.gz
[url.vector_nightly_x86_64-unknown-linux-gnu]: https://packages.timber.io/vector/nightly/vector-nightly-x86_64-unknown-linux-gnu.tar.gz
[url.vector_nightly_x86_64-unknown-linux-musl]: https://packages.timber.io/vector/nightly/vector-nightly-x86_64-unknown-linux-musl.tar.gz
[url.vector_repo]: https://github.com/timberio/vector
