---
description: Stream log events to AWS' CloudWatch Logs service
---

# aws\_cloudwatch\_logs sink

![](../../../.gitbook/assets/cloudwatch-logs-sink.svg)



The `aws_cloudwatch_logs` sink streams [`log`](../../../about/data-model.md#log) events to the [AWS CloudWatch Logs](https://aws.amazon.com/cloudwatch/) service via the [`PutLogEvents` API endpoint](https://docs.aws.amazon.com/AmazonCloudWatchLogs/latest/APIReference/API_PutLogEvents.html).

## Example

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
[sinks.<sink-id>]
    # REQUIRED
    inputs      = ["{<source-id> | <transform-id>}", [ ... ]]
    type        = "aws_cloudwatch_logs"
    region      = "<region>"
    group_name  = "<group-name>"
    stream_name = "<stream-name>"
    
    # OPTIONAL - Generic
    encoding = "json"

    # OPTIONAL - Batch
    batch_size    = 1048576 # 1mib, max allowed
    batch_timeout = 1 # 1 second
    
    # OPTIONAL - Request
    request_in_flight_limit          = 5
    request_timeout_secs             = 60
    request_rate_limit_duration_secs = 1
    request_rate_limit_num           = 5
    request_retry_attempts           = 5
    request_retry_backoff_secs       = 1
    
    # OPTIONAL - Buffer
    [sinks.<sink-id>.buffer]
        type      = "memory"
        num_items = 1000
        when_full = "block"
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Options

<table>
  <thead>
    <tr>
      <th style="text-align:left">Key</th>
      <th style="text-align:center">Type</th>
      <th style="text-align:left">Description</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td style="text-align:left"><b>REQUIRED</b>
      </td>
      <td style="text-align:center"></td>
      <td style="text-align:left"></td>
    </tr>
    <tr>
      <td style="text-align:left"><code>region</code>
      </td>
      <td style="text-align:center"><code>string</code>
      </td>
      <td style="text-align:left">
        <p>The <a href="https://docs.aws.amazon.com/general/latest/gr/rande.html#cw_region">AWS region</a> the
          CloudWatch Logs stream resides.</p>
        <p><code>example: &quot;us-east-1&quot;</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><code>group_name</code>
      </td>
      <td style="text-align:center"><code>string</code>
      </td>
      <td style="text-align:left">
        <p>The <a href="https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/Working-with-log-groups-and-streams.html">group name</a> of
          the CloudWatch Logs stream.</p>
        <p><code>example: &quot;log-group&quot;</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><code>stream_name</code>
      </td>
      <td style="text-align:center"><code>string</code>
      </td>
      <td style="text-align:left">
        <p>The <a href="https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/Working-with-log-groups-and-streams.html">stream name</a> of
          the CloudWatch Logs stream.</p>
        <p><code>example: &quot;nginx-stream&quot;</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><b>OPTIONAL </b>- Generic</td>
      <td style="text-align:center"></td>
      <td style="text-align:left"></td>
    </tr>
    <tr>
      <td style="text-align:left"><code>encoding</code>
      </td>
      <td style="text-align:center"><code>string</code>
      </td>
      <td style="text-align:left">
        <p>The encoding format used to serialize the event before flushing. Must
          be one of <code>text</code> or <code>json</code>. See <a href="aws_cloudwatch_logs.md#encoding">Encoding</a> below.</p>
        <p><code>no default</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><b>OPTIONAL</b> - Batch</td>
      <td style="text-align:center"></td>
      <td style="text-align:left"></td>
    </tr>
    <tr>
      <td style="text-align:left"><code>batch_size</code>
      </td>
      <td style="text-align:center"><code>int</code>
      </td>
      <td style="text-align:left">
        <p>The maximum size of a <a href="./#batches">batch</a>, in bytes, before
          it is flushed. Cannot exceed <code>1048576</code> as per the <a href="https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/cloudwatch_limits_cwl.html">service limits</a>.</p>
        <p><code>default: 1048576</code> (max allowed)</p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><code>batch_timeout</code>
      </td>
      <td style="text-align:center"><code>int</code>
      </td>
      <td style="text-align:left">
        <p>The maximum age of a <a href="./#batches">batch</a>, in seconds, before
          it is flushed. See <a href="aws_cloudwatch_logs.md#batching">Batching</a> below
          for more info.</p>
        <p><code>default: 1</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><b>OPTIONAL </b>- Request</td>
      <td style="text-align:center"></td>
      <td style="text-align:left"></td>
    </tr>
    <tr>
      <td style="text-align:left"><code>request_in_flight_limit</code>
      </td>
      <td style="text-align:center"><code>int</code>
      </td>
      <td style="text-align:left">
        <p>The maximum number of in-flight requests allowed at any given time. See
          <a
          href="aws_cloudwatch_logs.md#rate-limiting">Rate Limiting</a>below for more info.</p>
        <p><code>default: 5</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><code>request_timeout_secs</code>
      </td>
      <td style="text-align:center"><code>int</code>
      </td>
      <td style="text-align:left">
        <p>The maximum time a request can take before being aborted. See <a href="aws_cloudwatch_logs.md#timeouts">Timeouts</a> below
          for more info.</p>
        <p><code>default: 60</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><code>rate_limit_duration</code>
      </td>
      <td style="text-align:center"><code>int</code>
      </td>
      <td style="text-align:left">
        <p>The window, in seconds, used for the <code>request_rate_limit_num</code> option.
          See <a href="aws_cloudwatch_logs.md#rate-limiting">Rate Limiting</a> below
          for more info.</p>
        <p><code>default: 1</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><code>rate_limit_num</code>
      </td>
      <td style="text-align:center"><code>int</code>
      </td>
      <td style="text-align:left">
        <p>The maximum number of requests allowed within the <code>rate_limit_duration</code> window.
          See <a href="aws_cloudwatch_logs.md#rate-limiting">Rate Limiting</a> below
          for more info.</p>
        <p><code>default: 5</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><code>retry_attempts</code>
      </td>
      <td style="text-align:center"><code>int</code>
      </td>
      <td style="text-align:left">
        <p>The maximum number of retries to make for failed requests. See <a href="aws_cloudwatch_logs.md#retry-policy">Retry Policy</a> below
          for more info.</p>
        <p><code>default: 5</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><code>retry_backoff_secs</code>
      </td>
      <td style="text-align:center"><code>int</code>
      </td>
      <td style="text-align:left">
        <p>The amount of time, in seconds, to wait before attempting a failed request
          again. See <a href="aws_cloudwatch_logs.md#retry-policy">Retry Policy</a> below
          for more info.</p>
        <p><code>default: 1</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><b>OPTIONAL</b> - Buffer</td>
      <td style="text-align:center"></td>
      <td style="text-align:left"></td>
    </tr>
    <tr>
      <td style="text-align:left">&lt;code&gt;&lt;/code&gt;<a href="buffer.md"><code>buffer.*</code></a>&lt;code&gt;&lt;/code&gt;</td>
      <td
      style="text-align:center"><code>table</code>
        </td>
        <td style="text-align:left">A table that configures the sink specific buffer. See the <a href="buffer.md">*.buffer document</a>.</td>
    </tr>
  </tbody>
</table>## Tuning

The `aws_cloudwatch_logs` sink should not require any tuning unless your AWS account has been configured with special rate limits. The Vector defaults align with the [documented AWS CloudWatch Logs limits](https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/cloudwatch_limits_cwl.html).

## Input

The `aws_cloudwatch_logs` sink accepts [`log`](../../../about/data-model.md#log) events only from a [source](../sources/) or [transform](../transforms/).

## Output

The `aws_cloudwatch_logs` sink batches events up to the `batch_size` or `batch_timeout` [options](aws_cloudwatch_logs.md#options). When flushed, Vector will produce an HTTP request to the CloudWatch Logs [`PutLogEvents` API endpoint](https://docs.aws.amazon.com/AmazonCloudWatchLogs/latest/APIReference/API_PutLogEvents.html). The encoding is dictated by the `encoding` option \(see [Encoding](aws_cloudwatch_logs.md#encoding)\), each encoding is demonstrated below:

{% code-tabs %}
{% code-tabs-item title="json" %}
```http
POST / HTTP/1.1
Host: logs.<region>.<domain>
X-Amz-Date: <DATE>
Accept: application/json
Content-Type: application/x-amz-json-1.1
Content-Length: <PayloadSizeBytes>
Connection: Keep-Alive
X-Amz-Target: Logs_20140328.PutLogEvents
{
  "logGroupName": "<group_name>",
  "logStreamName": "<stream_name>",
  "logEvents": [
    {
      "timestamp": 1396035378988, 
      "message": "{\"timestamp\": 1557932537, \"message\": \"GET /roi/evolve/embrace/transparent\", \"host\": \"Stracke8362\", \"process_id\": 914, \"remote_addr\": \"30.163.82.140\", \"response_code\": 504, \"bytes\": 29763}"
    }, 
    {
      "timestamp": 1396035378988, 
      "message": "{\"timestamp\": 1557933548, \"message\": \"PUT /value-added/b2b\", \"host\": \"Wiza2458\", \"process_id\": 775, \"remote_addr\": \"30.163.82.140\", \"response_code\": 503, \"bytes\": 9468}"
    }, 
    {
      "timestamp": 1396035378989, 
      "message": "{\"timestamp\": 1557933742, \"message\": \"DELETE /reinvent/interfaces\", \"host\": \"Herman3087\", \"process_id\": 775, \"remote_addr\": \"43.246.221.247\", \"response_code\": 503, \"bytes\": 9700}"
    }
  ]
}
```
{% endcode-tabs-item %}

{% code-tabs-item title="text" %}
```http
POST / HTTP/1.1
Host: logs.<region>.<domain>
X-Amz-Date: <DATE>
Accept: application/json
Content-Type: application/x-amz-json-1.1
Content-Length: <PayloadSizeBytes>
Connection: Keep-Alive
X-Amz-Target: Logs_20140328.PutLogEvents
{
  "logGroupName": "<group_name>",
  "logStreamName": "<stream_name>",
  "logEvents": [
    {
      "timestamp": 1396035378988, 
      "message": "30.163.82.140 - Stracke8362 914 [2019-05-15T11:17:57-04:00] \"GET /roi/evolve/embrace/transparent\" 504 29763"
    }, 
    {
      "timestamp": 1396035378988, 
      "message": "190.218.92.219 - Wiza2458 775 [2019-05-15T11:17:57-04:00] \"PUT /value-added/b2b\" 503 9468"
    }, 
    {
      "timestamp": 1396035378989, 
      "message": "43.246.221.247 - Herman3087 294 [2019-05-15T11:17:57-04:00] \"DELETE /reinvent/interfaces\" 503 9700"
    }
  ]
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

The above examples are purposefully small for demonstration purposes. You can read more about encoding in the [Encoding](aws_cloudwatch_logs.md#encoding) section.

## How It Works

### Authentication

Vector checks for AWS credentials in the following order:

1. Environment variables `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY`
2. \`\`[`credential_process` command](https://docs.aws.amazon.com/cli/latest/userguide/cli-configure-sourcing-external.html) in the AWS config file, usually located at `~/.aws/config`.
3. [AWS credentials file](https://docs.aws.amazon.com/cli/latest/userguide/cli-configure-files.html), usually located at `~/.aws/credentials`.
4. [IAM instance profile](https://docs.aws.amazon.com/IAM/latest/UserGuide/id_roles_use_switch-role-ec2_instance-profiles.html). Will only work if running on an EC2 instance with an instance profile/role.

If credentials are not found the [healtcheck](aws_cloudwatch_logs.md#healthchecks) will fail and an error will be logged.

#### Obtaining an access key

In general, we recommend using instance profiles/roles whenever possible. In cases where this is not possible you can generate an AWS access key for any user within your AWS account. AWS provides a [detailed guide](https://docs.aws.amazon.com/IAM/latest/UserGuide/id_credentials_access-keys.html) on how to do this.

### Batching

AWS CloudWatch Logs is designed for rapid flushing \([up to 5 requests per second per stream](https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/cloudwatch_limits_cwl.html)\). Therefore, Vector, by default, flushes every 1 second to make data available quickly. This can be changed by adjusting the `batch_timeout` option. Keep in mind that CloudWatch Logs will only accept payloads up to `1048576` bytes, \(controlled by the `batch_size` option\), which may trigger a flush as well.

### Encoding

The `aws_cloudwatch_logs` sink encodes [events](../../../about/data-model.md#event) before flushing them to CloudWatch. CloudWatch log events include a [single `Message` key](https://docs.aws.amazon.com/AmazonCloudWatchLogs/latest/APIReference/API_InputLogEvent.html#CWL-Type-InputLogEvent-message) that accepts a blob of data. This blob is encoded via the `encoding` option. Each encoding type is described in more detail below.

#### text

When encoding [events](../../../about/data-model.md#event) to `text` Vector will use the raw value of the `"message"` field.

#### json

When encoding events to `json`, Vector will encode the entire [event](../../../about/concepts.md#events) to JSON.

#### nil \(default\)

If left unspecified, Vector will dynamically choose the appropriate encoding. If an [event](../../../about/concepts.md#events) is explicitly structured then it will be encoded as `json`, if it is not, it will be encoded as `text`. This provides the path of least surprise for different [pipelines](../../../about/concepts.md#pipelines).

For example, take the simple [`tcp` source](../sources/tcp.md) to `aws_cloudwatch_logs` sink [pipeline](../../../about/concepts.md#pipelines). The data coming from the `tcp` source is raw text lines, therefore, if you connected it directly to this sink you would expect to see those same raw text lines. Alternatively, if you parsed that data with a [transform](../transforms/), you would expect to see encoded structured data.

### Healthchecks

Vector will perform a simple health check against the CloudWatch Logs service before initializing the sink. This ensures that the service is reachable. You can require this check with the [`--require-healthy` flag](../../administration/starting.md#options) upon [starting](../../administration/starting.md) Vector.

### Rate Limiting

Vector offers a few levers to control the rate and volume of requests. Please note, CloudWatch implements its [own rate limits](https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/cloudwatch_limits_cwl.html) and Vector's defaults are inline with those limits. If you need to change these anyway, then we recommend starting with the `rate_limit_duration` and `rate_limit_num` options to ensure Vector does not exceed the specified number of requests in the specified window. You can further control the pace at which this window is saturated with the `request_in_flight_limit` option, which will guarantee no more than the specified number of requests are in-flight at any given time.

### Retry Policy

Vector will retry failed requests \(status `== 429`, `>= 500`, and `!= 501`\). Other responses will not be retried. You can control the number of retry attempts and backoff rate with the `retry_attempts` and `retry_backoff_secs` options.

#### Partial Failures

CloudWatch Logs will only reject events if they fall outside of the acceptable ingest window. You can see the exact failure reasons in the [CloudWatch Logs docs](https://docs.aws.amazon.com/AmazonCloudWatchLogs/latest/APIReference/API_RejectedLogEventsInfo.html). Vector will not attempt to retry individual records since retrying these records would continually fail. If losing this data is unacceptable we recommend pairing your `aws_cloudwatch_logs` sink with an archiving sink \(such as the [`aws_s3` sink](aws_s3.md)\) that does not have this restriction.

### Service Limits

This sink flushes to the `PutLogEvents` API endpoint. All limitations can be found on the [associated documentation page](https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/cloudwatch_limits_cwl.html).

### Timeouts

The default `request_timeout_secs` is based on CloudWatch's service timeout as well as how AWS configures its libraries. It is highly recommended that you do not lower this, as this could create orphaned requests and pile on retries.

## Resources

* [Source code](https://github.com/timberio/vector/blob/master/src/sinks/cloudwatch_logs.rs)
* [Issues](https://github.com/timberio/vector/labels/Sink%3A%20AWS%20CW%20Logs)
* [Vendor Website](https://aws.amazon.com/cloudwatch/)

