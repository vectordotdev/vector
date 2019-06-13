---
description: Batch and flush log and metric events to an Elasticsearch cluster
---

# elasticsearch sink

![](../../../.gitbook/assets/elasticsearch-sink.svg)

The `elasticsearch` sink batches and flushes [`log`](../../../about/data-model.md#log) and [`metric`](../../../about/data-model.md#metric) events to [Elasticsearch](https://github.com/elastic/elasticsearch).

## Example

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
[sinks.<sink-id>]
    # REQUIRED
    inputs      = ["{<source-id> | <transform-id>}", [ ... ]]
    type        = "elasticsearch"
    host        = "10.23.22.112"
    
    # Optional - Generic
    index       = "vector-%F"
    doc_type    = "_doc" # only required for ES <= 6.X  

    # OPTIONAL - Batch
    batch_size    = 1048576 # 10mib
    batch_timeout = 1 # 1 second
    
    # OPTIONAL - Request
    request_in_flight_limit          = 5
    request_timeout_secs             = 60
    request_rate_limit_duration_secs = 1
    request_rate_limit_num           = 5
    request_retry_attempts           = 5
    request_retry_backoff_secs       = 1
    
    # OPTIONAL - Authentication
    [sinks.<sink-id>.authentication]
        type = "aws"
    
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
      <td style="text-align:left"><code>host</code>
      </td>
      <td style="text-align:center"><code>string</code>
      </td>
      <td style="text-align:left">The host of your Elasticsearch cluster.
        <br /><code>example: &quot;10.24.32.122:9000&quot;</code>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><b>OPTIONAL</b> - Generic</td>
      <td style="text-align:center"></td>
      <td style="text-align:left"></td>
    </tr>
    <tr>
      <td style="text-align:left"><code>index</code>
      </td>
      <td style="text-align:center"><code>string</code>
      </td>
      <td style="text-align:left">
        <p>Index name to write events to. This supports <a href="https://docs.rs/chrono/0.3.1/chrono/format/strftime/index.html">`strftime` specifiers</a>.
          See <a href="elasticsearch.md#index-naming">Index Naming</a> for more info.</p>
        <p><code>default: &quot;vector-%F&quot;</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><code>doc_type</code>
      </td>
      <td style="text-align:center"><code>string</code>
      </td>
      <td style="text-align:left">
        <p>The <code>doc_type</code> for your Elasticsearch data. This is only relevant
          for Elasticsearch &lt;= 6.X. If you are using &gt;= 7.0 you do not need
          to set this option since Elasticsearch has removed it.</p>
        <p><code>default: &quot;_doc&quot;</code>
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
          it is flushed. Cannot exceed <code>1048576</code> as per the <a href="https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/cloudwatch_limits_cwl.html">service limits</a>.
          See <a href="elasticsearch.md#batching">Batching</a> below for more info.</p>
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
          it is flushed. See <a href="elasticsearch.md#batching">Batching</a> below
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
          href="elasticsearch.md#rate-limiting">Rate Limiting</a>below for more info.</p>
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
        <p>The maximum time a request can take before being aborted. See <a href="elasticsearch.md#timeouts">Timeouts</a> below
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
          See <a href="elasticsearch.md#rate-limiting">Rate Limiting</a> below for
          more info.</p>
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
          See <a href="elasticsearch.md#rate-limiting">Rate Limiting</a> below for
          more info.</p>
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
        <p>The maximum number of retries to make for failed requests. See <a href="elasticsearch.md#retries">Retries</a> below
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
          again. See <a href="elasticsearch.md#retries">Retries</a> below for more
          info.</p>
        <p><code>default: 1</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><b>OPTIONAL</b> - Authentication</td>
      <td style="text-align:center"></td>
      <td style="text-align:left"></td>
    </tr>
    <tr>
      <td style="text-align:left"><code>auth.type</code>
      </td>
      <td style="text-align:center"><code>string</code>
      </td>
      <td style="text-align:left">
        <p>The type of authentication to implement. See <a href="elasticsearch.md#authentication">Authentication</a> for
          more info.</p>
        <p><code>enum: &quot;aws&quot;, &quot;x-pack&quot;</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><code>auth.username</code>
      </td>
      <td style="text-align:center"><code>string</code>
      </td>
      <td style="text-align:left">
        <p>X-Pack username. Only relevant if <code>auth.type</code> is <code>x-pack</code>.</p>
        <p><code>example: &quot;es_user&quot;</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><code>auth.password</code>
      </td>
      <td style="text-align:center"><code>string</code>
      </td>
      <td style="text-align:left">
        <p>X-Pack password. Only relevant if <code>auth.type</code> is <code>x-pack</code>.</p>
        <p><code>example: &quot;es_password&quot;</code>
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

Because Elasticsearch clusters come in many sizes, you'll want to adjust the follow settings to align with your throughput limits:

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
batch_size = 1048576 # 10mib
request_in_flight_limit = 5
request_rate_limit_num = 5
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Input

The `elasticsearch` sink accepts both [`log`](../../../about/data-model.md#log) and [`metric`](../../../about/data-model.md#metric) events from a [source](../sources/) or [transform](../transforms/).

## Output

The `elasticsearch` sink batches [events](../../../about/data-model.md#event) up to the `batch_size` or `batch_timeout` [options](elasticsearch.md#options). When flushed, Vector will produce an HTTP request to the Elasticsearch [Bulk endpoint](https://www.elastic.co/guide/en/elasticsearch/reference/current/docs-bulk.html):

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

### Authentication

#### AWS Authentication

If `auth.type` is set to `aws` then Vector will authenticate requests to Elasticsearch in the following order:

1. Environment variables `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY`
2. \`\`[`credential_process` command](https://docs.aws.amazon.com/cli/latest/userguide/cli-configure-sourcing-external.html) in the AWS config file, usually located at `~/.aws/config`.
3. [AWS credentials file](https://docs.aws.amazon.com/cli/latest/userguide/cli-configure-files.html), usually located at `~/.aws/credentials`.
4. [IAM instance profile](https://docs.aws.amazon.com/IAM/latest/UserGuide/id_roles_use_switch-role-ec2_instance-profiles.html). Will only work if running on an EC2 instance with an instance profile/role.

If credentials are not found the [healtcheck](elasticsearch.md#healthchecks) will fail and an error will be logged.

#### X-Pack Authentication

If you've enabled x-pack security you can set `auth.type` to `x-pack`:

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
[sinks.<sink-id>]
  # ...
  
  [sinks.<sink-id>.auth]
    type     = "x-pack"
    user     = "<user>"
    password = "<password>"
```
{% endcode-tabs-item %}
{% endcode-tabs %}

If credentials are not found the [healtcheck](elasticsearch.md#healthchecks) will fail and an error will be logged.

### Batching

### Encoding

All [events](../../../about/data-model.md#event) are encoded as JSON, regardless if the event has been structured or not. This is due to the fact that Elasticsearch is a document store and expects all documents to be provided as JSON.

### Healthchecks

Vector will perform a simple health check before initializing the sink. This ensures that the service is reachable. You can require this check with the [`--require-healthy` flag](../../administration/starting.md#options) upon [starting](../../administration/starting.md) Vector.

### Index Naming

Vector supports dynamic index names through [strftime specificiers](https://docs.rs/chrono/0.3.1/chrono/format/strftime/index.html). This allows you to use the [event `timestamp`](../../../about/data-model.md#default-schema) within the index name, creating time partitioned indices. This is highly recommended for the logging use case since it allows for easy data pruning by simply deleting old indices.

For example, when the `index` setting is set to `vector-%Y-%m-%d` vector will created indexes with names like `vector-2019-05-04`, `vector-2019-05-05`, and so on. The date values are derived from the [event's timestamp](../../../about/data-model.md#default-schema).

### Nested Documents

Vector will explode events into nested documents before writing them to Elasticsearch. Vector assumes keys with a `.` delimit nested fields. You can read more about how Vector handles nested documents in the [Data Model document](../../../about/data-model.md#nested-keys).

### Rate Limiting

Vector offers a few levers to control the rate and volume of requests. We recommend starting with the `rate_limit_duration` and `rate_limit_num` options to ensure Vector does not exceed the specified number of requests in the specified window. You can further control the pace at which this window is saturated with the `request_in_flight_limit` option, which will guarantee no more than the specified number of requests are in-flight at any given time.

### Retry Policy

Vector will retry failed requests \(status `== 429`, `>= 500`, and `!= 501`\). Other responses will not be retried. You can control the number of retry attempts and backoff rate with the `retry_attempts` and `retry_backoff_secs` options.

#### Partial Failures

It is possible for a bulk request to partially fail \(some records succeed, some don't\). You can read more about this in the [Elasticsearch `_bulk` docs](https://www.elastic.co/guide/en/elasticsearch/reference/current/docs-bulk.html). Vector will not attempt to retry partial failures, it will only retry entire failed requests as described in the [Retries](elasticsearch.md#retries) section.

### Timeouts

The default `request_timeout_secs` is based on Elasticsearch's default bulk endpoint timeout. It highly recommended that you do not lower this, unless you have also lowered the Elasticsearch bulk endpoint timeout, as this could create orphaned requests and pile on retries.

## Resources

* [Source code](https://github.com/timberio/vector/blob/master/src/sinks/elasticsearch.rs)
* [Issues](https://github.com/timberio/vector/labels/Sink%3A%20elasticsearch)

