---
title: Monitoring and observing
short: Monitoring
weight: 2
---

Vector strives to be a good example of observability and therefore includes various facilities to observe and monitor Vector itself. It is intentionally designed to be composable and fit into your existing workflows.

## Logs

Vector is built to provide clear, informative, well-structured logs. In this section we'll show you how to access and route them.

### Accessing logs

The preferred way to access Vector's logs is based on your [installation method][install].

#### APT

The Vector package from the APT repository installs Vector as a Systemd service. Logs can be accessed through the `journalctl` utility:

```bash
sudo journalctl -fu vector
```

#### dpkg

The Vector DEB package installs Vector as a Systemd service. Logs can be accessed through the `journalctl` utility:

```bash
sudo journalctl -fu vector
```

#### Docker

If you've started Vector through the `docker` CLI you can access Vector's logs via the `docker logs` command. First, find the Vector container ID:

```bash
docker ps | grep vector
```

Copy Vector's container ID and use it to tail the logs:

```bash
docker logs -f <container-id>
```

#### Docker Compose

If you started Vector through Docker compose you can use the following command to access Vector's logs:

```bash
docker-compose logs -f vector
```

Replace `vector` with the name of Vector's service if it isn't named `vector`.

#### Homebrew

When Vector is started through Homebrew the logs are automatically routed to `/usr/local/var/log/vector.log`. You can tail them with the `tail` utility:

```bash
tail -f /usr/local/var/log/vector.log
```

#### MSI

The Vector MSI package doesn't install Vector into a proces manager. Therefore, Vector must be started by executing the Vector binary directly. Vector's logs are written to `STDOUT`. You are in charge of routing `STDOUT`, and this determines how you access Vector's logs.

#### Nix

The Vector Nix package doesn't install Vector into a proces manager. Therefore, Vector must be started by executing the Vector binary directly. Vector's logs are written to `STDOUT`. You are in charge of routing `STDOUT`, and this determines how you access Vector's logs.

#### RPM

The Vector RPM package installs Vector as a Systemd service. Logs can be
accessed through the `journalctl` utility:

```bash
sudo journalctl -fu vector
```

### Routing logs

By default, Vector emits its logs over `STDOUT`. This allows you to redirect logs through system-level utilities like any other service. If you're using a process manager like Systemd, logs should be collected for you and made available through utilities like [Journald]. This means that you can collect Vector's logs like other logs on your host. In the case of Systemd/Journald, you can use Vector's [`journald` source][journald_source]:

```toml
[sources.vector_logs]
type = "journald"
include_units = ["vector"]
```

### Configuring logs

#### Levels

Log levels can be adjusted when [starting] Vector via the following methods:

Method | Description
:------|:-----------
`-v` flag | Drops the log level to `debug`
`-vv` flag | Drops the log level to `trace`
`-q` flag | Raises the log level to `warn`
`-qq` flag | Raises the log level to `error`
`-qqq` flag | Disables logging
`LOG=<level>` environment variable | Set the log level. Must be one of `trace`, `debug`, `info`, `warn`, `error`, `off`.

#### Stack traces

You can enable full error backtraces by setting the `RUST_BACKTRACE=full` environment variable. More on this in the [Troubleshooting guide][troubleshooting].

## Metrics



## Troubleshooting

More information in our troubleshooting guide:

{{< jump "/guides/level-up/troubleshooting" >}}

## How it works

### Event-driven observability

Vector employs an event-driven observability strategy that ensures consistent and correlated telemetry data. You can read more about our approach in [RFC 2064][rfc_2064].

### Log rate limiting

Vector rate limits log events in the hot path. This enables you to get granular insight without the risk of saturating IO and disrupting the service. The trade-off is that repetitive logs aren't logged.

[install]: /docs/setup/installation
[journald]: https://www.freedesktop.org/software/systemd/man/systemd-journald.service.html
[journald_source]: /docs/reference/configuration/sources/journald
[rfc_2064]: https://github.com/timberio/vector/blob/master/rfcs/2020-03-17-2064-event-driven-observability.md
[starting]: /docs/administration/process-management/#start
[troubleshooting]: /guides/level-up/troubleshooting/
