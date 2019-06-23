---
description: Install Vector through the APT package manager
---

# APT Package Manager

Vector can be installed through the [APT package manager][apt] which is
generally used on Debian and Ubuntu systems.

## Install

{% tabs %}
{% tab title="Quick" %}
The "quick" install option provides a one-liner that adds Timber APT repository:

```bash
curl -s https://packagecloud.io/install/repositories/timberio/packages/script.rpm.sh | sudo bash
```

Now install Vector:

```bash
sudo yum install td-agent-bit-1.0.4-1.x86_64
```
{% endtab %}
{% tab title="Manual" %}
The "manual" install option outlines the individual steps to add the Timber APT
repository:

```bash
curl -s https://packagecloud.io/install/repositories/timberio/packages/script.rpm.sh | sudo bash
```

Install Vector:

```bash
sudo yum install td-agent-bit-1.0.4-1.x86_64
```
{% endtab %}
{% endtabs %}

## Administration

### Configuring

The Vector configuration file is placed in:

```
etc/vector/vector.toml
```

A full spec is located at `/etc/vector/vector.spec.toml` and examples are
located in `/etc/vector/examples/*`. You can learn more about configuring
Vector in the [Configuration][configuration] section.

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
[configuration]: ../build-from-source.md
[metrics]: /usage/administration/monitoring.md#metrics
[monitoring]: /usage/administration/monitoring.md
[releases]: https://github.com/timberio/vector/releases
[systemd]: https://wiki.debian.org/systemd