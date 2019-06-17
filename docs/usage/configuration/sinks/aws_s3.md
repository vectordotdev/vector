---
description: Batch and flush log events to AWS' S3 service
---

# aws\_s3 sink

![](../../../.gitbook/assets/s3-sink.svg)

The `s3` sink batches and flushes [`log`](../../../about/data-model.md#log) events to an [AWS S3 bucket](https://docs.aws.amazon.com/AmazonS3/latest/dev/UsingBucket.html) via the [`PutObject` endpoint](https://docs.aws.amazon.com/AmazonS3/latest/API/RESTObjectPUT.html).

## Example

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
[sinks.<sink-id>]
    # REQUIRED
    inputs     = ["{<source-id> | <transform-id>}", [ ... ]]
    type       = "s3"
    region     = "<region>"
    bucket     = "<bucket-name>"
    encoding   = "ndjson"
    
    # OPTIONAL - Generic
    compression   = "gzip"
    key_prefix    = "date=%F/"
    
    # OPTIONAL - Batch
    batch_size = 10490000 # 10mib
    batch_timeout = 300 # 5 minutes
    
    # OPTIONAL - Request
    request_in_flight_limit          = 25
    request_timeout_secs             = 60
    request_rate_limit_duration_secs = 1
    request_rate_limit_num           = 25
    request_retry_attempts           = 5
    request_retry_backoff_secs       = 5
    
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
      <td style="text-align:left">The <a href="https://docs.aws.amazon.com/general/latest/gr/rande.html#s3_region">AWS region</a> the
        S3 bucket resides.
        <br /><code>example: &quot;us-east-1&quot;</code>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><code>bucket</code>
      </td>
      <td style="text-align:center"><code>string</code>
      </td>
      <td style="text-align:left">The S3 bucket name. Do <em>not</em> include the <code>s3://</code> or a trailing <code>/</code>.
        <br
        /><code>example: &quot;my-bucket-name&quot;</code>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><code>encoding</code>
      </td>
      <td style="text-align:center"><code>string</code>
      </td>
      <td style="text-align:left">
        <p>The encoding format used to serialize the events before flushing. See
          <a
          href="aws_s3.md#encoding">Encoding</a>below for more info.</p>
        <p><code>enum: &quot;text&quot;, &quot;ndjson&quot;</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><b>OPTIONAL </b>- Generic</td>
      <td style="text-align:center"></td>
      <td style="text-align:left"></td>
    </tr>
    <tr>
      <td style="text-align:left"><code>compression</code>
      </td>
      <td style="text-align:center"><code>string</code>
      </td>
      <td style="text-align:left">
        <p>The compression type to use before flushing data. See <a href="aws_s3.md#compression">Compression</a> for
          more info.
          <br /><code>enum: &quot;gzip&quot;, &quot;ndjson&quot;</code>
        </p>
        <p><code>default: &quot;gzip&quot;</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><code>key_prefix</code>
      </td>
      <td style="text-align:center"><code>string</code>
      </td>
      <td style="text-align:left">
        <p>The S3 object key prefix. This is used to namespace your objects. This
          supports <a href="https://docs.rs/chrono/0.3.1/chrono/format/strftime/index.html"><code>strftime</code> specifiers</a>.
          See Partitioning for more info.</p>
        <p><code>default: &quot;date=%F&quot;</code>
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
        <p>The maximum size of the <a href="./#batches">batch</a>, in bytes, before
          it is flushed. See <a href="aws_s3.md#batching">Batching</a> below.</p>
        <p><code>default: 10490000</code> (1mib)</p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><code>batch_timeout</code>
      </td>
      <td style="text-align:center"><code>int</code>
      </td>
      <td style="text-align:left">
        <p>The maximum age of a <a href="./#batches">batch</a>, in seconds, before
          it is flushed. See <a href="aws_s3.md#batching">Batching</a> below.</p>
        <p><code>default: 300</code> (5 min)</p>
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
          href="http.md#rate-limiting">Rate Limiting</a>below for more info.</p>
        <p><code>default: 25</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><code>request_timeout_secs</code>
      </td>
      <td style="text-align:center"><code>int</code>
      </td>
      <td style="text-align:left">
        <p>The maximum time a request can take before being aborted. See <a href="http.md#timeouts">Timeouts</a> below
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
          See <a href="http.md#rate-limiting">Rate Limiting</a> below for more info.</p>
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
          See <a href="http.md#rate-limiting">Rate Limiting</a> below for more info.</p>
        <p><code>default: 25</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><code>retry_attempts</code>
      </td>
      <td style="text-align:center"><code>int</code>
      </td>
      <td style="text-align:left">
        <p>The maximum number of retries to make for failed requests. See <a href="aws_s3.md#retry-policy">Retry Policy</a> below
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
          again. See <a href="aws_s3.md#retry-policy">Retry Policy</a> below for more
          info.</p>
        <p><code>default: 1</code>
        </p>
      </td>
    </tr>
  </tbody>
</table>## Tuning

Typically the `aws_s3` sink does not require tuning since the intent is to batch data over a period of time and flush it at longer intervals. If your use case deviates from this you'll want to adjust the `batch_size`, `batch_timeout` , `request_in_flight_limit`, and `rate_limit_num` options to suit your needs. AWS S3 has very liberal limits \(up to 3500 writes per second\).

## Input

The `aws_s3` sink accepts [`log`](../../../about/data-model.md#log) events only from a [source](../sources/) or [transform](../transforms/).

## Output

The `aws_s3` sink batches events up to the `batch_size` or `batch_timeout` [options](aws_s3.md#options). When flushed, Vector will produce an HTTP request to the S3 [`PutObject` endpoint](https://docs.aws.amazon.com/AmazonS3/latest/API/RESTObjectPUT.html). The encoding is dictated by the `encoding` option \(see [Encoding](aws_s3.md#encoding)\), each encoding is demonstrated below:

{% code-tabs %}
{% code-tabs-item title="ndjson" %}
```http
PUT <key_prefix>/<filename> HTTP/1.1
Host: <bucket>.s3.amazonaws.com
Content-Type: application/x-ndjson
Content-Length: 711

{"timestamp": 1557932537, "message": "GET /roi/evolve/embrace/transparent", "host": "Stracke8362", "process_id": 914, "remote_addr": "30.163.82.140", "response_code": 504, "bytes": 29763} 
{"timestamp": 1557933548, "message": "PUT /value-added/b2b", "host": "Wiza2458", "process_id": 775, "remote_addr": "30.163.82.140", "response_code": 503, "bytes": 9468}
{"timestamp": 1557933742, "message": "DELETE /reinvent/interfaces", "host": "Herman3087", "process_id": 775, "remote_addr": "43.246.221.247", "response_code": 503, "bytes": 9700}
```
{% endcode-tabs-item %}

{% code-tabs-item title="text" %}
```http
PUT <key_prefix>/<filename> HTTP/1.1
Host: <bucket>.s3.amazonaws.com
Content-Type: text/plain
Content-Length: 645

30.163.82.140 - Stracke8362 914 [2019-05-15T11:17:57-04:00] "GET /roi/evolve/embrace/transparent" 504 29763
190.218.92.219 - Wiza2458 775 [2019-05-15T11:17:57-04:00] "PUT /value-added/b2b" 503 9468
43.246.221.247 - Herman3087 294 [2019-05-15T11:17:57-04:00] "DELETE /reinvent/interfaces" 503 9700
```
{% endcode-tabs-item %}
{% endcode-tabs %}

The above examples are purposefully small for demonstration purposes. You can read more about encoding in the [Encoding](aws_s3.md#encoding) section.

## How It Works

### Authentication

Vector checks for AWS credentials in the following order:

1. Environment variables `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY`
2. \`\`[`credential_process` command](https://docs.aws.amazon.com/cli/latest/userguide/cli-configure-sourcing-external.html) in the AWS config file, usually located at `~/.aws/config`.
3. [AWS credentials file](https://docs.aws.amazon.com/cli/latest/userguide/cli-configure-files.html). Usually located at `~/.aws/credentials`.
4. [IAM instance profile](https://docs.aws.amazon.com/IAM/latest/UserGuide/id_roles_use_switch-role-ec2_instance-profiles.html). Will only work if running on an EC2 instance with an instance profile/role.

If credentials are not found the [healtcheck](aws_cloudwatch_logs.md#healthchecks) will fail and an error will be logged.

#### Obtaining an access key

In general, we recommend using instance profiles/roles whenever possible. In cases where this is not possible you can generate an AWS access key for any user within your AWS account. AWS provides a [detailed guide](https://docs.aws.amazon.com/IAM/latest/UserGuide/id_credentials_access-keys.html) on how to do this.

### Batching

It is not recommended to write rapid small files to S3 since this reduces the compression benefit and can be [expensive](https://aws.amazon.com/s3/pricing/). As such, Vector offers 2 thresholds that you can set to trigger a flush: the `batch_size` and `batch_timeout` will trigger a flush if the batch exceeds a byte size or has an age that exceeds the specified age, respectively.

### Compression

By default Vector uses [Gzip compression](https://en.wikipedia.org/wiki/Gzip). You can chance the compression type via the `compression` option. In general, we highly recommend keeping `gzip` compression on as this will reduce network activity and space used on S3. On average we see a 90% reduction on both of these data points.

### Defaults

This sinks defaults are optimized for the typical S3 archiving use case, optimizing for cost reduction.

### Encoding

The `aws_s3` sink encodes [events](../../../about/data-model.md#event) before flushing them to S3. Because S3 objects are just a blob of data Vector can encode that data in different formats via the `encoding` option. Each encoding type is described in more detail below.

#### text

When encoding [events](../../../about/data-model.md#event) to `text` Vector will use the raw value of the `"message"` field and new line delimit \(the `0xA` byte\) the contents.

#### ndjson

When encoding events to `ndjson`, Vector will encode the object as [ndjson](http://ndjson.org/), which means the entire [event](../../../about/concepts.md#events) is JSON encoded and then new line \(the `0xA` byte\) delimited.

### Healthchecks

Vector will perform a simple health check against the S3 service before initializing the sink. This ensures that the service is reachable.

### Object Naming

Aside from [partitioning](aws_s3.md#partitioning), Vector automatically names your objects in the following format.

### Partitioning

Partitioning on S3 is achieved through the `key_prefix` setting. This setting supports [strftime specifiers](https://docs.rs/chrono/0.3.1/chrono/format/strftime/index.html), allowing you to partition by the [event's `timestamp`](../../../about/data-model.md#default-schema).

For example, setting `key_prefix` to `date=%F/` produces a key prefix of `date=2019-05-02/` \(reflecting the event's `timestamp` date\), which effectively partitions your data by date.

#### Trailing Slashes

{% hint style="warning" %}
It's important that you end your `key_prefix` in a `/` if you want create S3 "folders". Otherwise Vector will simply prefix your object names.
{% endhint %}

### Rate Limiting

Vector offers a few levers to control the rate and volume of requests. We recommend starting with the `rate_limit_duration` and `rate_limit_num` options to ensure Vector does not exceed the specified number of requests in the specified window. You can further control the pace at which this window is saturated with the `request_in_flight_limit` option, which will guarantee no more than the specified number of requests are in-flight at any given time.

### Retry Policy

Vector will retry failed requests \(status `== 429`, `>= 500`, and `!= 501`\). Other responses will not be retried. You can control the number of retry attempts and backoff rate with the `retry_attempts` and `retry_backoff_secs` options.

### Timeouts

The default `request_timeout_secs` is based on S3's service timeout as well as how AWS configures its libraries. It is highly recommended that you do not lower this, as this could create orphaned requests and pile on retries.

## Resources

* [Source code](https://github.com/timberio/vector/blob/master/src/sinks/s3.rs)
* [Issues](https://github.com/timberio/vector/labels/Sink%3A%20AWS%20S3)
* [Vendor Website](https://aws.amazon.com/s3/)

