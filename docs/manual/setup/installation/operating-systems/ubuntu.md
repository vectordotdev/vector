---
title: Install Vector On Ubuntu
sidebar_label: Ubuntu
description: Install Vector on the Ubuntu operating system
---

This document will cover installing Vector on Ubuntu.

## Install

<Tabs
block={true}
defaultValue="daemon"
values={[{"label":"As a Daemon","value":"daemon"}]}>
<TabItem value="daemon">

The [daemon deployment strategy][docs.strategies#daemon] is designed for data
collection on a single host. Vector runs in the background, in its own process,
collecting _all_ data for that host.
Typically data is collected from a process manager, such as Journald via
Vector's [`journald` source][docs.sources.journald], but can be collected
through any of Vector's [sources][docs.sources].
The following diagram demonstrates how it works.

<DaemonDiagram
  platformName={null}
  sourceName={null}
  sinkName={null} />

---

<Tabs
centered={false}
className={null}
defaultValue={"apt"}
placeholder="Please choose an installation method..."
select={true}
size={"lg"}
values={[{"group":"Package managers","label":"APT","value":"apt"},{"group":"Package managers","label":"DPKG","value":"dpkg"},{"group":"Nones","label":"Vector CLI","value":"vector-cli"},{"group":"Platforms","label":"Docker CLI","value":"docker-cli"},{"group":"Platforms","label":"Docker Compose","value":"docker-compose"}]}>
<TabItem value="apt">

<Steps headingDepth={3}>
<ol>
<li>

### Install

Vector can be installed from an [Apt package repository][urls.apt] which is
generally used on Debian and Ubuntu.

Vector's DPKG packages are multi-arch and support the
x86_64 and ARM64
architectures. The architecture name is prepended to the artifact file name.

Vector's Deb packages can be downloaded via our [Cloudsmith][urls.cloudsmith] Apt repository.
Packages are upgraded for each release. You can add this repository to your host automatically using
this script:

```bash
curl -1sLf \
  'https://repositories.timber.io/public/vector/cfg/setup/bash.deb.sh' \
  | sudo -E bash
```

Or manually like so:

```bash
apt-get install -y debian-keyring  # debian only
apt-get install -y debian-archive-keyring  # debian only
apt-get install -y apt-transport-https
curl -1sLf 'https://repositories.timber.io/public/vector/cfg/gpg/gpg.3543DB2D0A2BC4B8.key' | apt-key add -
curl -1sLf 'https://repositories.timber.io/public/vector/cfg/setup/config.deb.txt?distro=debian&codename=wheezy' > /etc/apt/sources.list.d/timber-vector.list
apt-get update
```

<Alert type="info">

These packages are automatically updated whenever Vector is [released][urls.vector_releases].

</Alert>

</li>
<li>

### Source Files

Vector's DPKG source files are located in
[Vector's repo][urls.vector_debian_source_files].

</li>
<li>

### Configure Vector

<ConfigExample
format="toml"
path={"/etc/vector/vector.toml"}
sourceName={"journald"}
sinkName={null} />

</li>
<li>

### Start Vector

```bash
sudo systemctl start vector
```

</li>
</ol>
</Steps>

</TabItem>
<TabItem value="dpkg">

<Tabs
centered={true}
className="rounded"
defaultValue="x86_64"
values={[{"label":"x86_64","value":"x86_64"},{"label":"ARM64","value":"arm64"}]}>

<TabItem value="x86_64">
<Steps headingDepth={3}>
<ol>
<li>

### Download the Vector `.deb` package

```bash
curl --proto '=https' --tlsv1.2 -O https://packages.timber.io/vector/0.10.X/vector-amd64.deb
```

[Looking for a different version?][docs.package_managers.dpkg#versions]

</li>
<li>

### Install the downloaded package

```bash
sudo dpkg -i vector-amd64.deb
```

</li>
<li>

### Configure Vector

<ConfigExample
format="toml"
path={"/etc/vector/vector.toml"}
sourceName={"journald"}
sinkName={null} />

</li>
<li>

### Start Vector

```bash
sudo systemctl start vector
```

</li>
</ol>
</Steps>
</TabItem>
<TabItem value="arm64">
<Steps headingDepth={3}>
<ol>
<li>

### Download the Vector `.deb` package

```bash
curl --proto '=https' --tlsv1.2 -O https://packages.timber.io/vector/0.10.X/vector-arm64.deb
```

[Looking for a different version?][docs.package_managers.dpkg#versions]

</li>
<li>

### Install the downloaded package

```bash
sudo dpkg -i vector-arm64.deb
```

</li>
<li>

### Configure Vector

<ConfigExample
format="toml"
path={"/etc/vector/vector.toml"}
sourceName={"journald"}
sinkName={null} />

</li>
<li>

### Start Vector

```bash
sudo systemctl start vector
```

</li>
</ol>
</Steps>
</TabItem>
</Tabs>

</TabItem>
<TabItem value="vector-cli">

<Steps headingDepth={3}>
<ol>
<li>

### Install Vector

<InstallationCommand />

Or choose your [preferred method][docs.installation].

</li>
<li>

### Configure Vector

<ConfigExample
format="toml"
path={"vector.toml"}
sourceName={"journald"}
sinkName={null} />

</li>
<li>

### Start Vector

```bash
vector --config vector.toml
```

That's it! Simple and to the point. Hit `ctrl+c` to exit.

</li>
</ol>
</Steps>

</TabItem>
<TabItem value="docker-cli">

<Steps headingDepth={3}>
<ol>
<li>

### Configure Vector

<ConfigExample
format="toml"
path={"/etc/vector/vector.toml"}
sourceName={"journald"}
sinkName={null} />

</li>
<li>

### Start the Vector container

```bash
docker run \
  -v $PWD/vector.toml:/etc/vector/vector.toml:ro \
  timberio/vector:latest-alpine
```

<CodeExplanation>

- The `-v $PWD/vector.to...` flag passes your custom configuration to Vector.
- The `timberio/vector:latest-alpine` is the default image we've chosen, you are welcome to use [other image variants][docs.platforms.docker#variants].

</CodeExplanation>

That's it! Simple and to the point. Hit `ctrl+c` to exit.

</li>
</ol>
</Steps>

</TabItem>
<TabItem value="docker-compose">

compose!

</TabItem>
</Tabs>
</TabItem>
</Tabs>

[docs.installation]: /docs/setup/installation/
[docs.package_managers.dpkg#versions]: /docs/setup/installation/package-managers/dpkg/#versions
[docs.platforms.docker#variants]: /docs/setup/installation/platforms/docker/#variants
[docs.sources.journald]: /docs/reference/sources/journald/
[docs.sources]: /docs/reference/sources/
[docs.strategies#daemon]: /docs/setup/deployment/strategies/#daemon
[urls.apt]: https://en.wikipedia.org/wiki/APT_(software)
[urls.cloudsmith]: https://cloudsmith.io/~timber/repos/vector/packages/
[urls.vector_debian_source_files]: https://github.com/timberio/vector/tree/master/distribution/debian
[urls.vector_releases]: https://vector.dev/releases/latest/
