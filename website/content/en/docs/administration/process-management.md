---
title: Process management
short: Management
weight: 1
---

This document shows you how to manage the Vector process using various platforms. *How* you manage the Vector process is largely dependent on how you installed it.

## Administrate

{{< administrate >}}

### Health checks

By default, Vector performs health checks on all components. Health checks ensure that the downstream service is accessible and ready to accept data. This is check is perform when the component is initialized. If the check fails, an error is logged and Vector proceeds to start.

**Require health checks**

To make Vector immediately exit when any health check fails, pass the `--require-healthy` flag when starting Vector:

```bash
vector --config /etc/vector/vector.toml --require-healthy
```

**Disable health checks**

To disable health checks, set the `healthcheck` option to `false` for each component:

```toml
[sinks.my-sink]
type = "..."
healthcheck = false
```

##### Options

See the [`start` command CLI reference][start] for a comprehensive list of command line flags and options.

#### Stop

```bash
sudo systemctl stop vector
```

##### Graceful shutdown

Vector is designed to gracefully shut down within 60 seconds when a `SIGTERM` process signal is received. Vector prints the shutdown status every 10 seconds so that you remain informed of the process. The graceful shutdown process proceeds like this:

1. Stop accepting new data for all [sources]
1. Close open connections
1. Flush sink buffers
1. Exist the process with a `0` exit code

##### Force killing

Please note that Vector can lose in-flight data if it's forcefully killed. If Vector fails to shut down gracefully please [report it as a bug][bug].

#### Reload

```bash
systemctl kill -s HUP --kill-who=main vector.service
```

##### Automatic reload on changes

You can automatically reload Vector's configuration file when it changes by using the `-w` or `--watch-config` flag when starting Vector. This should be used with caution since it can sometimes cause surprise behavior. When possible, we recommend issuing a manual reload after you've changed configuration.

##### Configuration errors

When Vector is reloaded it reads the new configuration file from disk. If the file has errors it will be logged to `STDOUT` and ignored, preserving any previous configuration that was set. If the process exits you will not be able to restart the process since it will try to use the new, invalid, configuration file.

##### Graceful pipeline transitioning

Vector will perform a diff between the new and old topology to determine which changes need to be made. Deleted components will be shut down first, ensuring there are no resource conflicts with new components, and then new components will be started.

##### Global options

Global options can't be changed with a reload. Instead, Vector can be restarted with new configuration file.

#### Restart

```bash
sudo systemctl restart vector
```

Restarting is the equivalent to fully stopping and starting the Vector process. When possible, we recommend [reloading](#reload) Vector instead, since it will minimize downtime and disruptions.

#### Observe

To observe logs:

```bash
sudo journalctl -fu vector
```

To observe metrics:

```bash
vector top
```

### macOS

### Windows

## How it works

### Signals

Signal | Description
:------|:-----------
`SIGTERM` | Initiates a graceful shutdown process
`SIGHUP` | Reloads configuration on the fly

### Exit codes

A full list of exit codes can be found in the [`exitcodes` Rust crate][exitcodes]. The codes that Vector uses:

Code | Description
:----|:-----------
`0` | No error
`1` | Exited with error
`78` | Bad [configuration]

[bug]: https://github.com/timberio/vector/issues/new?labels=type%3A+bug
[configuration]: /docs/reference/configuration
[exitcodes]: https://docs.rs/exitcode/latest/exitcode/#constants
[sources]: /docs/reference/configuration/sources
[start]: /docs/reference/cli/#start
