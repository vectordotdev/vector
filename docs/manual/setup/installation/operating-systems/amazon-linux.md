---
title: Install Vector On Amazon Linux
sidebar_label: Amazon Linux
description: Install Vector on Amazon Linux
---

Amazon Linux is an operating system generally used on AWS. This document
will cover how to install Vector on this operating system.

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
defaultValue={"yum"}
placeholder="Please choose an installation method..."
select={true}
size={"lg"}
values={[{"group":"Package managers","label":"Yum","value":"yum"},{"group":"Package managers","label":"RPM","value":"rpm"},{"group":"Nones","label":"Vector CLI","value":"vector-cli"},{"group":"Platforms","label":"Docker CLI","value":"docker-cli"},{"group":"Platforms","label":"Docker Compose","value":"docker-compose"}]}>
<TabItem value="yum">

<Steps headingDepth={3}>
<ol>
<li>

### Install

Vector can be installed from an [Yum package repository][urls.rpm] which is
generally used on Red Hat, Fedora, and CentOS.

Vector's RPM packages are multi-arch and support the
x86_64 and ARM64
architectures. The architecture name is prepended to the artifact file name.

Vector's RPM packages can be downloaded via our [Cloudsmith][urls.cloudsmith] RPM repository.
Packages are upgraded for each release. You can add this repository to your host automatically using
this script:

```bash
curl -1sLf \
  'https://repositories.timber.io/public/vector/cfg/setup/bash.rpm.sh' \
  | sudo -E bash
```

Or manually like so:

```bash
yum install yum-utils pygpgme
rpm --import 'https://repositories.timber.io/public/vector/cfg/gpg/gpg.3543DB2D0A2BC4B8.key'
curl -1sLf 'https://repositories.timber.io/public/vector/cfg/setup/config.rpm.txt?distro=amzn&codename=2018.03' > /tmp/timber-vector.repo
yum-config-manager --add-repo '/tmp/timber-vector.repo'
yum -q makecache -y --disablerepo='*' --enablerepo='timber-vector'
```

<Alert type="info">

These packages are automatically updated whenever Vector is [released][urls.vector_releases].

</Alert>

</li>
<li>

### Source Files

Vector's RPM source files are located in
[Vector's repo][urls.vector_rpm_source_files].

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
<TabItem value="rpm">

<Steps headingDepth={3}>
<Tabs
  centered={true}
  className="rounded"
  defaultValue="arm64"
  values={[{"label":"ARM64","value":"arm64"},{"label":"x86_64","value":"x86_64"}]}>

<TabItem value="arm64">

1.  ### Download the Vector `.rpm` file

    ```bash
    curl -O https://packages.timber.io/vector/0.10.X/vector-aarch64.rpm
    ```

    [Looking for a specific version?][docs.package_managers.rpm#versions]

2.  ### Install the Vector `.rpm` package directly

    ```bash
    sudo rpm -i vector-aarch64.rpm
    ```

3.  ### Configure Vector

    <ConfigExample
    format="toml"
    path={"/etc/vector/vector.toml"}
    sourceName={"journald"}
    sinkName={null} />

4.  ### Start Vector

    ```bash
    sudo systemctl start vector
    ```

</TabItem>
<TabItem value="x86_64">

1.  ### Download the Vector `.rpm` file

    ```bash
    curl -O https://packages.timber.io/vector/0.10.X/vector-x86_64.rpm
    ```

    [Looking for a specific version?][docs.package_managers.rpm#versions]

2.  ### Install the Vector `.rpm` package directly

    ```bash
    sudo rpm -i vector-x86_64.rpm
    ```

3.  ### Configure Vector

    <ConfigExample
    format="toml"
    path={"/etc/vector/vector.toml"}
    sourceName={"journald"}
    sinkName={null} />

4.  ### Start Vector

    ```bash
    sudo systemctl start vector
    ```

</TabItem>
</Tabs>
</Steps>

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
[docs.package_managers.rpm#versions]: /docs/setup/installation/package-managers/rpm/#versions
[docs.platforms.docker#variants]: /docs/setup/installation/platforms/docker/#variants
[docs.sources.journald]: /docs/reference/sources/journald/
[docs.sources]: /docs/reference/sources/
[docs.strategies#daemon]: /docs/setup/deployment/strategies/#daemon
[urls.cloudsmith]: https://cloudsmith.io/~timber/repos/vector/packages/
[urls.rpm]: https://rpm.org/
[urls.vector_releases]: https://vector.dev/releases/latest/
[urls.vector_rpm_source_files]: https://github.com/timberio/vector/tree/master/distribution/rpm
