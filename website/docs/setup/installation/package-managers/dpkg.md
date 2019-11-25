---
title: Install Vector via DPKG
sidebar_label: DPKG
description: Install Vector through the DKG package manager
---

Vector can be installed through the [DPKG package manager][urls.dpkg] which is
generally used on Debian and Ubuntu systems.

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

## Install

1.  Download the [Vector `.deb file`][urls.vector_downloads.0.5.0/vector-amd64.deb]:

    <Tabs
      className="mini"
      defaultValue="latest"
      values={[
        { label: 'Latest (0.5.0)', value: 'latest'},
        { label: 'Nightly', value: 'nightly'},
      ]}>

    <TabItem value="latest">

    ```bash
    curl -O https://packages.timber.io/vector/0.5.0/vector-amd64.deb
    ```

    </TabItem>
    <TabItem value="nightly">

    ```bash
    curl -O https://packages.timber.io/vector/nightly/latest/vector-amd64.deb
    ```

    </TabItem>
    </Tabs>

2.  Install the Vector `.deb` package directly:

    ```bash
    sudo dpkg -i vector-amd64.deb
    ```

3.  Start Vector:

    ```bash
    sudo systemctl start vector
    ```

    That's it! Proceed to [configure](#configuring) Vector for your use case.

### Previous Versions

Historical Vector versions can be found in the [releases][urls.vector_releases].
Once you've found the version you'd like to install you can re-follow the
[install](#install) steps with the URL to the Vector `.deb` file.

## Configuring

The Vector configuration file is placed in:

```
etc/vector/vector.toml
```

A full spec is located at `/etc/vector/vector.spec.toml` and examples are
located in `/etc/vector/examples/*`. You can learn more about configuring
Vector in the [Configuration][docs.configuration] section.

## Administering

Vector can be managed through the [Systemd][urls.systemd] service manager:

import Jump from '@site/src/components/Jump';

<Jump to="/docs/administration">Administration</Jump>

## Uninstalling

```bash
sudo dpkg -r vector
```

## Updating

Follow the [install](#install) steps again, downloading the latest version of
Vector.


[docs.configuration]: /docs/setup/configuration
[urls.dpkg]: https://wiki.debian.org/dpkg
[urls.systemd]: https://www.freedesktop.org/wiki/Software/systemd/
[urls.vector_downloads.0.5.0/vector-amd64.deb]: https://packages.timber.io/vector/0.5.0/vector-amd64.deb
[urls.vector_releases]: https://github.com/timberio/vector/releases
