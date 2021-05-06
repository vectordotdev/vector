---
title: InfluxDB logs
kind: sink
---

## Configuration

{{< component/config >}}

## Telemetry

{{< component/config >}}

## How it works

### Buffers and batches

{{< snippet "buffers-and-batches" >}}

### Health checks

{{< snippet "health-checks" >}}

### Mapping log fields

InfluxDB uses [line protocol][line_protocol] to write data points. It's a text-based format that provides the measurement, tag set, field set, and timestamp of a data point.

A Log Event contains an arbitrary set of fields (key/value pairs) that describe the event.

The following matrix outlines how Log Event fields are mapped onto InfluxDB line protocol:

Field | Line protocol
:-----|:-------------
host | tag
message | field
source_type | tag
timestamp | timestamp
[custom-key] | field

The default behavior can be overridden ussing a [`tags`](#tags) configuration.

#### Mapping example

The following event:

```json
{
  "host": "my.host.com",
  "message": "<13>Feb 13 20:07:26 74794bfb6795 root[8539]: i am foobar",
  "timestamp": "2019-11-01T21:15:47+00:00",
  "custom_field": "custom_value"
}
```

Will be mapped to InfluxDB's line protocol:

```
ns.vector,host=my.host.com,metric_type=logs custom_field="custom_value",message="<13>Feb 13 20:07:26 74794bfb6795 root[8539]: i am foobar" 1572642947000000000
```

### Partitioning

{{< snippet "partitioning" >}}

### Rate limits and adaptive concurrency

{{< snippet "arc" >}}

### Retry policy

{{< snippet "retry-policy" >}}

### State

{{< snippet "stateless" >}}

[line_protocol]: https://v2.docs.influxdata.com/v2.0/reference/syntax/line-protocol/
