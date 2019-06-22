---
description: Install Vector on the Mac operating system
---

# MacOS

## Install

```bash
brew tap timberio/brew && brew install vector
```

## Administration

### Info

```bash
brew info vector
```

### Monitoring

#### Logs

Vector logs are written to `STDOUT`. Vector's launchctl script included
with Homebrew writes logs to `/usr/local/var/log/vector`.

#### Metrics

Please see the [Metrics section][metrics] in the [Monitoring doc][monitoring].

### Reloading

Reloading is done on-the-fly and does not stop the Vector service.

```bash
kill -SIGHUP $(ps -A | grep -m1 vector | awk '{print $1}')
```

### Starting

```bash
brew services start vector
```

### Stopping

```bash
brew services stop vector
```

### Uninstalling

```bash
brew uninstall vector
```

### Updating

```bash
brew update vector
```

## Resources

* [Full administration section][administration]
* [Building from source][build_from_source]


[administration]: /usage/administration/README.md
[build_from_source]: ../build-from-source.md
[metrics]: /usage/administration/monitoring.md#metrics
[monitoring]: /usage/administration/monitoring.md
[releases]: https://github.com/timberio/vector/releases
[systemd]: https://wiki.debian.org/systemd