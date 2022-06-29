---
title: Enrich your observability data
short: Enrichment
description: Learn how to use CSV enrichment to provide more context to your data
author_github: https://github.com/barieom
domain: enriching
weight: 5
tags: ["enrichment", "logs", "level up", "guides", "guide"]
---

{{< requirement >}}
Before you begin, this guide assumes the following:

* You understand the [basic Vector concepts][concepts]
* You understand [how to set up a basic pipeline][pipeline]

[concepts]: /docs/about/concepts
[pipeline]: /docs/setup/quickstart
{{< /requirement >}}


It's important for any organization maintaining observability pipelines to
have flexibility to manipulate their observability data — whether it be appending
better context to an event or triggering alerts when there is a potential threat.
A key component that drives that flexibility is enriching your data from different
sources. Vector now offers initial support for enriching events from external data
sources, which is currently powered by a powerful Vector concept,
[enrichment tables][Enrichment tables]. For now, our support for enrichment
through external data sources is limited to `csv` files, but we're looking to
add support for more data sources.

This guide walks through how you can start enriching your observability by
 exploring two interesting use cases.

## Use Case 1 - IoT Devices

When working with IoT devices, you often want to minimize payloads being
emitted by the devices, even when collecting events from those devices. This
is where [Enrichment tables] comes in very handy. Let's assume that you have an
IoT device that emits its status - `online`, `offline`, `transmitting`, and
`error` states, but the device needs to emit as little payload as possible
due to technical constraints or requirements; instead, the IoT can device can
emit integers — `1` for `online`, `2` for `offline`, etc.

To enrich the IoT device's observability data, let's use a `csv` file
containing the following information:

```csv
status_code,status_message
1,"device status online"
2,"device status offline"
3,"device status connection error"
...
7,"device status transmitting"
8,"device status transmission complete"
9,"device status not responding"
```

To enrich your observability data, you can use ['get_enrichment_table_record'][get_enrichment_table_record].
This function searches your enrichment table for a row that matches the
original field value, replacing that with a new value in that row.
Assuming that your `csv` file is called `iot_status.csv`, the following
illustrates the required Vector configuration:

``` toml
[enrichment_tables.iot_status]
type = "file"

[enrichment_tables.iot_status.file]
path = "/etc/vector/iot_status.csv"
encoding = { type = "csv" }

[enrichment_tables.iot_status.schema]
status_code = "integer"
status_message = "string"
```

After this configuration, we can now translate the output from IoT devices to
human-readable messages that provide further context in our `iot_status.csv`.
To do so, we can make use of the [`get_enrichment_table_record`][get_enrichment_table_record] function.

``` toml
[transforms.enrich_iot_status]
type = "remap"
inputs = ["datadog_agents"]
source = '''
. = parse_json!(.status_message)

status_code = del(.status_code)

# In the case that no row with a matching value is found, the original value of
# the status code is assigned.
row = get_enrichment_table_record("iot_status", {"status_code" : status_code}) ?? status_code

.status = row.status_message
'''
```

Your observability data, assuming it was in JSON, now has been transformed from:

```json
{
  "host":"my.host.com",
  "timestamp":"2019-11-01T21:15:47+00:00",
  ...
  "status_code":1,
}
```

To:

```json
{
  "host":"my.host.com",
  "timestamp":"2019-11-01T21:15:47+00:00",
  ...
  "status":"device status transmission complete",
}
```

While the previous example is relatively straightforward, the second example
use case really shows how powerful event enrichment can be for your
observability pipeline.

## Use Case 2 - Setting Alerts for Suspicious Access

In this example, you are dealing with a system where attempted access from
specific identifiers, such as a blacklisted IP address, must automatically trigger
alerts to relevant personnel on your team. You can use an external database
(though Vector's current solution is limited to `csv` files) that contain
a list of blacklisted IP address and enrich the data so that whatever downstream
log management solution your team may be using, such as
[Datadog's Log Management product], can trigger the alert based on the scrubbed
log.

The key benefit to using `enrichment tables` in this case is that you can
enrich your observability data to trigger an alert from a data source. In cases
where you want to avoid exposing this data source due to its sensitivity,
you can do this entirely on your on-premise infrastructure, rather than uploading
the dataset to a 3rd-party log management solution.

Let's assume that you have a `csv` file containing a list of sensitive IP
addresses that's deemed suspicious for your company for one reason or another whether
it be from your own proprietary list or a 3rd-party source, such as [Emerging Threats][Emerging Threats]
or [FBI InfraGard][FBI InfraGard]. Any IP address that's on the list that attempts to access specific
URL on your service must trigger an alert to your team for further investigation. In
that case, you can set your `csv` file, let's call it  similarly to below:

``` csv
ip,alert_type,severity
"192.0.2.0", "alert", "high"
"198.51.100.0", "alert", "medium"
...
"203.0.113.0", "warn", "medium"
```

Assuming you set the `enrichment_tables` similarly to the configuration in Use
 Case 1 example, the following configuration is all that's necessary:

``` toml
[transforms.ip_alert]
type = "remap"
inputs = ["datadog_agent"]
source = '''

. = parse_json!(.message)

ip = del(.ip)

row = get_enrichment_table_record("ip_info", { "ip" : ip }) ?? ip

.alert.type = row.alert_type
.alert.severity = row.severity

'''
```

Your observability data, assuming it was in JSON, now has been transformed from:

```json
{
  "host":"my.host.com",
  "timestamp":"2019-11-01T21:15:47+00:00",
  ...
  "ip":"192.0.2.0",
}
```

To:

```json
{
  "host":"my.host.com",
  "timestamp":"2019-11-01T21:15:47+00:00",
  ...
  "alert": {
    "type":"alert",
    "severity":"medium"
  }
}
```

These examples are intended to serve as a basic guide on how you can start
leveraging `enrichment tables`. If you are or decide to use `enrichment tables`
for other use cases, let us know on our [Discord chat] or [Twitter], along with
any feedback or request for additional support you have for the Vector team!


[Enrichment tables]: /docs/reference/glossary/#enrichment-tables
[get_enrichment_table_record]: /docs/reference/vrl/functions/#get_enrichment_table_record
[Datadog's Log Management]: https://docs.datadoghq.com/logs/
[find_enrichment_table_records]: /docs/reference/vrl/functions/#find_enrichment_table_records
[example IP source]: https://datatracker.ietf.org/doc/html/rfc5737
[Emerging Threats]: https://rules.emergingthreats.net/
[FBI InfraGard]: https://www.infragard.org/Application/Account/Login
[Discord chat]: https://discord.com/invite/dX3bdkF
[Twitter]: https://twitter.com/vectordotdev
