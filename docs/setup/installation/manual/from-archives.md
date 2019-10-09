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
If you don't see your target, then we recommend [building Vector from \
source][docs.from_source]. You can also request a target by [opening an \
issue][urls.new_target] requesting your new target.
{% endhint %}

Copy the download URL for the appropriate archive:

* [**Latest release**][urls.vector_downloads.latest]
* [**Latest nightly**][urls.vector_downloads.nightly/latest]
* [**Historical releases**][urls.vector_downloads]

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

We highly recommend creating a [data directory][docs.configuration#data-directory]
that Vector can use:

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

Vector archives ship with service files in case you need them:

#### Init.d

To install Vector into Init.d run:

```bash
cp -av etc/init.d/vector /etc/init.d
```

#### Systemd

To install Vector into Systemd run:

```bash
cp -av etc/systemd/vector /etc/systemd/system
```

### Updating

Simply follow the same [installation instructions above](#installation).


[docs.configuration#data-directory]: ../../../usage/configuration#data-directory
[docs.configuration]: ../../../usage/configuration
[docs.from_source]: ../../../setup/installation/manual/from-source.md
[docs.operating_systems]: ../../../setup/installation/operating-systems
[docs.platforms]: ../../../setup/installation/platforms
[urls.new_target]: https://github.com/timberio/vector/issues/new?labels=Type%3A+Task&labels=Domain%3A+Operations
[urls.vector_downloads.latest]: https://packages.timber.io/vector/latest
[urls.vector_downloads.nightly/latest]: https://packages.timber.io/vector/nightly/latest
[urls.vector_downloads]: https://packages.timber.io/vector
