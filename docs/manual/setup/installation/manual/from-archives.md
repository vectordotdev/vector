---
title: Install Vector From Archives
sidebar_label: From Archives
description: Install Vector from pre-compiled archives
---

This page covers installing Vector from a pre-built archive. These archives
contain the `vector` binary as well as supporting configuration files.

<Alert type="warning">

We recommend installing Vector through a supported [platform][docs.platforms]
or [package manager][docs.package_managers], if possible. These handle
permissions, directory creation, and other intricacies covered in the
[Next Steps](#next-steps) section.

</Alert>

## Installation

<Tabs
block={true}
defaultValue="aarch64-unknown-linux-musl-tar-gz"
urlKey="file_name"
values={[{"label":"Linux (ARM64)","value":"aarch64-unknown-linux-musl-tar-gz"},{"label":"MacOS (x86_64)","value":"x86_64-apple-darwin-tar-gz"},{"label":"Windows (x86_64, 7+)","value":"x86_64-pc-windows-msvc-zip"},{"label":"Linux (x86_64)","value":"x86_64-unknown-linux-musl-tar-gz"}]}>

<TabItem value="vector-aarch64-unknown-linux-musl-tar-gz">
<Steps headingDepth={3}>

1.  ### Download & unpack the archive

    <Tabs
    className="mini"
    defaultValue="latest"
    values={[
    { label: 'Latest (0.10.0)', value: 'latest'},
    { label: 'Nightly', value: 'nightly'},
    ]}>

    <TabItem value="latest">

    ```bash
    mkdir -p vector && \
      curl -sSfL --proto '=https' --tlsv1.2 https://packages.timber.io/vector/0.10.X/vector-aarch64-unknown-linux-musl.tar.gz | \
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

2.  ### Change into the `vector` directory

    ```bash
    cd vector
    ```

3.  ### Move `vector` into your \$PATH

    ```bash
    echo "export PATH=\"$(pwd)/vector/bin:\$PATH\"" >> $HOME/.profile
    source $HOME/.profile
    ```

4.  ### Start Vector

    That's it! You can start vector with:

    ```bash
    vector --config config/vector.toml
    ```

</Steps>
</TabItem>

<TabItem value="vector-x86_64-apple-darwin-tar-gz">
<Steps headingDepth={3}>

1.  ### Download & unpack the archive

    <Tabs
    className="mini"
    defaultValue="latest"
    values={[
    { label: 'Latest (0.10.0)', value: 'latest'},
    { label: 'Nightly', value: 'nightly'},
    ]}>

    <TabItem value="latest">

    ```bash
    mkdir -p vector && \
      curl -sSfL --proto '=https' --tlsv1.2 https://packages.timber.io/vector/0.10.X/vector-x86_64-apple-darwin.tar.gz | \
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

2.  ### Change into the `vector` directory

    ```bash
    cd vector
    ```

3.  ### Move `vector` into your \$PATH

    ```bash
    echo "export PATH=\"$(pwd)/vector/bin:\$PATH\"" >> $HOME/.profile
    source $HOME/.profile
    ```

4.  ### Start Vector

    That's it! You can start vector with:

    ```bash
    vector --config config/vector.toml
    ```

</Steps>
</TabItem>

<TabItem value="vector-x86_64-pc-windows-msvc-zip">
<Steps headingDepth={3}>

1.  ### Download Vector release archive (latest)

    <Tabs
    className="mini"
    defaultValue="latest"
    values={[
    { label: 'Latest (0.10.0)', value: 'latest'},
    { label: 'Nightly', value: 'nightly'},
    ]}>

    <TabItem value="latest">

    ```bat
    powershell Invoke-WebRequest https://packages.timber.io/vector/0.10.X/vector-x86_64-pc-windows-msvc.zip -OutFile vector-x86_64-pc-windows-msvc.zip
    ```

    </TabItem>
    <TabItem value="nightly">

    ```bat
    powershell Invoke-WebRequest https://packages.timber.io/vector/nightly/latest/vector-x86_64-pc-windows-msvc.zip -OutFile vector-x86_64-pc-windows-msvc.zip
    ```

    </TabItem>
    </Tabs>

2.  ### Extract files from the archive

    ```bat
    powershell Expand-Archive vector-x86_64-pc-windows-msvc.zip .
    ```

3.  ### Navigate to the Vector directory

    ```bat
    cd vector-x86_64-pc-windows-msvc
    ```

4.  ### Start Vector

    ```bat
    .\bin\vector --config config\vector.toml
    ```

</Steps>
</TabItem>

<TabItem value="vector-x86_64-unknown-linux-musl-tar-gz">
<Steps headingDepth={3}>

1.  ### Download & unpack the archive

    <Tabs
    className="mini"
    defaultValue="latest"
    values={[
    { label: 'Latest (0.10.0)', value: 'latest'},
    { label: 'Nightly', value: 'nightly'},
    ]}>

    <TabItem value="latest">

    ```bash
    mkdir -p vector && \
      curl -sSfL --proto '=https' --tlsv1.2 https://packages.timber.io/vector/0.10.X/vector-x86_64-unknown-linux-musl.tar.gz | \
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

2.  ### Change into the `vector` directory

    ```bash
    cd vector
    ```

3.  ### Move `vector` into your \$PATH

    ```bash
    echo "export PATH=\"$(pwd)/vector/bin:\$PATH\"" >> $HOME/.profile
    source $HOME/.profile
    ```

4.  ### Start Vector

    That's it! You can start vector with:

    ```bash
    vector --config config/vector.toml
    ```

</Steps>
</TabItem>
</Tabs>

## Next Steps

### Configuring

The Vector configuration file is located at:

```text
config/vector.toml
```

Example configurations are located in `config/vector/examples/*`. You can learn more about configuring
Vector in the [Configuration][docs.setup.configuration] section.

### Data Directory

We highly recommend creating a [data directory][docs.global-options#data_dir]
that Vector can use:

```bash
mkdir /var/lib/vector
```

<Alert type="warning">

Make sure that this directory is writable by the `vector` process.

</Alert>

Vector offers a global [`data_dir` option][docs.global-options#data_dir] that
you can use to specify the path of your directory.

```toml title="vector.toml"
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

[docs.setup.configuration]: /docs/setup/configuration/
[docs.global-options#data_dir]: /docs/reference/global-options/#data_dir
[docs.package_managers]: /docs/setup/installation/package-managers/
[docs.platforms]: /docs/setup/installation/platforms/
