---
title: Install Vector From Source
sidebar_label: From Source
description: Install Vector from the Vector source code
---

This page covers installing Vector from source. Because Vector is written in
[Rust][urls.rust] it can compile to a single static binary. You can view an
example of this in the [musl builder Docker image][urls.musl_builder_docker_image].

import Alert from '@site/src/components/Alert';

<Alert type="warning">

We recommend installing Vector through a supported [container
platform][docs.containers], [package manager][docs.package_managers], or 
pre-built [archive][docs.from_archives], if possible. These handle permissions,
directory creation, and other intricacies covered in the [Next
Steps](#next-steps) section.

</Alert>

## Installation

import Tabs from '@theme/Tabs';


<Alert type="info">

This guide does _not_ cover cross compiling Vector. This guide is intended
to be followed on your target machine.

</Alert>

1.  Install Rust

    ```bash
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    ```
2. Install C++ toolchain

    Install C and C++ compilers (GCC or Clang) and GNU `make` if they are not pre-installed
    on your system.

3.  Create the `vector` directory

    ```bash
    mkdir vector
    ```

4.  Download Vector's Source
  
    <Tabs
      className="mini"
      defaultValue="latest"
      values={[
        { label: 'Latest (0.5.0)', value: 'latest'},
        { label: 'Master', value: 'master'},
      ]}>

    import TabItem from '@theme/TabItem';

    <TabItem value="latest">

    ```bash
    mkdir -p vector && \
      curl -sSfL --proto '=https' --tlsv1.2 https://github.com/timberio/vector/archive/v0.5.0.tar.gz | \
      tar xzf - -C vector --strip-components=1
    ```

    </TabItem>
    <TabItem value="master">

    ```bash
    mkdir -p vector && \
      curl -sSfL --proto '=https' --tlsv1.2 https://github.com/timberio/vector/archive/master.tar.gz | \
      tar xzf - -C vector --strip-components=1
    ```

    </TabItem>
    </Tabs>

5.  Change into the `vector` directory

    ```bash
    cd vector
    ```

6.  Compile Vector

    ```bash
    make build
    ```

    The vector binary will be placed in `target/<target>/release/vector`.
    For example, if you are building Vector on your Mac, your target triple
    is `x86_64-apple-darwin`, and the Vector binary will be located at
    `target/x86_64-apple-darwin/release/vector`.

7.  Start Vector

    Finally, start vector:

    ```bash
    target/<target>/release/vector --config config/vector.toml
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

We highly recommend creating a [data directory][docs.global-options#data-directory]
that Vector can use:

```
mkdir /var/lib/vector
```

<Alert type="warning">

Make sure that this directory is writable by the `vector` process.

</Alert>

Vector offers a global [`data_dir` option][docs.global-options#data_dir] that
you can use to specify the path of your directory.

import CodeHeader from '@site/src/components/CodeHeader';

<CodeHeader fileName="vector.toml" />

```toml
data_dir = "/var/lib/vector" # default
```

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


[docs.configuration]: /docs/setup/configuration
[docs.containers]: /docs/setup/installation/containers
[docs.from_archives]: /docs/setup/installation/manual/from-archives
[docs.global-options#data-directory]: /docs/reference/global-options#data-directory
[docs.global-options#data_dir]: /docs/reference/global-options#data_dir
[docs.package_managers]: /docs/setup/installation/package-managers
[urls.musl_builder_docker_image]: https://github.com/timberio/vector/blob/master/scripts/ci-docker-images/builder-x86_64-unknown-linux-musl/Dockerfile
[urls.rust]: https://www.rust-lang.org/
