---
title: "aws_cloudwatch_logs sink" 
sidebar_label: "aws_cloudwatch_logs"
---

The `aws_cloudwatch_logs` sink [batches](#buffers-and-batches) [`log`][docs.data-model.log] events to [AWS CloudWatch Logs][urls.aws_cw_logs] via the [`PutLogEvents` API endpoint](https://docs.aws.amazon.com/AmazonCloudWatchLogs/latest/APIReference/API_PutLogEvents.html).

## Example

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

<Tabs
  defaultValue="simple"
  values={[
    { label: 'Simple', value: 'simple', },
    { label: 'Advanced', value: 'advanced', },
  ]
}>
<TabItem value="simple">

```coffeescript
[sinks.my_sink_id]
  type = "aws_cloudwatch_logs" # enum
  inputs = ["my-source-id"]
  group_name = "{{ file }}"
  region = "us-east-1"
  stream_name = "{{ instance_id }}"
```

</TabItem>
<TabItem value="advanced">

```coffeescript
[sinks.my_sink_id]
  # REQUIRED - General
  type = "aws_cloudwatch_logs" # enum
  inputs = ["my-source-id"]
  group_name = "{{ file }}"
  region = "us-east-1"
  stream_name = "{{ instance_id }}"
  
  # OPTIONAL - General
  create_missing_group = true # default
  create_missing_stream = true # default
  endpoint = "127.0.0.0:5000" # no default
  healthcheck = true # default
  
  # OPTIONAL - Batching
  batch_size = 1049000 # default, bytes
  batch_timeout = 1 # default, seconds
  
  # OPTIONAL - Requests
  rate_limit_duration = 1 # default, seconds
  rate_limit_num = 5 # default
  request_in_flight_limit = 5 # default
  request_timeout_secs = 30 # default, seconds
  retry_attempts = 5 # default
  retry_backoff_secs = 5 # default, seconds
  
  # OPTIONAL - Buffer
  [sinks.my_sink_id.buffer]
    type = "memory" # default, enum
    max_size = 104900000 # no default, bytes, relevant when type = "disk"
    num_items = 500 # default, events, relevant when type = "memory"
    when_full = "block" # default, enum
```

</TabItem>

</Tabs>

You can learn more

## Options

import Option from '@site/src/components/Option';
import Options from '@site/src/components/Options';

<Options filters={true}>


<Option
  defaultValue={1049000}
  enumValues={null}
  examples={[1049000]}
  name={"batch_size"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"int"}
  unit={"bytes"}>

### batch_size

The maximum size of a batch before it is flushed. See [Buffers & Batches](#buffers-batches) for more info.


</Option>


<Option
  defaultValue={1}
  enumValues={null}
  examples={[1]}
  name={"batch_timeout"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"int"}
  unit={"seconds"}>

### batch_timeout

The maximum age of a batch before it is flushed. See [Buffers & Batches](#buffers-batches) for more info.


</Option>


<Option
  defaultValue={null}
  enumValues={null}
  examples={[]}
  name={"buffer"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"table"}
  unit={null}>

### buffer

Configures the sink specific buffer.

<Options filters={false}>


<Option
  defaultValue={"memory"}
  enumValues={{"memory":"Stores the sink's buffer in memory. This is more performant (~3x), but less durable. Data will be lost if Vector is restarted abruptly.","disk":"Stores the sink's buffer on disk. This is less performance (~3x),  but durable. Data will not be lost between restarts."}}
  examples={["memory","disk"]}
  name={"type"}
  nullable={false}
  path={"buffer"}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"string"}
  unit={null}>

#### type

The buffer's type / location. `disk` buffers are persistent and will be retained between restarts.


</Option>


<Option
  defaultValue={"block"}
  enumValues={{"block":"Applies back pressure when the buffer is full. This prevents data loss, but will cause data to pile up on the edge.","drop_newest":"Drops new data as it's received. This data is lost. This should be used when performance is the highest priority."}}
  examples={["block","drop_newest"]}
  name={"when_full"}
  nullable={false}
  path={"buffer"}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"string"}
  unit={null}>

#### when_full

The behavior when the buffer becomes full.


</Option>


<Option
  defaultValue={null}
  enumValues={null}
  examples={[104900000]}
  name={"max_size"}
  nullable={true}
  path={"buffer"}
  relevantWhen={{"type":"disk"}}
  required={false}
  simple={false}
  type={"int"}
  unit={"bytes"}>

#### max_size

The maximum size of the buffer on the disk.


</Option>


<Option
  defaultValue={500}
  enumValues={null}
  examples={[500]}
  name={"num_items"}
  nullable={true}
  path={"buffer"}
  relevantWhen={{"type":"memory"}}
  required={false}
  simple={false}
  type={"int"}
  unit={"events"}>

#### num_items

The maximum number of [events][docs.event] allowed in the buffer.


</Option>


</Options>

</Option>


<Option
  defaultValue={true}
  enumValues={null}
  examples={[true,false]}
  name={"create_missing_group"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"bool"}
  unit={null}>

### create_missing_group

Dynamically create a [log group][urls.aws_cw_logs_group_name] if it does not already exist. This will ignore `create_missing_stream` directly after creating the group and will create the first stream. 


</Option>


<Option
  defaultValue={true}
  enumValues={null}
  examples={[true,false]}
  name={"create_missing_stream"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"bool"}
  unit={null}>

### create_missing_stream

Dynamically create a [log stream][urls.aws_cw_logs_stream_name] if it does not already exist.


</Option>


<Option
  defaultValue={null}
  enumValues={null}
  examples={["127.0.0.0:5000"]}
  name={"endpoint"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"string"}
  unit={null}>

### endpoint

Custom endpoint for use with AWS-compatible services.


</Option>


<Option
  defaultValue={null}
  enumValues={null}
  examples={["{{ file }}","ec2/{{ instance_id }}","group-name"]}
  name={"group_name"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  simple={true}
  type={"string"}
  unit={null}>

### group_name

The [group name][urls.aws_cw_logs_group_name] of the target CloudWatch Logs stream. See [Partitioning](#partitioning) and [Template Syntax](#template-syntax) for more info.


</Option>


<Option
  defaultValue={true}
  enumValues={null}
  examples={[true,false]}
  name={"healthcheck"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"bool"}
  unit={null}>

### healthcheck

Enables/disables the sink healthcheck upon start. See [Health Checks](#health-checks) for more info.


</Option>


<Option
  defaultValue={1}
  enumValues={null}
  examples={[1]}
  name={"rate_limit_duration"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"int"}
  unit={"seconds"}>

### rate_limit_duration

The window used for the `request_rate_limit_num` option See [Rate Limits](#rate-limits) for more info.


</Option>


<Option
  defaultValue={5}
  enumValues={null}
  examples={[5]}
  name={"rate_limit_num"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"int"}
  unit={null}>

### rate_limit_num

The maximum number of requests allowed within the `rate_limit_duration` window. See [Rate Limits](#rate-limits) for more info.


</Option>


<Option
  defaultValue={null}
  enumValues={null}
  examples={["us-east-1"]}
  name={"region"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  simple={true}
  type={"string"}
  unit={null}>

### region

The [AWS region][urls.aws_cw_logs_regions] of the target CloudWatch Logs stream resides.


</Option>


<Option
  defaultValue={5}
  enumValues={null}
  examples={[5]}
  name={"request_in_flight_limit"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"int"}
  unit={null}>

### request_in_flight_limit

The maximum number of in-flight requests allowed at any given time. See [Rate Limits](#rate-limits) for more info.


</Option>


<Option
  defaultValue={30}
  enumValues={null}
  examples={[30]}
  name={"request_timeout_secs"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"int"}
  unit={"seconds"}>

### request_timeout_secs

The maximum time a request can take before being aborted. It is highly recommended that you do not lower value below the service's internal timeout, as this could create orphaned requests, pile on retries, and result in deuplicate data downstream.


</Option>


<Option
  defaultValue={5}
  enumValues={null}
  examples={[5]}
  name={"retry_attempts"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"int"}
  unit={null}>

### retry_attempts

The maximum number of retries to make for failed requests. See [Retry Policy](#retry-policy) for more info.


</Option>


<Option
  defaultValue={5}
  enumValues={null}
  examples={[5]}
  name={"retry_backoff_secs"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"int"}
  unit={"seconds"}>

### retry_backoff_secs

The amount of time to wait before attempting a failed request again. See [Retry Policy](#retry-policy) for more info.


</Option>


<Option
  defaultValue={null}
  enumValues={null}
  examples={["{{ instance_id }}","%Y-%m-%d","stream-name"]}
  name={"stream_name"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  simple={true}
  type={"string"}
  unit={null}>

### stream_name

The [stream name][urls.aws_cw_logs_stream_name] of the target CloudWatch Logs stream. See [Partitioning](#partitioning) and [Template Syntax](#template-syntax) for more info.


</Option>


</Options>

## Input/Output

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
2. The [`credential_process` command][urls.aws_credential_process] in the AWS config file. (usually located at `~/.aws/config`)
3. The [AWS credentials file][urls.aws_credentials_file]. (usually located at `~/.aws/credentials`)
4. The [IAM instance profile][urls.iam_instance_profile]. (will only work if running on an EC2 instance with an instance profile/role)

If credentials are not found the [healtcheck](#healthchecks) will fail and an
error will be [logged][docs.monitoring#logs].

#### Obtaining an access key

In general, we recommend using instance profiles/roles whenever possible. In
cases where this is not possible you can generate an AWS access key for any user
within your AWS account. AWS provides a [detailed guide][urls.aws_access_keys] on
how to do this.

### Buffers & Batches

 
![][assets.sink-flow-partitioned]

The `aws_cloudwatch_logs` sink buffers & batches data as
shown in the diagram above. You'll notice that Vector treats these concepts
differently, instead of treating them as global concepts, Vector treats them
as sink specific concepts. This isolates sinks, ensuring services disruptions
are contained and [delivery guarantees][docs.guarantees] are honored.

*Batches* are flushed when 1 of 2 conditions are met:

1. The batch age meets or exceeds the configured `batch_timeout` (default: `1 seconds`).
2. The batch size meets or exceeds the configured `batch_size` (default: `1049000 bytes`).

*Buffers* are controlled via the [`buffer.*`](#buffer) options.

### Delivery Guarantee

This component offers an [**at least once** delivery guarantee][docs.guarantees#at-least-once-delivery]
if your [pipeline is configured to achieve this][docs.guarantees#at-least-once-delivery].

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration#environment-variables]
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
options and allows you to dynamically partition data on the fly.
You'll notice that Vector's [template sytax](#template-syntax) is supported
for these options, enabling you to use field values as the partition's key.

### Rate Limits

Vector offers a few levers to control the rate and volume of requests to the
downstream service. Start with the `rate_limit_duration` and `rate_limit_num`
options to ensure Vector does not exceed the specified number of requests in
the specified window. You can further control the pace at which this window is
saturated with the `request_in_flight_limit` option, which will guarantee no
more than the specified number of requests are in-flight at any given time.

Please note, Vector's defaults are carefully chosen and it should be rare that
you need to adjust these. If you found a good reason to do so please share it
with the Vector team by [opening an issie][urls.new_aws_cloudwatch_logs_sink_issue].

### Retry Policy

Vector will retry failed requests (status == `429`, >= `500`, and != `501`).
Other responses will _not_ be retried. You can control the number of retry
attempts and backoff rate with the `retry_attempts` and `retry_backoff_secs` options.

### Template Syntax

The `group_name` and `stream_name` options
support [Vector's template syntax][docs.configuration#template-syntax],
enabling dynamic values derived from the event's data. This syntax accepts
[strftime specifiers][urls.strftime_specifiers] as well as the
`{{ field_name }}` syntax for accessing event fields. For example:

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
[sinks.my_aws_cloudwatch_logs_sink_id]
  # ...
  group_name = "{{ file }}"
  group_name = "ec2/{{ instance_id }}"
  group_name = "group-name"
  # ...
```
{% endcode-tabs-item %}
{% endcode-tabs %}

You can read more about the complete syntax in the
[template syntax section][docs.configuration#template-syntax].

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring#logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open `aws_cloudwatch_logs_sink` issues][urls.aws_cloudwatch_logs_sink_issues].
2. If encountered a bug, please [file a bug report][urls.new_aws_cloudwatch_logs_sink_bug].
3. If encountered a missing feature, please [file a feature request][urls.new_aws_cloudwatch_logs_sink_enhancement].
4. If you need help, [join our chat/forum community][urls.vector_chat]. You can post a question and search previous questions.

## Resources

* [**Issues**][urls.aws_cloudwatch_logs_sink_issues] - [enhancements][urls.aws_cloudwatch_logs_sink_enhancements] - [bugs][urls.aws_cloudwatch_logs_sink_bugs]
* [**Source code**][urls.aws_cloudwatch_logs_sink_source]
* [**Service Limits**][urls.aws_cw_logs_service_limits]


[assets.sink-flow-partitioned]: ../../../assets/sink-flow-partitioned.svg
[docs.configuration#environment-variables]: ../../../usage/configuration#environment-variables
[docs.configuration#template-syntax]: ../../../usage/configuration#template-syntax
[docs.data-model.log]: ../../../about/data-model/log.md
[docs.event]: ../../../setup/getting-started/sending-your-first-event.md
[docs.guarantees#at-least-once-delivery]: ../../../about/guarantees.md#at-least-once-delivery
[docs.guarantees]: ../../../about/guarantees.md
[docs.monitoring#logs]: ../../../usage/administration/monitoring.md#logs
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[urls.aws_access_keys]: https://docs.aws.amazon.com/IAM/latest/UserGuide/id_credentials_access-keys.html
[urls.aws_cloudwatch_logs_sink_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22sink%3A+aws_cloudwatch_logs%22+label%3A%22Type%3A+bug%22
[urls.aws_cloudwatch_logs_sink_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22sink%3A+aws_cloudwatch_logs%22+label%3A%22Type%3A+enhancement%22
[urls.aws_cloudwatch_logs_sink_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22sink%3A+aws_cloudwatch_logs%22
[urls.aws_cloudwatch_logs_sink_source]: https://github.com/timberio/vector/blob/master/src/sinks/aws_cloudwatch_logs/mod.rs
[urls.aws_credential_process]: https://docs.aws.amazon.com/cli/latest/userguide/cli-configure-sourcing-external.html
[urls.aws_credentials_file]: https://docs.aws.amazon.com/cli/latest/userguide/cli-configure-files.html
[urls.aws_cw_logs]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/WhatIsCloudWatchLogs.html
[urls.aws_cw_logs_group_name]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/Working-with-log-groups-and-streams.html
[urls.aws_cw_logs_regions]: https://docs.aws.amazon.com/general/latest/gr/rande.html#cwl_region
[urls.aws_cw_logs_service_limits]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/cloudwatch_limits_cwl.html
[urls.aws_cw_logs_stream_name]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/Working-with-log-groups-and-streams.html
[urls.iam_instance_profile]: https://docs.aws.amazon.com/IAM/latest/UserGuide/id_roles_use_switch-role-ec2_instance-profiles.html
[urls.new_aws_cloudwatch_logs_sink_bug]: https://github.com/timberio/vector/issues/new?labels=sink%3A+aws_cloudwatch_logs&labels=Type%3A+bug
[urls.new_aws_cloudwatch_logs_sink_enhancement]: https://github.com/timberio/vector/issues/new?labels=sink%3A+aws_cloudwatch_logs&labels=Type%3A+enhancement
[urls.new_aws_cloudwatch_logs_sink_issue]: https://github.com/timberio/vector/issues/new?labels=sink%3A+aws_cloudwatch_logs
[urls.strftime_specifiers]: https://docs.rs/chrono/0.3.1/chrono/format/strftime/index.html
[urls.vector_chat]: https://chat.vector.dev
