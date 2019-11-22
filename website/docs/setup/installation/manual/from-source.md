---
title: Install Vector From Source
sidebar_label: From Source
description: Install Vector from the Vector source code
---

Installing Vector from source should be a last resort if Vector does not
support your [container system][docs.containers], [operating
system][docs.operating_systems], or provide a pre-built
[archive][docs.from_archives]. Because Vector is written in [Rust][urls.rust]
it can compile to a single static binary. You can view an example of this
in the [musl builder Docker image][urls.musl_builder_docker_image].

## Installation

import Alert from '@site/src/components/Alert';

<Alert type="info">

This guide does _not_ cover cross compiling Vector. This guide is intended
to be followed on your target machine.

</Alert>

<div class="section-list section-list--lg">
<div class="section">

### 1. Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
```

</div>
<div class="section">

### 2. Create the `vector` directory

```bash
mkdir vector
```

</div>
<div class="section">

### 3. Download Vector's Source

import Tabs from '@theme/Tabs';

<Tabs
  defaultValue="master"
  values={[
    { label: 'Master', value: 'master', },
    { label: 'Latest', value: 'latest', }
  ]
}>

import TabItem from '@theme/TabItem';

<TabItem value="master">

```bash
mkdir -p vector && curl -sSfL --proto '=https' --tlsv1.2 https://github.com/timberio/vector/archive/master.tar.gz | tar xzf - -C vector --strip-components=1
```

</TabItem>
<TabItem value="latest">

```bash
mkdir -p vector && curl -sSfL --proto '=https' --tlsv1.2 https://github.com/timberio/vector/releases/latest/download/source.tar.gz | tar xzf - -C vector --strip-components=1

```

</TabItem>
</Tabs>
</div>
<div class="section">

### 4. Change into the `vector` directory

```bash
cd vector
```

</div>
<div class="section">

### 5. Compile Vector

```bash
make build
```

The vector binary will be placed in `target/<target>/release/vector`.
For example, if you are building Vector on your Mac, your target triple
is `x86_64-apple-darwin`, and the Vector binary will be located at
`target/x86_64-apple-darwin/release/vector`.

</div>
<div class="section">

### 6. Start Vector

Finally, start vector:

```bash
target/<target>/release/vector --config config/vector.toml
```

</div>
</div>

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

Vector includes service files in case you need them:

#### Init.d

To install Vector into Init.d run:

```bash
cp -av distribution/init.d/vector /etc/init.d
```

#### Systemd

To install Vector into Systemd run:

```bash
cp -av distribution/systemd/vector /etc/systemd/system
```

### Updating

Simply follow the same [installation instructions above](#installation).


[docs.configuration#data-directory]: /docs/setup/configuration#data-directory
[docs.configuration]: /docs/setup/configuration
[docs.containers]: /docs/setup/installation/containers
[docs.from_archives]: /docs/setup/installation/manual/from-archives
[docs.operating_systems]: /docs/setup/installation/operating-systems
[urls.musl_builder_docker_image]: https://github.com/timberio/vector/blob/master/scripts/ci-docker-images/builder-x86_64-unknown-linux-musl/Dockerfile
[urls.rust]: https://www.rust-lang.org/
