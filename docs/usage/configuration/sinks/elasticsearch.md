---
description: Batches `log` events to Elasticsearch via the `_bulk` API endpoint.
---

<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/sinks/elasticsearch.md.erb
-->

# elasticsearch sink

![][images.elasticsearch_sink]

{% hint style="warning" %}
The `elasticsearch` sink is in beta. Please see the current
[enhancements][url.elasticsearch_sink_enhancements] and
[bugs][url.elasticsearch_sink_bugs] for known issues.
We kindly ask that you [add any missing issues][url.new_elasticsearch_sink_issues]
as it will help shape the roadmap of this component.
{% endhint %}

The `elasticsearch` sink batches [`log`][docs.log_event] events to [Elasticsearch][url.elasticsearch] via the [`_bulk` API endpoint](https://www.elastic.co/guide/en/elasticsearch/reference/current/docs-bulk.html).

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```coffeescript
[sinks.my_elasticsearch_sink_id]
  # REQUIRED - General
  type = "elasticsearch" # must be: "elasticsearch"
  inputs = ["my-source-id"]
  host = "http://10.24.32.122:9000"
  
  # OPTIONAL - General
  doc_type = "_doc" # default
  index = "vector-%F" # default
  
  # OPTIONAL - Batching
  batch_size = 10490000 # default, bytes
  batch_timeout = 1 # default, bytes
  
  # OPTIONAL - Requests
  rate_limit_duration = 1 # default, seconds
  rate_limit_num = 5 # default
  request_in_flight_limit = 5 # default
  request_timeout_secs = 60 # default, seconds
  retry_attempts = 5 # default
  retry_backoff_secs = 5 # default, seconds
  
  # OPTIONAL - Buffer
  [sinks.my_elasticsearch_sink_id.buffer]
    # OPTIONAL
    type = "memory" # default, enum: "memory", "disk"
    when_full = "block" # default, enum: "block", "drop_newest"
    max_size = 104900000 # no default
    num_items = 500 # default
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (schema)" %}
```coffeescript
[sinks.<sink-id>]
  # REQUIRED - General
  type = "elasticsearch"
  inputs = ["<string>", ...]
  host = "<string>"

  # OPTIONAL - General
  doc_type = "<string>"
  index = "<string>"

  # OPTIONAL - Batching
  batch_size = <int>
  batch_timeout = <int>

  # OPTIONAL - Requests
  rate_limit_duration = <int>
  rate_limit_num = <int>
  request_in_flight_limit = <int>
  request_timeout_secs = <int>
  retry_attempts = <int>
  retry_backoff_secs = <int>

  # OPTIONAL - Buffer
  [sinks.<sink-id>.buffer]
    type = {"memory" | "disk"}
    when_full = {"block" | "drop_newest"}
    max_size = <int>
    num_items = <int>
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (specification)" %}
```coffeescript
[sinks.elasticsearch]
  #
  # General
  #

  # The component type
  # 
  # * required
  # * no default
  # * must be: "elasticsearch"
  type = "elasticsearch"

  # A list of upstream source or transform IDs. See Config Composition for more
  # info.
  # 
  # * required
  # * no default
  inputs = ["my-source-id"]

  # The host of your Elasticsearch cluster. This should be the full URL as shown
  # in the example.
  # 
  # * required
  # * no default
  host = "http://10.24.32.122:9000"

  # The `doc_type` for your index data. This is only relevant for Elasticsearch
  # <= 6.X. If you are using >= 7.0 you do not need to set this option since
  # Elasticsearch has removed it.
  # 
  # * optional
  # * default: "_doc"
  doc_type = "_doc"

  # Index name to write events to. `strftime` specifiers are supported.
  # 
  # * optional
  # * default: "vector-%F"
  index = "vector-%F"

  #
  # Batching
  #

  # The maximum size of a batch before it is flushed.
  # 
  # * optional
  # * default: 10490000
  # * unit: bytes
  batch_size = 10490000

  # The maximum age of a batch before it is flushed.
  # 
  # * optional
  # * default: 1
  # * unit: bytes
  batch_timeout = 1

  #
  # Requests
  #

  # The window used for the `request_rate_limit_num` option
  # 
  # * optional
  # * default: 1
  # * unit: seconds
  rate_limit_duration = 1

  # The maximum number of requests allowed within the `rate_limit_duration`
  # window.
  # 
  # * optional
  # * default: 5
  rate_limit_num = 5

  # The maximum number of in-flight requests allowed at any given time.
  # 
  # * optional
  # * default: 5
  request_in_flight_limit = 5

  # The maximum time a request can take before being aborted.
  # 
  # * optional
  # * default: 60
  # * unit: seconds
  request_timeout_secs = 60

  # The maximum number of retries to make for failed requests.
  # 
  # * optional
  # * default: 5
  retry_attempts = 5

  # The amount of time to wait before attempting a failed request again.
  # 
  # * optional
  # * default: 5
  # * unit: seconds
  retry_backoff_secs = 5

  #
  # Buffer
  #

  [sinks.elasticsearch.buffer]
    # The buffer's type / location. `disk` buffers are persistent and will be
    # retained between restarts.
    # 
    # * optional
    # * default: "memory"
    # * enum: "memory", "disk"
    type = "memory"
    type = "disk"

    # The behavior when the buffer becomes full.
    # 
    # * optional
    # * default: "block"
    # * enum: "block", "drop_newest"
    when_full = "block"
    when_full = "drop_newest"

    # Only relevant when `type` is `disk`. The maximum size of the buffer on the
    # disk.
    # 
    # * optional
    # * no default
    max_size = 104900000

    # Only relevant when `type` is `memory`. The maximum number of events allowed
    # in the buffer.
    # 
    # * optional
    # * default: 500
    num_items = 500
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Options

| Key  | Type  | Description |
|:-----|:-----:|:------------|
| **REQUIRED** - General | | |
| `type` | `string` | The component type<br />`required` `enum: "elasticsearch"` |
| `inputs` | `[string]` | A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.config_composition] for more info.<br />`required` `example: ["my-source-id"]` |
| `host` | `string` | The host of your Elasticsearch cluster. This should be the full URL as shown in the example.<br />`required` `example: "http://10.24.32.122:9000"` |
| **OPTIONAL** - General | | |
| `doc_type` | `string` | The `doc_type` for your index data. This is only relevant for Elasticsearch <= 6.X. If you are using >= 7.0 you do not need to set this option since Elasticsearch has removed it.<br />`default: "_doc"` |
| `index` | `string` | Index name to write events to. [`strftime` specifiers][url.strftime_specifiers] are supported.<br />`default: "vector-%F"` |
| **OPTIONAL** - Batching | | |
| `batch_size` | `int` | The maximum size of a batch before it is flushed.<br />`default: 10490000` `unit: bytes` |
| `batch_timeout` | `int` | The maximum age of a batch before it is flushed.<br />`default: 1` `unit: bytes` |
| **OPTIONAL** - Requests | | |
| `rate_limit_duration` | `int` | The window used for the `request_rate_limit_num` option<br />`default: 1` `unit: seconds` |
| `rate_limit_num` | `int` | The maximum number of requests allowed within the `rate_limit_duration` window.<br />`default: 5` |
| `request_in_flight_limit` | `int` | The maximum number of in-flight requests allowed at any given time.<br />`default: 5` |
| `request_timeout_secs` | `int` | The maximum time a request can take before being aborted.<br />`default: 60` `unit: seconds` |
| `retry_attempts` | `int` | The maximum number of retries to make for failed requests.<br />`default: 5` |
| `retry_backoff_secs` | `int` | The amount of time to wait before attempting a failed request again.<br />`default: 5` `unit: seconds` |
| **OPTIONAL** - Buffer | | |
| `buffer.type` | `string` | The buffer's type / location. `disk` buffers are persistent and will be retained between restarts.<br />`default: "memory"` `enum: "memory", "disk"` |
| `buffer.when_full` | `string` | The behavior when the buffer becomes full.<br />`default: "block"` `enum: "block", "drop_newest"` |
| `buffer.max_size` | `int` | Only relevant when `type` is `disk`. The maximum size of the buffer on the disk.<br />`no default` `example: 104900000` |
| `buffer.num_items` | `int` | Only relevant when `type` is `memory`. The maximum number of [events][docs.event] allowed in the buffer.<br />`default: 500` |

## Examples

The `elasticsearch` sink batches [`log`][docs.log_event] up to the `batch_size` or `batch_timeout` options. When flushed, Vector will write to [Elasticsearch][url.elasticsearch] via the [`_bulk` API endpoint](https://www.elastic.co/guide/en/elasticsearch/reference/current/docs-bulk.html). The encoding is dictated by the `encoding` option. For example:

```http
POST <host>/_bulk HTTP/1.1
Host: <host>
Content-Type: application/x-ndjson
Content-Length: 654

{ "index" : { "_index" : "<index>" } }
{"timestamp": 1557932537, "message": "GET /roi/evolve/embrace/transparent", "host": "Stracke8362", "process_id": 914, "remote_addr": "30.163.82.140", "response_code": 504, "bytes": 29763} 
{ "index" : { "_index" : "<index>" } }
{"timestamp": 1557933548, "message": "PUT /value-added/b2b", "host": "Wiza2458", "process_id": 775, "remote_addr": "30.163.82.140", "response_code": 503, "bytes": 9468}
{ "index" : { "_index" : "<index>" } }
{"timestamp": 1557933742, "message": "DELETE /reinvent/interfaces", "host": "Herman3087", "process_id": 775, "remote_addr": "43.246.221.247", "response_code": 503, "bytes": 9700}
```

## How It Works

### Buffers & Batches

 
![][images.sink-flow-partitioned]

The `elasticsearch` sink buffers, batches, and
partitions data as shown in the diagram above. You'll notice that Vector treats
these concepts differently, instead of treating them as global concepts, Vector
treats them as sink specific concepts. This isolates sinks, ensuring services
disruptions are contained and [delivery guarantees][docs.guarantees] are
honored.

#### Buffers types

The `buffer.type` option allows you to control buffer resource usage:

| Type     | Description                                                                                                    |
|:---------|:---------------------------------------------------------------------------------------------------------------|
| `memory` | Pros: Fast. Cons: Not persisted across restarts. Possible data loss in the event of a crash. Uses more memory. |
| `disk`   | Pros: Persisted across restarts, durable. Uses much less memory. Cons: Slower, see below.                      |

#### Buffer overflow

The `buffer.when_full` option allows you to control the behavior when the
buffer overflows:

| Type          | Description                                                                                                                        |
|:--------------|:-----------------------------------------------------------------------------------------------------------------------------------|
| `block`       | Applies back pressure until the buffer makes room. This will help to prevent data loss but will cause data to pile up on the edge. |
| `drop_newest` | Drops new data as it's received. This data is lost. This should be used when performance is the highest priority.                  |

#### Batch flushing

Batches are flushed when 1 of 2 conditions are met:

1. The batch age meets or exceeds the configured `batch_timeout` (default: `1 bytes`).
2. The batch size meets or exceeds the configured `batch_size` (default: `10490000 bytes`).

### Delivery Guarantee

Due to the nature of this component, it offers a
[**best effort** delivery guarantee][docs.best_effort_delivery].

### Health Checks

Upon [starting][docs.starting], Vector will perform a simple health check
against this sink. The ensures that the downstream service is healthy and
reachable. By default, if the health check fails an error will be logged and
Vector will proceed to restart. Vector will continually check the health of
the service on an interval until healthy.

If you'd like to exit immediately when a service is unhealthy you can pass
the `--require-healthy` flag:

```bash
vector --config /etc/vector/vector.toml --require-healthy
```

Be careful when doing this if you have multiple sinks configured, as it will
prevent Vector from starting is one sink is unhealthy, preventing the other
healthy sinks from receiving data.

### Nested Documents

Vector will explode events into nested documents before writing them to
Elasticsearch. Vector assumes keys with a . delimit nested fields. You can read
more about how Vector handles nested documents in the [Data Model
document][docs.data_model].

### Partitioning

Partitioning is controlled via the `index`
options and allows you to dynamically partition data. You'll notice that
[`strftime` specifiers][url.strftime_specifiers] are allowed in the values,
enabling dynamic partitioning. The interpolated result is effectively the
internal batch partition key. Let's look at a few examples:

| Value | Interpolation | Desc |
|:------|:--------------|:-----|
| `date=%F` | `date=2019-05-02` | Partitions data by the event's day. |
| `date=%Y` | `date=2019` | Partitions data by the event's year. |
| `timestamp=%s` | `timestamp=1562450045` | Partitions data by the unix timestamp. |

### Rate Limits

Vector offers a few levers to control the rate and volume of requests to the
downstream service. Start with the `rate_limit_duration` and `rate_limit_num`
options to ensure Vector does not exceed the specified number of requests in
the specified window. You can further control the pace at which this window is
saturated with the `request_in_flight_limit` option, which will guarantee no
more than the specified number of requests are in-flight at any given time.

Please note, Vector's defaults are carefully chosen and it should be rare that
you need to adjust these. If you found a good reason to do so please share it
with the Vector team by [opening an issie][url.new_elasticsearch_sink_issue].

### Retry Policy

Vector will retry failed requests (status == `429`, >= `500`, and != `501`).
Other responses will _not_ be retried. You can control the number of retry
attempts and backoff rate with the `retry_attempts` and `retry_backoff_secs` options.

### Timeouts

To ensure the pipeline does not halt when a service fails to respond Vector
will abort requests after `60 seconds`.
This can be adjsuted with the `request_timeout_secs` option.

It is highly recommended that you do not lower value below the service's
internal timeout, as this could create orphaned requests, pile on retries,
and result in deuplicate data downstream.

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring_logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open sink issues][url.elasticsearch_sink_issues].
2. [Search the forum][url.search_forum] for any similar issues.
2. Reach out to the [community][url.community] for help.

## Resources

* [**Issues**][url.elasticsearch_sink_issues] - [enhancements][url.elasticsearch_sink_enhancements] - [bugs][url.elasticsearch_sink_bugs]
* [**Source code**][url.elasticsearch_sink_source]


[docs.best_effort_delivery]: ../../../about/guarantees.md#best-effort-delivery
[docs.config_composition]: ../../../usage/configuration/README.md#composition
[docs.data_model]: ../../../about/data-model.md
[docs.event]: ../../../about/data-model.md#event
[docs.guarantees]: ../../../about/guarantees.md
[docs.log_event]: ../../../about/data-model.md#log
[docs.monitoring_logs]: ../../../usage/administration/monitoring.md#logs
[docs.sources]: ../../../usage/configuration/sources
[docs.starting]: ../../../usage/administration/starting.md
[docs.transforms]: ../../../usage/configuration/transforms
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[images.elasticsearch_sink]: ../../../assets/elasticsearch-sink.svg
[images.sink-flow-partitioned]: ../../../assets/sink-flow-partitioned.svg
[url.community]: https://vector.dev/community
[url.elasticsearch]: https://www.elastic.co/products/elasticsearch
[url.elasticsearch_sink_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+elasticsearch%22+label%3A%22Type%3A+Bug%22
[url.elasticsearch_sink_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+elasticsearch%22+label%3A%22Type%3A+Enhancement%22
[url.elasticsearch_sink_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+elasticsearch%22
[url.elasticsearch_sink_source]: https://github.com/timberio/vector/tree/master/src/sinks/elasticsearch.rs
[url.new_elasticsearch_sink_issue]: https://github.com/timberio/vector/issues/new?labels%5B%5D=Sink%3A+elasticsearch
[url.new_elasticsearch_sink_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+new_elasticsearch%22
[url.search_forum]: https://forum.vector.dev/search?expanded=true
[url.strftime_specifiers]: https://docs.rs/chrono/0.3.1/chrono/format/strftime/index.html
