---
description: Starting Vector
---

# Starting

This document covers how to properly start Vector.

## Quick Start

Vector can be started by calling the `vector` binary directly, no command is necessary.

```bash
vector --config /etc/vector/vector.toml
```

## Options

| Name | Arg | Description |
| :--- | :---: | :--- |
| **Required** |  |  |
| `-c, --config` | `<path>` | Path the Vector [configuration file](../configuration/). |
| **Optional** |  |  |
| `-r, --require-healthy` | - | Causes vector to immediate exit on startup if any sinks have failing healthchecks. |

## How It Works

### Daemonizing

Vector does not _directly_ offer a way to daemonize the Vector process. We highly recommend that you use a utility like [Systemd](https://www.freedesktop.org/wiki/Software/systemd/) to daemonize and manage your processes.

