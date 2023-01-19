---
date: "2020-12-01"
title: "0.11 Upgrade Guide"
description: "An upgrade guide that addresses breaking changes in 0.11.0"
authors: ["binarylogic"]
pr_numbers: [3557, 4580, 4557, 4647, 4918, 3297, 3427, 4103]
release: "0.11.0"
hide_on_release_notes: false
badges:
  type: "breaking change"
---

0.11 includes some minor breaking changes:

1. [The metrics emitted by the `internal_metrics` source have changed names.](#second)
1. [The `statsd` sink now supports all socket types.](#third)
1. [The `source_type` field is now explicit in the `splunk_hec` sink.](#fifth)
1. [Remove forwarding to syslog from distributed systemd unit.](#sixth)
1. [The `http` source no longer dedots JSON fields.](#seventh)
1. [The `prometheus` sink has been renamed to `prometheus_exporter`](#first)
1. [The `reduce` transform `identifier_fields` was renamed to `group_by`.](#fourth)

We cover each below to help you upgrade quickly:

## Upgrade Guide

### Breaking: The metrics emitted by the `internal_metrics` source have changed names {#second}

We have not officially announced the `internal_metrics` source (coming in 0.12)
due to the high probability of metric name changes.Since then we've settled on a
[metric naming convention][metric_naming_convention] that is largely inspired by
the [Prometheus naming convention][prometheus_naming_convention]. 0.11 includes
these naming changes.

To upgrade, please see the following:

1. [`internal_metrics` names][internal_metrics_output]
2. [Metric names diff][metric_names_diff]

You'll likely need to update any downstream consumers of this data. We plan to
ship official Vector dashboards in 0.12 that will relieve this maintenance
burden for you in the future.

### Breaking: The `statsd` sink now supports all socket types {#third}

If you're using the [`statsd` sink][statsd_sink] you'll need to add the new
`mode` option that specifies which protocol you'd like to use. Previously, the
only protocol available was UDP.

```diff title="vector.toml"
 [sinks.statsd]
   type = "statsd"
+  mode = "udp"
```

### Breaking: The `source_type` field is now explicit in the `splunk_hec` sink {#fifth}

Previously, the `splunk_hec` sink was using the event's `source_type` field
and mapping that to Splunk's expected `sourcetype` field. Splunk uses this
field to inform parsing and processing of the data. Because this field can
vary depending on your data, we've made the `sourcetype` field an explicit
option:

```diff title="vector.toml"
 [sinks.reduce]
   type = "splunk_hec"
+  sourcetype = "syslog" # only set this if your `message` field is formatted as syslog
```

Only set this field if you want to explicitly inform Splunk of your `message`
field's format. Most users will not want to set this field.

### Breaking: Remove forwarding to syslog from distributed systemd unit {#sixth}

Vector's previous Systemd unit file included configuration that forwarded
Vector's logs over Syslog. This was presumptuous and we've removed these
settings in favor of your system defaults.

If you'd like Vector to continue logging to Syslog, you'll need to add back
the [removed options][removed_systemd_syslog_options], but most users should
not have to do anything.

### Breaking: The `http` source no longer dedots JSON fields {#seventh}

Previously, the `http` source would dedot JSON keys in incoming data. This means
that a JSON payload like this:

```json
{
  "first.second": "value"
}
```

Would turn into this after being ingested into Vector:

```json
{
  "first": {
    "second": "value"
  }
}
```

This is incorrect as Vector should not alter your data in this way. This has
been corrected and your events will keep `.` in their key names.

There is nothing you need to do to upgrade except understand that your data
structure may change if it contained `.` characters in the keys.

### Deprecation: The `prometheus` sink has been renamed to `prometheus_exporter` {#first}

The `prometheus` sink has been renamed to `prometheus_exporter` since 0.11
introduced a new `prometheus_remote_write` sink. This renaming distinguishes
between the two. Upgrading is easy:

```diff title="vector.toml"
[sinks.prometheus]
-  type = "prometheus"
+  type = "prometheus_exporter"
-  namespace = "..."
+  default_namespace = "..."
```

### Deprecation: The `reduce` transform `identifier_fields` was renamed to `group_by` {#fourth}

We renamed the `reduce` transform's `identifier_fields` option to `group_by`
for clarity. We are repositioning this transform to handle broad reduce
operations, such as merging multi-line logs together:

```diff title="vector.toml"
 [sinks.reduce]
   type = "reduce"
-  identifier_fields = ["my_field"]
+  group_by = ["my_field"]
```

[internal_metrics_output]: /docs/reference/configuration/sources/internal_metrics/#output-metrics
[metric_names_diff]: https://github.com/vectordotdev/vector/pull/4647/files
[metric_naming_convention]: https://github.com/vectordotdev/vector/blob/master/CONTRIBUTING.md#metric-naming-convention
[prometheus_naming_convention]: https://prometheus.io/docs/practices/naming/
[removed_systemd_syslog_options]: https://github.com/vectordotdev/vector/pull/3427/files
[statsd_sink]: /docs/reference/configuration/sinks/statsd/
