---
description: Batches `log` events to a Splunk HTTP Event Collector.
---

<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/sinks/splunk_hec.md.erb
-->

# splunk_hec sink

![][images.splunk_hec_sink]


The `splunk_hec` sink [batches](#buffers-and-batches) [`log`][docs.log_event] events to a [Splunk HTTP Event Collector][url.splunk_hec].

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```coffeescript
[sinks.my_sink_id]
  # REQUIRED - General
  type = "splunk_hec" # must be: "splunk_hec"
  inputs = ["my-source-id"]
  host = "my-splunk-host.com"
  token = "A94A8FE5CCB19BA61C4C08"
  
  # OPTIONAL - General
  healthcheck = true # default
  
  # OPTIONAL - Batching
  batch_size = 1049000 # default, bytes
  batch_timeout = 1 # default, seconds
  
  # OPTIONAL - Requests
  encoding = "ndjson" # no default, enum: "ndjson" or "text"
  rate_limit_duration = 1 # default, seconds
  rate_limit_num = 10 # default
  request_in_flight_limit = 10 # default
  request_timeout_secs = 60 # default, seconds
  retry_attempts = 5 # default
  retry_backoff_secs = 5 # default, seconds
  
  # OPTIONAL - Buffer
  [sinks.my_sink_id.buffer]
    type = "memory" # default, enum: "memory" or "disk"
    when_full = "block" # default, enum: "block" or "drop_newest"
    max_size = 104900000 # no default, bytes, relevant when type = "disk"
    num_items = 500 # default, events, relevant when type = "memory"
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (schema)" %}
```coffeescript
[sinks.<sink-id>]
  # REQUIRED - General
  type = "splunk_hec"
  inputs = ["<string>", ...]
  host = "<string>"
  token = "<string>"

  # OPTIONAL - General
  healthcheck = <bool>

  # OPTIONAL - Batching
  batch_size = <int>
  batch_timeout = <int>

  # OPTIONAL - Requests
  encoding = {"ndjson" | "text"}
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
[sinks.splunk_hec_sink]
  #
  # General
  #

  # The component type
  # 
  # * required
  # * no default
  # * must be: "splunk_hec"
  type = "splunk_hec"

  # A list of upstream source or transform IDs. See Config Composition for more
  # info.
  # 
  # * required
  # * no default
  inputs = ["my-source-id"]

  # Your Splunk HEC host.
  # 
  # * required
  # * no default
  host = "my-splunk-host.com"

  # Your Splunk HEC token.
  # 
  # * required
  # * no default
  token = "A94A8FE5CCB19BA61C4C08"

  # Enables/disables the sink healthcheck upon start.
  # 
  # * optional
  # * default: true
  healthcheck = true

  #
  # Batching
  #

  # The maximum size of a batch before it is flushed.
  # 
  # * optional
  # * default: 1049000
  # * unit: bytes
  batch_size = 1049000

  # The maximum age of a batch before it is flushed.
  # 
  # * optional
  # * default: 1
  # * unit: seconds
  batch_timeout = 1

  #
  # Requests
  #

  # The encoding format used to serialize the events before flushing. The default
  # is dynamic based on if the event is structured or not.
  # 
  # * optional
  # * no default
  # * enum: "ndjson" or "text"
  encoding = "ndjson"
  encoding = "text"

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
  # * default: 10
  rate_limit_num = 10

  # The maximum number of in-flight requests allowed at any given time.
  # 
  # * optional
  # * default: 10
  request_in_flight_limit = 10

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

  [sinks.splunk_hec_sink.buffer]
    # The buffer's type / location. `disk` buffers are persistent and will be
    # retained between restarts.
    # 
    # * optional
    # * default: "memory"
    # * enum: "memory" or "disk"
    type = "memory"
    type = "disk"

    # The behavior when the buffer becomes full.
    # 
    # * optional
    # * default: "block"
    # * enum: "block" or "drop_newest"
    when_full = "block"
    when_full = "drop_newest"

    # The maximum size of the buffer on the disk.
    # 
    # * optional
    # * no default
    # * unit: bytes
    max_size = 104900000

    # The maximum number of events allowed in the buffer.
    # 
    # * optional
    # * default: 500
    # * unit: events
    num_items = 500
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Options

| Key  | Type  | Description |
|:-----|:-----:|:------------|
| **REQUIRED** - General | | |
| `type` | `string` | The component type<br />`required` `must be: "splunk_hec"` |
| `inputs` | `[string]` | A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.config_composition] for more info.<br />`required` `example: ["my-source-id"]` |
| `host` | `string` | Your Splunk HEC host.<br />`required` `example: "my-splunk-host.com"` |
| `token` | `string` | Your Splunk HEC token.<br />`required` `example: "A94A8FE5CCB19BA61C4C08"` |
| **OPTIONAL** - General | | |
| `healthcheck` | `bool` | Enables/disables the sink healthcheck upon start. See [Health Checks](#health-checks) for more info.<br />`default: true` |
| **OPTIONAL** - Batching | | |
| `batch_size` | `int` | The maximum size of a batch before it is flushed. See [Buffers & Batches](#buffers-batches) for more info.<br />`default: 1049000` `unit: bytes` |
| `batch_timeout` | `int` | The maximum age of a batch before it is flushed. See [Buffers & Batches](#buffers-batches) for more info.<br />`default: 1` `unit: seconds` |
| **OPTIONAL** - Requests | | |
| `encoding` | `string` | The encoding format used to serialize the events before flushing. The default is dynamic based on if the event is structured or not. See [Encodings](#encodings) for more info.<br />`no default` `enum: "ndjson" or "text"` |
| `rate_limit_duration` | `int` | The window used for the `request_rate_limit_num` option See [Rate Limits](#rate-limits) for more info.<br />`default: 1` `unit: seconds` |
| `rate_limit_num` | `int` | The maximum number of requests allowed within the `rate_limit_duration` window. See [Rate Limits](#rate-limits) for more info.<br />`default: 10` |
| `request_in_flight_limit` | `int` | The maximum number of in-flight requests allowed at any given time. See [Rate Limits](#rate-limits) for more info.<br />`default: 10` |
| `request_timeout_secs` | `int` | The maximum time a request can take before being aborted. See [Timeouts](#timeouts) for more info.<br />`default: 60` `unit: seconds` |
| `retry_attempts` | `int` | The maximum number of retries to make for failed requests. See [Retry Policy](#retry-policy) for more info.<br />`default: 5` |
| `retry_backoff_secs` | `int` | The amount of time to wait before attempting a failed request again. See [Retry Policy](#retry-policy) for more info.<br />`default: 5` `unit: seconds` |
| **OPTIONAL** - Buffer | | |
| `buffer.type` | `string` | The buffer's type / location. `disk` buffers are persistent and will be retained between restarts.<br />`default: "memory"` `enum: "memory" or "disk"` |
| `buffer.when_full` | `string` | The behavior when the buffer becomes full.<br />`default: "block"` `enum: "block" or "drop_newest"` |
| `buffer.max_size` | `int` | The maximum size of the buffer on the disk. Only relevant when type = "disk"<br />`no default` `example: 104900000` `unit: bytes` |
| `buffer.num_items` | `int` | The maximum number of [events][docs.event] allowed in the buffer. Only relevant when type = "memory"<br />`default: 500` `unit: events` |

## How It Works

### Buffers & Batches

![][images.sink-flow-serial]

The `splunk_hec` sink buffers & batches data as
shown in the diagram above. You'll notice that Vector treats these concepts
differently, instead of treating them as global concepts, Vector treats them
as sink specific concepts. This isolates sinks, ensuring services disruptions
are contained and [delivery guarantees][docs.guarantees] are honored.

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

1. The batch age meets or exceeds the configured `batch_timeout` (default: `1 seconds`).
2. The batch size meets or exceeds the configured `batch_size` (default: `1049000 bytes`).

### Delivery Guarantee

This component offers an [**at least once** delivery guarantee][docs.at_least_once_delivery]
if your [pipeline is configured to achieve this][docs.at_least_once_delivery].

### Encodings

The `splunk_hec` sink encodes events before writing
them downstream. This is controlled via the `encoding` option which accepts
the following options:

| Encoding | Description |
| :------- | :---------- |
| `ndjson` | The payload will be encoded in new line delimited JSON payload, each line representing a JSON encoded event. |
| `text` | The payload will be encoded as new line delimited text, each line representing the value of the `"message"` key. |

#### Dynamic encoding

By default, the `encoding` chosen is dynamic based on the explicit/implcit
nature of the event's structure. For example, if this event is parsed (explicit
structuring), Vector will use `json` to encode the structured data. If the event
was not explicitly structured, the `text` encoding will be used.

To further explain why Vector adopts this default, take the simple example of
accepting data over the [`tcp` source][docs.tcp_source] and then connecting
it directly to the `splunk_hec` sink. It is less
surprising that the outgoing data reflects the incoming data exactly since it
was not explicitly structured.

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration.environment-variables]
section.

### Health Checks

Health checks ensure that the downstream service is accessible and ready to
accept data. This check is performed upon sink initialization.

If the health check fails an error will be logged and Vector will proceed to
start. If you'd like to exit immediately upon health check failure, you can
pass the `--require-healthy` flag:

```bash
vector --config /etc/vector/vector.toml --require-healthy
```

And finally, if you'd like to disable health checks entirely for this sink
you can set the `healthcheck` option to `false`.

### Rate Limits

Vector offers a few levers to control the rate and volume of requests to the
downstream service. Start with the `rate_limit_duration` and `rate_limit_num`
options to ensure Vector does not exceed the specified number of requests in
the specified window. You can further control the pace at which this window is
saturated with the `request_in_flight_limit` option, which will guarantee no
more than the specified number of requests are in-flight at any given time.

Please note, Vector's defaults are carefully chosen and it should be rare that
you need to adjust these. If you found a good reason to do so please share it
with the Vector team by [opening an issie][url.new_splunk_hec_sink_issue].

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

1. Check for any [open `splunk_hec_sink` issues][url.splunk_hec_sink_issues].
2. If encountered a bug, please [file a bug report][url.new_splunk_hec_sink_bug].
3. If encountered a missing feature, please [file a feature request][url.new_splunk_hec_sink_enhancement].
4. If you need help, [join our chat/forum community][url.vector_chat]. You can post a question and search previous questions.

### Setup

In order to supply values for both the `host` and `token` options you must first
setup a Splunk HTTP Event Collector. Please refer to the [Splunk setup
docs][url.splunk_hec_setup] for a guide on how to do this. Once you've setup
your Spunk HTTP Collectory you'll be provided a `host` and `token` that you
should supply to the `host` and `token` options.

## Resources

* [**Issues**][url.splunk_hec_sink_issues] - [enhancements][url.splunk_hec_sink_enhancements] - [bugs][url.splunk_hec_sink_bugs]
* [**Source code**][url.splunk_hec_sink_source]


[docs.at_least_once_delivery]: ../../../about/guarantees.md#at-least-once-delivery
[docs.config_composition]: ../../../usage/configuration/README.md#composition
[docs.configuration.environment-variables]: ../../../usage/configuration#environment-variables
[docs.event]: ../../../about/data-model/README.md#event
[docs.guarantees]: ../../../about/guarantees.md
[docs.log_event]: ../../../about/data-model/log.md
[docs.monitoring_logs]: ../../../usage/administration/monitoring.md#logs
[docs.sources]: ../../../usage/configuration/sources
[docs.tcp_source]: ../../../usage/configuration/sources/tcp.md
[docs.transforms]: ../../../usage/configuration/transforms
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[images.sink-flow-serial]: ../../../assets/sink-flow-serial.svg
[images.splunk_hec_sink]: ../../../assets/splunk_hec-sink.svg
[url.new_splunk_hec_sink_bug]: https://github.com/timberio/vector/issues/new?labels=Sink%3A+splunk_hec&labels=Type%3A+Bug
[url.new_splunk_hec_sink_enhancement]: https://github.com/timberio/vector/issues/new?labels=Sink%3A+splunk_hec&labels=Type%3A+Enhancement
[url.new_splunk_hec_sink_issue]: https://github.com/timberio/vector/issues/new?labels=Sink%3A+splunk_hec
[url.splunk_hec]: http://dev.splunk.com/view/event-collector/SP-CAAAE6M
[url.splunk_hec_setup]: https://docs.splunk.com/Documentation/Splunk/latest/Data/UsetheHTTPEventCollector
[url.splunk_hec_sink_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+splunk_hec%22+label%3A%22Type%3A+Bug%22
[url.splunk_hec_sink_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+splunk_hec%22+label%3A%22Type%3A+Enhancement%22
[url.splunk_hec_sink_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+splunk_hec%22
[url.splunk_hec_sink_source]: https://github.com/timberio/vector/tree/master/src/sinks/splunk_hec.rs
[url.vector_chat]: https://chat.vector.dev
