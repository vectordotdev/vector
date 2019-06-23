---
description: Install Vector on the Debian operating system
---

# CentOS

Vector can be installed on the CentOS operating system through the Vector RPM package.

## Install

<!--
NOTE: We cannot use a list below since Gitbook has a formatting bug. Gitbook
cannot properly format lists with code examples. It's one of the many
issues with Gitbook.
-->

Download the [latest version of Vector][releases]:

```bash
curl -o /tmp/vector.rpm https://packages.timber.io/vector/latest/vector-latest-x86_64.rpm
```

Insall the RPM:
   
```bash
rpm -i /tmp/vector.rpm
```

Update the `/etc/vector/vector.toml` configuration file to suit your use use case:

```bash
vi /etc/vector/vector.toml
```

A full configuration spec is located at `/etc/vector/vector.spec.toml` and
the [Configuration Section][configuration] documents and explains all available
options.

Finally, [start](#starting) Vector:

```bash
# CentOS >= 7
sudo systemctl start vector

# CentOS <= 6
sudo service vector start
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
# CentOS >= 7
systemctl kill -s HUP --kill-who=main vector.service

# CentOS <= 6
sudo service vector reload
```

### Starting

```bash
# CentOS >= 7
sudo systemctl start vector

# CentOS <= 6
sudo service vector start
```

### Stopping

```bash
# CentOS >= 7
sudo systemctl stop vector

# CentOS <= 6
sudo service vector stop
```

### Uninstalling

```bash
rpm -e vector
```

### Updating

```bash
rpm -Uvh vector.rpm
```

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