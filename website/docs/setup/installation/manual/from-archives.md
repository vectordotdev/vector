---
title: Install Vector From Archives
sidebar_label: From Archives
description: Install Vector from pre-compiled archives
---

Installing Vector from a pre-built archive should be a last resort if Vector
cannot be installed through a supported [container platform][docs.containers] or
[operating system][docs.operating_systems]. Archives are built for released
versions as well as nightly builds.

## Installation

import Tabs from '@theme/Tabs';

<Tabs
  block={true}
  defaultValue="linux_x86_64"
  values={[
    { label: 'Linux (x86_64)', value: 'linux_x86_64', },
    { label: 'Linux (ARM64)', value: 'linux_arm64', },
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
      curl -sSfL --proto '=https' --tlsv1.2 https://packages.timber.io/vector/latest/vector-x86_64-unknown-linux-musl.tar.gz | \
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
      curl -sSfL --proto '=https' --tlsv1.2 https://packages.timber.io/vector/latest/vector-aarch64-unknown-linux-musl.tar.gz | \
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
    Invoke-WebRequest https://packages.timber.io/vector/latest/vector-x86_64-pc-windows-msvc.zip -OutFile vector-x86_64-pc-windows-msvc.zip
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

We highly recommend creating a [data directory][docs.configuration#data-directory]
that Vector can use:

```
mkdir /var/lib/vector
```

And in your `vector.toml` file:

```toml
data_dir = "/var/lib/vector"
```

import Alert from '@site/src/components/Alert';

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
[urls.new_target]: https://github.com/timberio/vector/issues/new?labels=Type%3A+Task&labels=Domain%3A+Operations
