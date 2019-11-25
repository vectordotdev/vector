---
title: Install Vector From Archives
sidebar_label: From Archives
description: Install Vector from pre-compiled archives
---

This page covers installing Vector from a pre-built archive. These archives
contain the `vector` binary as well as supporting configuration files.

import Alert from '@site/src/components/Alert';

<Alert type="warning">

We recommend installing Vector through a supported [container
platform][docs.containers] or [package manager][docs.package_managers], if
possible. These handle permissions, directory creation, and other
intricacies covered in the [Next Steps](#next-steps) section.

</Alert>

## Installation

import Tabs from '@theme/Tabs';

<Tabs
  block={true}
  defaultValue="linux_x86_64"
  urlKey="os"
  values={[
    { label: 'Linux (x86_64)', value: 'linux_x86_64', },
    { label: 'Linux (ARM64)', value: 'linux_arm64', },
    { label: 'Linux (ARMv7)', value: 'linux_armv7', },
    { label: 'MacOS (x86_64)', value: 'macos_x86_64', },
    { label: 'Windows (x86_64)', value: 'windows_x86_64', },
    { label: 'Other', value: 'other', },
  ]}>

import TabItem from '@theme/TabItem';

<TabItem value="linux_x86_64">

1.  Download & unpack the archive
    
    <Tabs
      className="mini"
      defaultValue="latest"
      values={[
        { label: 'Latest (0.5.0)', value: 'latest'},
        { label: 'Nightly', value: 'nightly'},
      ]}>

    <TabItem value="latest">

    ```bash
    mkdir -p vector && \
      curl -sSfL --proto '=https' --tlsv1.2 https://packages.timber.io/vector/0.5.0/vector-x86_64-unknown-linux-musl.tar.gz | \
      tar xzf - -C vector --strip-components=2
    ```

    </TabItem>
    <TabItem value="nightly">

    ```bash
    mkdir -p vector && \
      curl -sSfL --proto '=https' --tlsv1.2 https://packages.timber.io/vector/nightly/latest/vector-x86_64-unknown-linux-musl.tar.gz | \
      tar xzf - -C vector --strip-components=2
    ```

    </TabItem>
    </Tabs>

2.  Change into the `vector` directory

    ```bash
    cd vector
    ```

3.  Move `vector` into your $PATH

    ```bash
    echo "export PATH=\"$(pwd)/vector/bin:\$PATH\"" >> $HOME/.profile
    source $HOME/.profile
    ```

4.  Start Vector

    That's it! You can start vector with:

    ```bash
    vector --config config/vector.toml
    ```

</TabItem>
<TabItem value="linux_arm64">

1.  Download & unpack the archive
    
    <Tabs
      className="mini"
      defaultValue="latest"
      values={[
        { label: 'Latest (0.5.0)', value: 'latest'},
        { label: 'Nightly', value: 'nightly'},
      ]}>

    <TabItem value="latest">

    ```bash
    mkdir -p vector && \
      curl -sSfL --proto '=https' --tlsv1.2 https://packages.timber.io/vector/0.5.0/vector-aarch64-unknown-linux-musl.tar.gz | \
      tar xzf - -C vector --strip-components=2
    ```

    </TabItem>
    <TabItem value="nightly">

    ```bash
    mkdir -p vector && \
      curl -sSfL --proto '=https' --tlsv1.2 https://packages.timber.io/vector/nightly/latest/vector-aarch64-unknown-linux-musl.tar.gz | \
      tar xzf - -C vector --strip-components=2
    ```

    </TabItem>
    </Tabs>

2.  Change into the `vector` directory

    ```bash
    cd vector
    ```

3.  Move `vector` into your $PATH

    ```bash
    echo "export PATH=\"$(pwd)/vector/bin:\$PATH\"" >> $HOME/.profile
    source $HOME/.profile
    ```

4.  Start Vector

    That's it! You can start vector with:

    ```bash
    vector --config config/vector.toml
    ```

</TabItem>
<TabItem value="linux_armv7">

1.  Download & unpack the archive
    
    <Tabs
      className="mini"
      defaultValue="latest"
      values={[
        { label: 'Latest (0.5.0)', value: 'latest'},
        { label: 'Nightly', value: 'nightly'},
      ]}>

    <TabItem value="latest">

    ```bash
    mkdir -p vector && \
      curl -sSfL --proto '=https' --tlsv1.2 https://packages.timber.io/vector/0.5.0/vector-armv7-unknown-linux-musleabihf.tar.gz | \
      tar xzf - -C vector --strip-components=2
    ```

    </TabItem>
    <TabItem value="nightly">

    ```bash
    mkdir -p vector && \
      curl -sSfL --proto '=https' --tlsv1.2 https://packages.timber.io/vector/nightly/latest/vector-armv7-unknown-linux-musleabihf.tar.gz | \
      tar xzf - -C vector --strip-components=2
    ```

    </TabItem>
    </Tabs>

2.  Change into the `vector` directory

    ```bash
    cd vector
    ```

3.  Move `vector` into your $PATH

    ```bash
    echo "export PATH=\"$(pwd)/vector/bin:\$PATH\"" >> $HOME/.profile
    source $HOME/.profile
    ```

4.  Start Vector

    That's it! You can start vector with:

    ```bash
    vector --config config/vector.toml
    ```

</TabItem>
<TabItem value="macos_x86_64">

1.  Download & unpack the archive
    
    <Tabs
      className="mini"
      defaultValue="latest"
      values={[
        { label: 'Latest (0.5.0)', value: 'latest'},
        { label: 'Nightly', value: 'nightly'},
      ]}>

    <TabItem value="latest">

    ```bash
    mkdir -p vector && \
      curl -sSfL --proto '=https' --tlsv1.2 https://packages.timber.io/vector/0.5.0/vector-x86_64-apple-darwin.tar.gz | \
      tar xzf - -C vector --strip-components=2
    ```

    </TabItem>
    <TabItem value="nightly">

    ```bash
    mkdir -p vector && \
      curl -sSfL --proto '=https' --tlsv1.2 https://packages.timber.io/vector/nightly/latest/vector-x86_64-apple-darwin.tar.gz | \
      tar xzf - -C vector --strip-components=2
    ```

    </TabItem>
    </Tabs>

2.  Change into the `vector` directory

    ```bash
    cd vector
    ```

3.  Move `vector` into your $PATH

    ```bash
    echo "export PATH=\"$(pwd)/vector/bin:\$PATH\"" >> $HOME/.profile
    source $HOME/.profile
    ```

4.  Start Vector

    That's it! You can start vector with:

    ```bash
    vector --config config/vector.toml
    ```

</TabItem>
<TabItem value="windows_x86_64">

1.  Download Vector release archive (latest):

    <Tabs
      className="mini"
      defaultValue="latest"
      values={[
        { label: 'Latest (0.5.0)', value: 'latest'},
        { label: 'Nightly', value: 'nightly'},
      ]}>

    <TabItem value="latest">

    ```powershell
    Invoke-WebRequest https://packages.timber.io/vector/0.5.0/vector-x86_64-pc-windows-msvc.zip -OutFile vector-x86_64-pc-windows-msvc.zip
    ```

    </TabItem>
    <TabItem value="nightly">

    ```powershell
    Invoke-WebRequest https://packages.timber.io/vector/nightly/latest/vector-x86_64-pc-windows-msvc.zip -OutFile vector-x86_64-pc-windows-msvc.zip
    ```

    </TabItem>
    </Tabs>

2.  Extract files from the archive:

    ```powershell
    Expand-Archive vector-x86_64-pc-windows-msvc.zip .
    ```

3.  Navigate to Vector directory:

    ```powershell
    cd vector-x86_64-pc-windows-msvc
    ```
4.  Start Vector:

    ```powerhsell
    bin\vector.exe --config config\vector.toml
    ```

</TabItem>
<TabItem value="other">

To install Vector on targets not listed above we recommend that you [build
Vector from source][docs.from_source]. You can also request a target by
[opening an issue][urls.new_target].

</TabItem>
</Tabs>

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
cp -av etc/systemd/vector.service /etc/systemd/system
```

### Updating

Simply follow the same [installation instructions above](#installation).


[docs.configuration]: /docs/setup/configuration
[docs.containers]: /docs/setup/installation/containers
[docs.from_source]: /docs/setup/installation/manual/from-source
[docs.global-options#data-directory]: /docs/reference/global-options#data-directory
[docs.global-options#data_dir]: /docs/reference/global-options#data_dir
[docs.package_managers]: /docs/setup/installation/package-managers
[urls.new_target]: https://github.com/timberio/vector/issues/new?labels=type%3A+task&labels=domain%3A+operations
