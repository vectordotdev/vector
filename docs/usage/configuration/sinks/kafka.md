---
description: Streams `log` events to Apache Kafka via the Kafka protocol.
---

<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/sinks/kafka.md.erb
-->

# kafka sink

![][images.kafka_sink]


The `kafka` sink [streams](#streaming) [`log`][docs.log_event] events to [Apache Kafka][url.kafka] via the [Kafka protocol][url.kafka_protocol].

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```coffeescript
[sinks.my_sink_id]
  # REQUIRED - General
  type = "kafka" # must be: "kafka"
  inputs = ["my-source-id"]
  bootstrap_servers = "10.14.22.123:9092,10.14.23.332:9092"
  key_field = "user_id"
  topic = "topic-1234"
  
  # OPTIONAL - General
  encoding = "json" # no default, enum: "json" or "text"
  
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
  type = "kafka"
  inputs = ["<string>", ...]
  bootstrap_servers = "<string>"
  key_field = "<string>"
  topic = "<string>"

  # OPTIONAL - General
  encoding = {"json" | "text"}

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
[sinks.kafka_sink]
  #
  # General
  #

  # The component type
  # 
  # * required
  # * no default
  # * must be: "kafka"
  type = "kafka"

  # A list of upstream source or transform IDs. See Config Composition for more
  # info.
  # 
  # * required
  # * no default
  inputs = ["my-source-id"]

  # A comma-separated list of host and port pairs that are the addresses of the
  # Kafka brokers in a "bootstrap" Kafka cluster that a Kafka client connects to
  # initially to bootstrap itself
  # 
  # * required
  # * no default
  bootstrap_servers = "10.14.22.123:9092,10.14.23.332:9092"

  # The field name to use for the topic key. If unspecified, the key will be
  # randomly generated. If the field does not exist on the event, a blank value
  # will be used.
  # 
  # * required
  # * no default
  key_field = "user_id"

  # The Kafka topic name to write events to.
  # 
  # * required
  # * no default
  topic = "topic-1234"

  # The encoding format used to serialize the events before flushing. The default
  # is dynamic based on if the event is structured or not.
  # 
  # * optional
  # * no default
  # * enum: "json" or "text"
  encoding = "json"
  encoding = "text"

  #
  # Buffer
  #

  [sinks.kafka_sink.buffer]
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
| `type` | `string` | The component type<br />`required` `must be: "kafka"` |
| `inputs` | `[string]` | A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.config_composition] for more info.<br />`required` `example: ["my-source-id"]` |
| `bootstrap_servers` | `string` | A comma-separated list of host and port pairs that are the addresses of the Kafka brokers in a "bootstrap" Kafka cluster that a Kafka client connects to initially to bootstrap itself<br />`required` `example: (see above)` |
| `key_field` | `string` | The field name to use for the topic key. If unspecified, the key will be randomly generated. If the field does not exist on the event, a blank value will be used.<br />`required` `example: "user_id"` |
| `topic` | `string` | The Kafka topic name to write events to.<br />`required` `example: "topic-1234"` |
| **OPTIONAL** - General | | |
| `encoding` | `string` | The encoding format used to serialize the events before flushing. The default is dynamic based on if the event is structured or not. See [Encodings](#encodings) for more info.<br />`no default` `enum: "json" or "text"` |
| **OPTIONAL** - Buffer | | |
| `buffer.type` | `string` | The buffer's type / location. `disk` buffers are persistent and will be retained between restarts.<br />`default: "memory"` `enum: "memory" or "disk"` |
| `buffer.when_full` | `string` | The behavior when the buffer becomes full.<br />`default: "block"` `enum: "block" or "drop_newest"` |
| `buffer.max_size` | `int` | The maximum size of the buffer on the disk. Only relevant when type = "disk"<br />`no default` `example: 104900000` `unit: bytes` |
| `buffer.num_items` | `int` | The maximum number of [events][docs.event] allowed in the buffer. Only relevant when type = "memory"<br />`default: 500` `unit: events` |

## How It Works

### Delivery Guarantee

This component offers an [**at least once** delivery guarantee][docs.at_least_once_delivery]
if your [pipeline is configured to achieve this][docs.at_least_once_delivery].

### Encodings

The `kafka` sink encodes events before writing
them downstream. This is controlled via the `encoding` option which accepts
the following options:

| Encoding | Description |
| :------- | :---------- |
| `json` | The payload will be encoded as a single JSON payload. |
| `text` | The payload will be encoded as new line delimited text, each line representing the value of the `"message"` key. |

#### Dynamic encoding

By default, the `encoding` chosen is dynamic based on the explicit/implcit
nature of the event's structure. For example, if this event is parsed (explicit
structuring), Vector will use `json` to encode the structured data. If the event
was not explicitly structured, the `text` encoding will be used.

To further explain why Vector adopts this default, take the simple example of
accepting data over the [`tcp` source][docs.tcp_source] and then connecting
it directly to the `kafka` sink. It is less
surprising that the outgoing data reflects the incoming data exactly since it
was not explicitly structured.

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration.environment-variables]
section.

### Health Checks

Upon [starting][docs.starting], Vector will perform a simple health check
against this sink. The ensures that the downstream service is healthy and
reachable.
By default, if the health check fails an error will be logged and
Vector will proceed to start. If you'd like to exit immediately upomn healt
check failure, you can pass the `--require-healthy` flag:

```bash
vector --config /etc/vector/vector.toml --require-healthy
```

Be careful when doing this, one unhealthy sink can prevent other healthy sinks
from processing data at all.

### Streaming

The `kafka` sink streams data on a real-time
event-by-event basis. It does not batch data.

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring_logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open `kafka_sink` issues][url.kafka_sink_issues].
2. If encountered a bug, please [file a bug report][url.new_kafka_sink_bug].
3. If encountered a missing feature, please [file a feature request][url.new_kafka_sink_enhancement].
4. If you need help, [join our chat/forum community][url.vector_chat]. You can post a question and search previous questions.

## Resources

* [**Issues**][url.kafka_sink_issues] - [enhancements][url.kafka_sink_enhancements] - [bugs][url.kafka_sink_bugs]
* [**Source code**][url.kafka_sink_source]


[docs.at_least_once_delivery]: ../../../about/guarantees.md#at-least-once-delivery
[docs.config_composition]: ../../../usage/configuration/README.md#composition
[docs.configuration.environment-variables]: ../../../usage/configuration#environment-variables
[docs.event]: ../../../about/data-model/README.md#event
[docs.log_event]: ../../../about/data-model/log.md
[docs.monitoring_logs]: ../../../usage/administration/monitoring.md#logs
[docs.sources]: ../../../usage/configuration/sources
[docs.starting]: ../../../usage/administration/starting.md
[docs.tcp_source]: ../../../usage/configuration/sources/tcp.md
[docs.transforms]: ../../../usage/configuration/transforms
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[images.kafka_sink]: ../../../assets/kafka-sink.svg
[url.kafka]: https://kafka.apache.org/
[url.kafka_protocol]: https://kafka.apache.org/protocol
[url.kafka_sink_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+kafka%22+label%3A%22Type%3A+Bug%22
[url.kafka_sink_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+kafka%22+label%3A%22Type%3A+Enhancement%22
[url.kafka_sink_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+kafka%22
[url.kafka_sink_source]: https://github.com/timberio/vector/tree/master/src/sinks/kafka.rs
[url.new_kafka_sink_bug]: https://github.com/timberio/vector/issues/new?labels=Sink%3A+kafka&labels=Type%3A+Bug
[url.new_kafka_sink_enhancement]: https://github.com/timberio/vector/issues/new?labels=Sink%3A+kafka&labels=Type%3A+Enhancement
[url.vector_chat]: https://chat.vector.dev
