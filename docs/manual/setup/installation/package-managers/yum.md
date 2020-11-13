---
title: Install Vector via Yum Repository
sidebar_label: Yum Repository
description: Install Vector through an RPM package repository
---

Vector can be installed from an [Yum package repository][urls.rpm] which is
generally used on Red Hat, Fedora, and CentOS.

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
centered={true}
className={"rounded"}
defaultValue={"yum"}
placeholder="Please choose an installation method..."
select={false}
size={null}
values={[{"group":"Package managers","label":"Yum","value":"yum"}]}>
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
</Tabs>
</TabItem>
</Tabs>

## Configuring

The Vector configuration file is placed in:

```text
etc/vector/vector.toml
```

A full spec is located at `/etc/vector/vector.spec.toml` and examples are
located in `/etc/vector/examples/*`. You can learn more about configuring
Vector in the [Configuration][docs.configuration] section.

## Deploying

How you deploy Vector is largely dependent on your use case and environment.
Please see the [deployment section][docs.deployment] for more info on how to
deploy Vector.

## Administering

Vector can be managed through the [Systemd][urls.systemd] service manager:

<Jump to="/docs/administration/">Administration</Jump>

## Uninstalling

Via `yum`:

```bash
sudo yum remove vector
```

## Updating

Follow the [install](#install) steps again, upgrading to the latest version of
Vector.

## Package

### Architectures

Vector's RPM packages are multi-arch and support the
x86_64 and ARM64
architectures. The architecture name is prepended to the artifact file name.

### Versions

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
curl -1sLf 'https://https://repositories.timber.io/public/vector/cfg/setup/config.rpm.txt?distro=el&codename=7' > /tmp/timber-vector.repo
yum-config-manager --add-repo '/tmp/timber-vector.repo'
yum -q makecache -y --disablerepo='*' --enablerepo='timber-vector'
```

You can replace `distro` and `codename` with your specific distribution and version.

<Alert type="info">

These packages are automatically updated whenever Vector is [released][urls.vector_releases].

</Alert>

### Source Files

Vector's RPM source files are located in
[Vector's repo][urls.vector_rpm_source_files].

[docs.configuration]: /docs/setup/configuration/
[docs.deployment]: /docs/setup/deployment/
[docs.sources.journald]: /docs/reference/sources/journald/
[docs.sources]: /docs/reference/sources/
[docs.strategies#daemon]: /docs/setup/deployment/strategies/#daemon
[urls.cloudsmith]: https://cloudsmith.io/~timber/repos/vector/packages/
[urls.rpm]: https://rpm.org/
[urls.systemd]: https://systemd.io/
[urls.vector_releases]: https://vector.dev/releases/latest/
[urls.vector_rpm_source_files]: https://github.com/timberio/vector/tree/master/distribution/rpm
