---
description: Install Vector on the Ubuntu operating system
---

# Unbuntu

Vector can be installed on the Ubuntu operating system through the \
`vector.deb` package.

## Install

1. Head over to the [Vector releases][releases] page to download Vector:

    ```bash
    curl -o /tmp/vector.deb https://packages.timber.io/vector/X.X.X/vector-vX.X.X-amd64.deb
    ```

    Replace `X.X.X` with the latest version.

2. Execute:

    ```bash
    dpkg -i /tmp/vector.deb
    ```

3. Update the `/etc/vector/vector.toml` configuration file to suit your use
   use case:

   ```bash
   vi /etc/vector/vector.toml
   ```

   A full configuration spec is located at `/etc/vector/vector.spec.toml`
   and the [Configuration Section] documents and explains all available
   options.

4. [Start](#starting) Vector:

    ```base
    sudo systemctl start vector
    ```

## Administration

### Monitoring

#### Logs

Vector logs are written to `STDOUT` and can be accessed via:

```bash
sudo journalctl -fu vector
```

#### Metrics

Please see the [Metrics section][metrics] in the [Monitoring doc][monitoring].

### Reloading

Reloading is done on-the-fly and does not stop the Vector service.

```bash
systemctl kill -s HUP --kill-who=main vector.service
```

### Starting

```bash
sudo systemctl start vector
```

### Stopping

```bash
sudo systemctl stop vector
```

### Uninstalling

```bash
dpkg –-remove vector
```

### Updating

Simply follow the [install instructions](#install) again with the \
latest `vector.deb` file. Vector will not overwrite your configuration \
file.

## Resources

* [Full administration section][administration]
* [Systemd Docs][systemd]
* [Building from source][build_from_source]


[administration]: /usage/administration/README.md
[build_from_source]: ../build-from-source.md
[metrics]: /usage/administration/monitoring.md#metrics
[monitoring]: /usage/administration/monitoring.md
[releases]: https://github.com/timberio/vector/releases
[systemd]: https://wiki.debian.org/systemd