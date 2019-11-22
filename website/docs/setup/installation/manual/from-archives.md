---
title: Install Vector From Archives
sidebar_label: From Archives
description: Install Vector from pre-compiled archives
---

Installing Vector from a pre-built archive should be a last resort if Vector
cannot be installed through a supported [container system][docs.containers] or
[operating system][docs.operating_systems]. Archives are built for released
versions as well as nightly builds.

## Installation

import Alert from '@site/src/components/Alert';

<Alert type="info">

If you don't see your target, then we recommend [building Vector from source][docs.from_source].
You can also request a target by [opening an issue][urls.new_target] requesting
your new target.

</Alert>

<div class="section-list section-list--lg">
<div class="section">

### 1. Copy the Vector archive URL

Head over to the [download page][pages.download] and copy the appropriate
archive URL.

</div>
<div class="section">

### 2. Download & unpack the archive

```bash
mkdir -p vector && curl -sSfL --proto '=https' --tlsv1.2 <release-download-url> | tar xzf - -C vector --strip-components=2
```

</div>
<div class="section">

### 3. Change into the `vector` directory

```bash
cd vector
```

</div>
<div class="section">

### 4. Move `vector` into your $PATH

```bash
echo "export PATH=\"$(pwd)/vector/bin:\$PATH\"" >> $HOME/.profile
source $HOME/.profile
```

</div>
<div class="section">

### 5. Start Vector

That's it! You can start vector with:

```bash
vector --config config/vector.toml
```

</div>
</div>

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

```toml
data_dir = "/var/lib/vector"
```

<Alert type="warning">

If you plan to run Vector under a separate user, be sure that the directory
is writable by the `vector` process.

</Alert>

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


[docs.configuration#data-directory]: /docs/setup/configuration#data-directory
[docs.configuration]: /docs/setup/configuration
[docs.containers]: /docs/setup/installation/containers
[docs.from_source]: /docs/setup/installation/manual/from-source
[docs.operating_systems]: /docs/setup/installation/operating-systems
[pages.download]: /download
[urls.new_target]: https://github.com/timberio/vector/issues/new?labels=Type%3A+Task&labels=Domain%3A+Operations
