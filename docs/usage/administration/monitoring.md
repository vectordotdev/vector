---
description: Monitoring Vector
---

# Monitoring

This document will cover monitoring Vector.

## Logs

Vector writes all output to `STDOUT`, therefore, you have complete control of
the output destination. Accessing the logs depends on your service manager:

{% tabs %}
{% tab title="Manual" %}
If you are not using a service manager, and you're redirecting Vector's
output to a file then you can use a utility like `tail` to access your logs:

```bash
tail /var/log/vector.log
```
{% endtab %}
{% tab title="Systemd" %}
Tail logs:

```bash
sudo journalctl -fu vector
```
{% endtab %}
{% tab title="Initd" %}
Tail logs:

```bash
tail -f /var/log/vector.log
```
{% endtab %}
{% tab title="Homebrew" %}
Tail logs:

```bash
tail -f /usr/local/var/log/vector.log
```
{% endtab %}
{% endtabs %}

### Levels

By default, Vector logs on the `info` level, you can change the level through
a variety of methods:

| Method | Description |
| :----- | :---------- |
| [`-v` flag][docs.starting.flags] | Drops the log level to `debug`. |
| [`-vv` flag][docs.starting.flags] | Drops the log level to `trace`. |
| [`-q` flag][docs.starting.flags] | Raises the log level to `warn`. |
| [`-qq` flag][docs.starting.flags] | Raises the log level to `error`. |
| `LOG=<level>` env var | Set the log level. Must be one of `trace`, `debug`, `info`, `warn`, `error`. |

### Full Backtraces

You can enable full error backtraces by setting the  `RUST_BACKTRACE=full` env
var. More on this in the [Troubleshooting guide][docs.troubleshooting].

### Rate Limiting

Vector rate limits log events in the hot path. This is to your benefit as
it allows you to get granular insight without the risk of saturating IO
and disrupting service. The tradeoff is that repetitive logs will not be logged.

## Metrics

Currently, Vector does not expose Metrics. [Issue #230][url.issue_230]
represents work to run internal Vector metrics through Vector's pipeline.
Allowing you to define internal metrocs as a [source][docs.sources] and
then define one of many metrics [sinks][docs.sinks] to collect those metrics,
just as you would metrics from any other source.

## Troubleshooting

Please refer to our troubleshooting guide:

{% page-ref page="../usage/guides/troubleshooting.md" %}


[docs.sinks]: ../../usage/configuration/sinks/README.md
[docs.sources]: ../../usage/configuration/sources/README.md
[docs.starting.flags]: ../../usage/administration/starting.md#flags
[docs.troubleshooting]: ../../usage/guides/troubleshooting.md
[url.issue_230]: https://github.com/timberio/vector/issues/230
