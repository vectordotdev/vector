---
title: Install Vector from archives
short: From archives
weight: 1
---

This page covers installing Vector from a pre-built archive. These archives contain the vector binary as well as supporting configuration files.

{{< warning >}}
We recommend installing Vector through a supported platform or package manager, if possible. These handle permissions, directory creation, and other intricacies covered in the Next Steps section.
{{< /warning >}}

## Installation

### Linux (ARM64)

Download and unpack the archive:

```shell
# Latest ({{< version >}})
mkdir -p vector && \
  curl -sSfL --proto '=https' --tlsv1.2 https://packages.timber.io/vector/{{< version >}}/vector-{{< version >}}-aarch64-unknown-linux-musl.tar.gz | \
  tar xzf - -C vector --strip-components=2

# Nightly
mkdir -p vector && \
  curl -sSfL --proto '=https' --tlsv1.2 https://packages.timber.io/vector/nightly/latest/vector-nightly-aarch64-unknown-linux-musl.tar.gz | \
  tar xzf - -C vector --strip-components=2
```

Change into the `vector` directory:

```shell
cd vector
```

Move Vector into your `$PATH`:

```shell
echo "export PATH=\"$(pwd)/vector/bin:\$PATH\"" >> $HOME/.profile
source $HOME/.profile
```

Start Vector:

```shell
vector --config config/vector.yaml
```

### Linux (ARMv7)

Download and unpack the archive:

```shell
# Latest ({{< version >}})
mkdir -p vector && \
  curl -sSfL --proto '=https' --tlsv1.2 https://packages.timber.io/vector/{{< version >}}/vector-{{< version >}}-armv7-unknown-linux-gnueabihf.tar.gz | \
  tar xzf - -C vector --strip-components=2

# Nightly
mkdir -p vector && \
  curl -sSfL --proto '=https' --tlsv1.2 https://packages.timber.io/vector/nightly/latest/vector-nightly-armv7-unknown-linux-gnueabihf.tar.gz | \
  tar xzf - -C vector --strip-components=2
```

Change into the `vector` directory:

```shell
cd vector
```

Move Vector into your `$PATH`:

```shell
echo "export PATH=\"$(pwd)/vector/bin:\$PATH\"" >> $HOME/.profile
source $HOME/.profile
```

Start Vector:

```shell
vector --config config/vector.yaml
```

### macoS (x86_64)

Download and unpack the archive:

```shell
# Latest (version {{< version >}})
mkdir -p vector && \
  curl -sSfL --proto '=https' --tlsv1.2 https://packages.timber.io/vector/{{< version >}}/vector-{{< version >}}-x86_64-apple-darwin.tar.gz  | \
  tar xzf - -C vector --strip-components=2

# Nightly
mkdir -p vector && \
  curl -sSfL --proto '=https' --tlsv1.2 https://packages.timber.io/vector/nightly/latest/vector-nightly-x86_64-apple-darwin.tar.gz  | \
  tar xzf - -C vector --strip-components=2
```

Change into the `vector` directory:

```shell
cd vector
```

Move Vector into your `$PATH`:

```shell
echo "export PATH=\"$(pwd)/vector/bin:\$PATH\"" >> $HOME/.profile
source $HOME/.profile
```

Start Vector:

```shell
vector --config config/vector.yaml
```

### Windows (x86_64)

Download the Vector release archive:

```powershell
# Latest (version {{< version >}})
powershell Invoke-WebRequest https://packages.timber.io/vector/{{< version >}}/vector-{{< version >}}-x86_64-pc-windows-msvc.zip -OutFile vector-{{< version >}}-x86_64-pc-windows-msvc.zip


# Nightly
powershell Invoke-WebRequest https://packages.timber.io/vector/0.12.X/vector-nightly-x86_64-pc-windows-msvc.zip -OutFile vector-nightly-x86_64-pc-windows-msvc.zip
```

Extract files from the archive:

```powershell
powershell Expand-Archive vector-nightly-x86_64-pc-windows-msvc.zip .
```

Navigate to the Vector directory:

```powershell
cd vector-nightly-x86_64-pc-windows-msvc
```

Start Vector:

```powershell
.\bin\vector --config config\vector.toml
```

### Linux (x86_64)

Download and unpack the archive:

```shell
# Latest (version {{< version >}})
mkdir -p vector && \
  curl -sSfL --proto '=https' --tlsv1.2 https://packages.timber.io/vector/{{< version >}}/vector-{{< version >}}-x86_64-unknown-linux-musl.tar.gz  | \
  tar xzf - -C vector --strip-components=2

# Nightly
mkdir -p vector && \
  curl -sSfL --proto '=https' --tlsv1.2 https://packages.timber.io/vector/nightly/latest/vector-nightly-x86_64-unknown-linux-musl.tar.gz | \
  tar xzf - -C vector --strip-components=2
```

Change into the `vector` directory:

```shell
cd vector
```

Move Vector into your `$PATH`:

```shell
echo "export PATH=\"$(pwd)/vector/bin:\$PATH\"" >> $HOME/.profile
source $HOME/.profile
```

Start Vector:

```shell
vector --config config/vector.yaml
```

## Next steps

### Configuring

The Vector configuration file is located at:

```shell
config/vector.yaml
```

Example configurations are located in `config/vector/examples/*`. You can learn more about configuring Vector in the [Configuration] documentation.

### Data directory

We recommend creating a [data directory][data_dir] that Vector can use:

```shell
mkdir /var/lib/vector
```

{{< warning >}}
Make sure that this directory is writable by the `vector` process.
{{< /warning >}}

Vector offers a global [`data_dir` option][data_dir] that you can use to specify the path of your directory:

```shell
data_dir = "/var/lib/vector" # default
```

### Service managers

Vector archives ship with service files in case you need them:

#### Init.d

To install Vector into init.d, run:

```shell
cp -av etc/init.d/vector /etc/init.d
```

#### Systemd

To install Vector into Systemd, run:

```shell
cp -av etc/systemd/vector.service /etc/systemd/system
```

### Updating

To update Vector, follow the same [installation](#installation) instructions above.

[configuration]: /docs/reference/configuration
[data_dir]: /docs/reference/configuration/global-options/#data_dir
