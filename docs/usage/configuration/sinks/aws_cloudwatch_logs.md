---
description: Batches `log` events to AWS CloudWatch Logs via the `PutLogEvents` API endpoint.
---

<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/sinks/aws_cloudwatch_logs.md.erb
-->

# aws_cloudwatch_logs sink

![][images.aws_cloudwatch_logs_sink]

{% hint style="warning" %}
The `aws_cloudwatch_logs` sink is in beta. Please see the current
[enhancements][url.aws_cloudwatch_logs_sink_enhancements] and
[bugs][url.aws_cloudwatch_logs_sink_bugs] for known issues.
We kindly ask that you [add any missing issues][url.new_aws_cloudwatch_logs_sink_issue]
as it will help shape the roadmap of this component.
{% endhint %}

The `aws_cloudwatch_logs` sink [batches](#buffers-and-batches) [`log`][docs.log_event] events to [AWS CloudWatch Logs][url.aws_cw_logs] via the [`PutLogEvents` API endpoint](https://docs.aws.amazon.com/AmazonCloudWatchLogs/latest/APIReference/API_PutLogEvents.html).

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```coffeescript
[sinks.my_sink_id]
  # REQUIRED - General
  type = "aws_cloudwatch_logs" # must be: "aws_cloudwatch_logs"
  inputs = ["my-source-id"]
  group_name = "/var/log/{{ file }}.log"
  region = "us-east-1"
  stream_name = "{{ instance_id }}"
  
  # OPTIONAL - General
  healthcheck = true # default
  hostname = "127.0.0.0:5000"
  
  # OPTIONAL - Batching
  batch_size = 1049000 # default, bytes
  batch_timeout = 1 # default, seconds
  
  # OPTIONAL - Requests
  encoding = "json" # no default, enum: "json" or "text"
  rate_limit_duration = 1 # default, seconds
  rate_limit_num = 5 # default
  request_in_flight_limit = 5 # default
  request_timeout_secs = 30 # default, seconds
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
  type = "aws_cloudwatch_logs"
  inputs = ["<string>", ...]
  group_name = "<string>"
  region = "<string>"
  stream_name = "<string>"

  # OPTIONAL - General
  healthcheck = <bool>
  hostname = "<string>"

  # OPTIONAL - Batching
  batch_size = <int>
  batch_timeout = <int>

  # OPTIONAL - Requests
  encoding = {"json" | "text"}
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
[sinks.aws_cloudwatch_logs_sink]
  #
  # General
  #

  # The component type
  # 
  # * required
  # * no default
  # * must be: "aws_cloudwatch_logs"
  type = "aws_cloudwatch_logs"

  # A list of upstream source or transform IDs. See Config Composition for more
  # info.
  # 
  # * required
  # * no default
  inputs = ["my-source-id"]

  # The group name of the target CloudWatch Logs stream.
  # 
  # * required
  # * no default
  group_name = "/var/log/{{ file }}.log"
  group_name = "ec2/{{ instance_id }}"

  # The AWS region of the target CloudWatch Logs stream resides.
  # 
  # * required
  # * no default
  region = "us-east-1"

  # The stream name of the target CloudWatch Logs stream.
  # 
  # * required
  # * no default
  stream_name = "{{ instance_id }}"
  stream_name = "stream-name"
  stream_name = "%Y-%m-%d"

  # Enables/disables the sink healthcheck upon start.
  # 
  # * optional
  # * default: true
  healthcheck = true

  # Custom hostname to send requests to. Useful for testing.
  # 
  # * optional
  # * no default
  hostname = "127.0.0.0:5000"

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
  # * enum: "json" or "text"
  encoding = "json"
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
  # * default: 30
  # * unit: seconds
  request_timeout_secs = 30

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

  [sinks.aws_cloudwatch_logs_sink.buffer]
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
| `type` | `string` | The component type<br />`required` `must be: "aws_cloudwatch_logs"` |
| `inputs` | `[string]` | A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.config_composition] for more info.<br />`required` `example: ["my-source-id"]` |
| `group_name` | `string` | The [group name][url.aws_cw_logs_group_name] of the target CloudWatch Logs stream.This option supports dynamic values via [Vector's template syntax][docs.configuration.template-syntax]. See [Partitioning](#partitioning) and [Template Syntax](#template-syntax) for more info.<br />`required` `example: "/var/log/{{ file }}.log"` |
| `region` | `string` | The [AWS region][url.aws_cw_logs_regions] of the target CloudWatch Logs stream resides.<br />`required` `example: "us-east-1"` |
| `stream_name` | `string` | The [stream name][url.aws_cw_logs_stream_name] of the target CloudWatch Logs stream.This option supports dynamic values via [Vector's template syntax][docs.configuration.template-syntax]. See [Partitioning](#partitioning) and [Template Syntax](#template-syntax) for more info.<br />`required` `example: "{{ instance_id }}"` |
| **OPTIONAL** - General | | |
| `healthcheck` | `bool` | Enables/disables the sink healthcheck upon start. See [Health Checks](#health-checks) for more info.<br />`default: true` |
| `hostname` | `string` | Custom hostname to send requests to. Useful for testing.<br />`default: "<aws-service-hostname>"` |
| **OPTIONAL** - Batching | | |
| `batch_size` | `int` | The maximum size of a batch before it is flushed. See [Buffers & Batches](#buffers-batches) for more info.<br />`default: 1049000` `unit: bytes` |
| `batch_timeout` | `int` | The maximum age of a batch before it is flushed. See [Buffers & Batches](#buffers-batches) for more info.<br />`default: 1` `unit: seconds` |
| **OPTIONAL** - Requests | | |
| `encoding` | `string` | The encoding format used to serialize the events before flushing. The default is dynamic based on if the event is structured or not. See [Encodings](#encodings) for more info.<br />`no default` `enum: "json" or "text"` |
| `rate_limit_duration` | `int` | The window used for the `request_rate_limit_num` option See [Rate Limits](#rate-limits) for more info.<br />`default: 1` `unit: seconds` |
| `rate_limit_num` | `int` | The maximum number of requests allowed within the `rate_limit_duration` window. See [Rate Limits](#rate-limits) for more info.<br />`default: 5` |
| `request_in_flight_limit` | `int` | The maximum number of in-flight requests allowed at any given time. See [Rate Limits](#rate-limits) for more info.<br />`default: 5` |
| `request_timeout_secs` | `int` | The maximum time a request can take before being aborted. See [Timeouts](#timeouts) for more info.<br />`default: 30` `unit: seconds` |
| `retry_attempts` | `int` | The maximum number of retries to make for failed requests. See [Retry Policy](#retry-policy) for more info.<br />`default: 5` |
| `retry_backoff_secs` | `int` | The amount of time to wait before attempting a failed request again. See [Retry Policy](#retry-policy) for more info.<br />`default: 5` `unit: seconds` |
| **OPTIONAL** - Buffer | | |
| `buffer.type` | `string` | The buffer's type / location. `disk` buffers are persistent and will be retained between restarts.<br />`default: "memory"` `enum: "memory" or "disk"` |
| `buffer.when_full` | `string` | The behavior when the buffer becomes full.<br />`default: "block"` `enum: "block" or "drop_newest"` |
| `buffer.max_size` | `int` | The maximum size of the buffer on the disk. Only relevant when type = "disk"<br />`no default` `example: 104900000` `unit: bytes` |
| `buffer.num_items` | `int` | The maximum number of [events][docs.event] allowed in the buffer. Only relevant when type = "memory"<br />`default: 500` `unit: events` |

## Examples

```http
POST / HTTP/1.1
Host: logs.<region>.<domain>
X-Amz-Date: <date>
Accept: application/json
Content-Type: application/x-amz-json-1.1
Content-Length: <byte_size>
Connection: Keep-Alive
X-Amz-Target: Logs_20140328.PutLogEvents
{
  "logGroupName": "<group_name>",
  "logStreamName": "<stream_name>",
  "logEvents": [
    {
      "timestamp": <timestamp>, 
      "message": "<encoded_event>"
    }, 
    {
      "timestamp": <timestamp>, 
      "message": "<encoded_event>"
    }, 
    {
      "timestamp": <timestamp>, 
      "message": "<encoded_event>"
    }
  ]
}
```

## How It Works

### Authentication

Vector checks for AWS credentials in the following order:

1. Environment variables `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY`.
​2. The [`credential_process` command][url.aws_credential_process] in the AWS config file. (usually located at `~/.aws/config`)
​3. The [AWS credentials file][url.aws_credentials_file]. (usually located at `~/.aws/credentials`)
4. The ​[IAM instance profile][url.iam_instance_profile]. (will only work if running on an EC2 instance with an instance profile/role)

If credentials are not found the [healtcheck](#healthchecks) will fail and an
error will be [logged][docs.monitoring_logs].

#### Obtaining an access key

In general, we recommend using instance profiles/roles whenever possible. In
cases where this is not possible you can generate an AWS access key for any user
within your AWS account. AWS provides a [detailed guide][url.aws_access_keys] on
how to do this.

### Buffers & Batches

 
![][images.sink-flow-partitioned]

The `aws_cloudwatch_logs` sink buffers & batches data as
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

The `aws_cloudwatch_logs` sink encodes events before writing
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
it directly to the `aws_cloudwatch_logs` sink. It is less
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

### Partitioning

Partitioning is controlled via the `group_name` and `stream_name`
options and allows you to dynamically partition data on the fly. You'll notice
that [`strftime` specifiers][url.strftime_specifiers] are allowed in the values,
enabling this partitioning. The interpolated result is effectively the internal
partition key. Let's look at a few examples:

| Value          | Interpolation          | Desc                                   |
|:---------------|:-----------------------|:---------------------------------------|
| `date=%F`      | `date=2019-05-02`      | Partitions data by the event's day.    |
| `date=%Y`      | `date=2019`            | Partitions data by the event's year.   |
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
with the Vector team by [opening an issie][url.new_aws_cloudwatch_logs_sink_issue].

### Retry Policy

Vector will retry failed requests (status == `429`, >= `500`, and != `501`).
Other responses will _not_ be retried. You can control the number of retry
attempts and backoff rate with the `retry_attempts` and `retry_backoff_secs` options.

### Template Syntax

The `group_name` and `stream_name` options
support [Vector's template syntax][docs.configuration.template-syntax],
enabling dynamic values derived from the event's data. This syntax accepts
[strftime specifiers][url.strftime_specifiers] as well as the
`{{ field_name }}` syntax for accessing event fields. For example:

```coffeescript
[sinks.my_aws_cloudwatch_logs_sink_id]
  # ...
  group_name = "/var/log/{{ file }}.log"
  group_name = "ec2/{{ instance_id }}"
  # ...
```

You can read more about the complete syntax in the
[template syntax section][docs.configuration.template-syntax].

### Timeouts

To ensure the pipeline does not halt when a service fails to respond Vector
will abort requests after `30 seconds`.
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

1. Check for any [open `aws_cloudwatch_logs_sink` issues][url.aws_cloudwatch_logs_sink_issues].
2. If encountered a bug, please [file a bug report][url.new_aws_cloudwatch_logs_sink_bug].
3. If encountered a missing feature, please [file a feature request][url.new_aws_cloudwatch_logs_sink_enhancement].
4. If you need help, [join our chat/forum community][url.vector_chat]. You can post a question and search previous questions.

## Resources

* [**Issues**][url.aws_cloudwatch_logs_sink_issues] - [enhancements][url.aws_cloudwatch_logs_sink_enhancements] - [bugs][url.aws_cloudwatch_logs_sink_bugs]
* [**Source code**][url.aws_cloudwatch_logs_sink_source]
* [**Service Limits**][url.aws_cw_logs_service_limits]


[docs.at_least_once_delivery]: ../../../about/guarantees.md#at-least-once-delivery
[docs.config_composition]: ../../../usage/configuration/README.md#composition
[docs.configuration.environment-variables]: ../../../usage/configuration#environment-variables
[docs.configuration.template-syntax]: ../../../usage/configuration#template-syntax
[docs.event]: ../../../about/data-model/README.md#event
[docs.guarantees]: ../../../about/guarantees.md
[docs.log_event]: ../../../about/data-model/log.md
[docs.monitoring_logs]: ../../../usage/administration/monitoring.md#logs
[docs.sources]: ../../../usage/configuration/sources
[docs.tcp_source]: ../../../usage/configuration/sources/tcp.md
[docs.transforms]: ../../../usage/configuration/transforms
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[images.aws_cloudwatch_logs_sink]: ../../../assets/aws_cloudwatch_logs-sink.svg
[images.sink-flow-partitioned]: ../../../assets/sink-flow-partitioned.svg
[url.aws_access_keys]: https://docs.aws.amazon.com/IAM/latest/UserGuide/id_credentials_access-keys.html
[url.aws_cloudwatch_logs_sink_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+aws_cloudwatch_logs%22+label%3A%22Type%3A+Bug%22
[url.aws_cloudwatch_logs_sink_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+aws_cloudwatch_logs%22+label%3A%22Type%3A+Enhancement%22
[url.aws_cloudwatch_logs_sink_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+aws_cloudwatch_logs%22
[url.aws_cloudwatch_logs_sink_source]: https://github.com/timberio/vector/blob/master/src/sinks/aws_cloudwatch_logs/mod.rs
[url.aws_credential_process]: https://docs.aws.amazon.com/cli/latest/userguide/cli-configure-sourcing-external.html
[url.aws_credentials_file]: https://docs.aws.amazon.com/cli/latest/userguide/cli-configure-files.html
[url.aws_cw_logs]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/WhatIsCloudWatchLogs.html
[url.aws_cw_logs_group_name]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/Working-with-log-groups-and-streams.html
[url.aws_cw_logs_regions]: https://docs.aws.amazon.com/general/latest/gr/rande.html#cw_region
[url.aws_cw_logs_service_limits]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/cloudwatch_limits_cwl.html
[url.aws_cw_logs_stream_name]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/Working-with-log-groups-and-streams.html
[url.iam_instance_profile]: https://docs.aws.amazon.com/IAM/latest/UserGuide/id_roles_use_switch-role-ec2_instance-profiles.html
[url.new_aws_cloudwatch_logs_sink_bug]: https://github.com/timberio/vector/issues/new?labels=Sink%3A+aws_cloudwatch_logs&labels=Type%3A+Bug
[url.new_aws_cloudwatch_logs_sink_enhancement]: https://github.com/timberio/vector/issues/new?labels=Sink%3A+aws_cloudwatch_logs&labels=Type%3A+Enhancement
[url.new_aws_cloudwatch_logs_sink_issue]: https://github.com/timberio/vector/issues/new?labels=Sink%3A+aws_cloudwatch_logs
[url.strftime_specifiers]: https://docs.rs/chrono/0.3.1/chrono/format/strftime/index.html
[url.vector_chat]: https://chat.vector.dev
